//! Runs bundles with node and records what they do, so two bundlers' outputs can be compared
//! by behaviour instead of by bytes.
//!
//! Browser globals (`document`, `localStorage`, `fetch`, …) are recording stand-ins: every
//! property read, write, call and construction is logged, as is every `console` call. ESM
//! imports that do not resolve (externals) get a recording module with the imported names;
//! CommonJS output runs with node's `require`. A script that throws logs the error and stops.
//!
//! Code that only runs later runs too: after the script, the functions it exposes (new
//! globals, exports) and the callbacks it handed to a stand-in (`addEventListener`, `forEach`,
//! `then`) are called once each, in order, with stand-in arguments, and what they return or
//! throw is logged. The log is capped, so a loop over stand-ins (`while (el.parentNode)`)
//! ends.

use std::path::{Path, PathBuf};
use std::process::Command;

/// `node` on `PATH`, or `None` (with a note on stderr).
pub fn node(test: &str) -> Option<PathBuf> {
    let found = std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join(if cfg!(windows) { "node.exe" } else { "node" }))
            .find(|f| f.is_file())
    });
    if found.is_none() {
        eprintln!("SKIPPED {test}: no node on PATH");
    }
    found
}

const HARNESS: &str = r#"import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { createRequire, register } from 'node:module';
import { pathToFileURL } from 'node:url';

const [file, format] = process.argv.slice(2);
const log = [];
const LIMIT = 4000;
class Cap extends Error {}
const out = (s) => {
  log.push(s);
  if (log.length > LIMIT) throw new Cap('log cap');
};
const NAME = Symbol('recorder');
// Later work: [label, function] pairs, run after the script.
const pending = [];
const later = (label, fn) => { if (pending.length < 500) pending.push([label, fn]); };
function show(v, depth = 0) {
  if (v === undefined) return 'undefined';
  if (v === null) return 'null';
  if (typeof v === 'function') return v[NAME] ?? 'fn';
  if (typeof v === 'symbol') return v.toString();
  if (typeof v === 'bigint') return `${v}n`;
  if (typeof v !== 'object') return JSON.stringify(v);
  if (depth > 4) return '...';
  if (Array.isArray(v)) return `[${v.map((x) => show(x, depth + 1)).join(',')}]`;
  const keys = Object.keys(v).sort();
  return `{${keys.map((k) => `${k}:${show(v[k], depth + 1)}`).join(',')}}`;
}
function recorder(name) {
  return new Proxy(function () {}, {
    get(_, k) {
      if (k === NAME) return name;
      // As a string a stand-in is its name; as a number, 0.
      if (k === Symbol.toPrimitive) return (hint) => (hint === 'number' ? 0 : `[${name}]`);
      if (typeof k === 'symbol' || k === 'then' || k === 'toJSON') return undefined;
      out(`get ${name}.${k}`);
      return recorder(`${name}.${k}`);
    },
    set(_, k, v) { out(`set ${name}.${String(k)} = ${show(v)}`); return true; },
    apply(_, __, args) {
      out(`call ${name}(${args.map((a) => show(a)).join(', ')})`);
      args.forEach((a, i) => { if (typeof a === 'function' && !a[NAME]) later(`${name}#${i}`, a); });
      return recorder(`${name}()`);
    },
    construct(_, args) { out(`new ${name}(${args.map((a) => show(a)).join(', ')})`); return recorder(`new ${name}`); },
  });
}
globalThis.__ssgRecorder = recorder;
const rec = {};
for (const m of ['log', 'info', 'warn', 'error', 'debug']) {
  rec[m] = (...a) => out(`console.${m}(${a.map((x) => show(x)).join(', ')})`);
}
globalThis.console = rec;
for (const g of ['document', 'localStorage', 'sessionStorage', 'navigator', 'location', 'history',
  'fetch', 'matchMedia', 'requestAnimationFrame', 'setTimeout', 'setInterval', 'addEventListener',
  'getComputedStyle', 'IntersectionObserver', 'MutationObserver', 'customElements', 'HTMLElement',
  'Element', 'Event', 'CustomEvent', 'XMLHttpRequest', 'jQuery', '$']) {
  // Some (navigator) are getters on node's global.
  Object.defineProperty(globalThis, g, { value: recorder(g), writable: true, configurable: true });
}
for (const g of ['window', 'self']) {
  Object.defineProperty(globalThis, g, { value: globalThis, writable: true, configurable: true });
}

const HOOKS = `
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
const esc = (s) => s.replace(/[.*+?^\${}()|[\\]\\\\]/g, '\\\\$&');
export async function resolve(spec, ctx, next) {
  try { return await next(spec, ctx); } catch (e) {
    if (/^[./]/.test(spec) || /^(file|node|data):/.test(spec)) throw e;
    const src = ctx.parentURL ? fs.readFileSync(fileURLToPath(ctx.parentURL), 'utf8') : '';
    const names = new Set();
    const re = new RegExp('import\\\\s*([\\\\w$]+)?\\\\s*,?\\\\s*(?:\\\\{([^}]*)\\\\}|\\\\*\\\\s*as\\\\s*[\\\\w$]+)?\\\\s*from\\\\s*["\\']' + esc(spec) + '["\\']', 'g');
    for (const m of src.matchAll(re)) {
      for (const part of (m[2] || '').split(',')) {
        const n = part.trim().split(/\\s+as\\s+/)[0];
        if (n && n !== 'default') names.add(n);
      }
    }
    return { url: 'stub:' + encodeURIComponent(JSON.stringify({ spec, names: [...names] })), shortCircuit: true };
  }
}
export async function load(url, ctx, next) {
  if (!url.startsWith('stub:')) return next(url, ctx);
  const { spec, names } = JSON.parse(decodeURIComponent(url.slice(5)));
  const lines = ['const r = globalThis.__ssgRecorder(' + JSON.stringify('import:' + spec) + ');', 'export default r;'];
  for (const n of names) lines.push('export const ' + n + ' = r[' + JSON.stringify(n) + '];');
  return { format: 'module', source: lines.join('\\n'), shortCircuit: true };
}`;

// What the script adds to the global object is what it exposes.
const before = new Set(Object.getOwnPropertyNames(globalThis));
const thrown = (e) => `${e?.constructor?.name}: ${e?.message}`;
const exposed = (label, value) => {
  for (const k of Object.keys(value).sort()) {
    out(`${label} ${k} = ${show(value[k])}`);
    if (typeof value[k] === 'function' && !value[k][NAME]) later(`${label} ${k}`, value[k]);
  }
};
try {
  try {
    if (format === 'esm') {
      register('data:text/javascript,' + encodeURIComponent(HOOKS), import.meta.url);
      const ns = await import(pathToFileURL(path.resolve(file)).href);
      exposed('export', ns);
    } else if (format === 'cjs') {
      const module = { exports: {} };
      const code = fs.readFileSync(file, 'utf8');
      const wrapped = vm.runInThisContext(`(function (exports, require, module) {${code}\n})`, { filename: file });
      wrapped(module.exports, createRequire(path.resolve(file)), module);
      exposed('export', module.exports);
    } else {
      vm.runInThisContext(fs.readFileSync(file, 'utf8'), { filename: file });
    }
  } catch (e) {
    if (e instanceof Cap) throw e;
    out(`throw ${thrown(e)}`);
  }
  const globals = {};
  for (const k of Object.getOwnPropertyNames(globalThis)) {
    if (!before.has(k)) globals[k] = globalThis[k];
  }
  exposed('global', globals);
  while (pending.length) {
    const [label, fn] = pending.shift();
    try {
      let r = fn(recorder(`${label}(a)`), recorder(`${label}(b)`));
      if (r instanceof Promise) r = await r;
      out(`${label}() -> ${show(r)}`);
    } catch (e) {
      if (e instanceof Cap) throw e;
      out(`${label}() throws ${thrown(e)}`);
    }
  }
} catch (e) {
  if (!(e instanceof Cap)) throw e;
  out('(log cap)');
}
process.stdout.write(log.join('\n') + '\n');
"#;

/// What `code` (a bundle in `format`: `iife`, `cjs` or `esm`) does when run, one line per
/// action. `name` names the script file in `dir`.
pub fn trace(node: &Path, dir: &Path, name: &str, code: &[u8], format: &str) -> String {
    let harness = dir.join("harness.mjs");
    if !harness.is_file() {
        std::fs::write(&harness, HARNESS).expect("harness");
    }
    let ext = if format == "esm" { "mjs" } else { "js" };
    let script = dir.join(format!("{name}.{ext}"));
    std::fs::write(&script, code).expect("script");
    let mut child = Command::new(node)
        .arg(&harness)
        .arg(&script)
        .arg(format)
        .current_dir(dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("running node");
    // A script stuck outside the log cap (a loop that touches no stand-in) must not hang the
    // test.
    let start = std::time::Instant::now();
    while child.try_wait().expect("node").is_none() {
        if start.elapsed() > std::time::Duration::from_secs(30) {
            let _ = child.kill();
            panic!("node ran {name} for 30 s");
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let out = child.wait_with_output().expect("node output");
    // The harness catches what the script throws: a failure is the harness's own.
    assert!(
        out.status.success(),
        "the harness failed on {name}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

// Shared by the CMS editor (in the browser) and its API (the Cloudflare Worker): globs, the paths
// the CMS may write, commit trailers and base64.

/** A part of the project the CMS may write (`crates/cms/src/paths.rs`). */
export interface Area {
  kind: string;
  glob: string;
  /** Allowed extensions, lower case, without the dot. */
  ext: string[];
}

/** The areas and the deny globs (lower case, matched against the lower-cased path). */
export interface Limits {
  areas: Area[];
  deny: string[];
}

type GlobNode =
  | { k: "literal"; c: string }
  | { k: "any" }
  | { k: "single" }
  | { k: "super" }
  | { k: "alternatives"; alts: GlobNode[][] }
  | { k: "range"; negated: boolean; lo: string; hi: string }
  | { k: "list"; negated: boolean; list: string[] };

/** The glob syntax of the site generator (`ssg_base::glob`), case-sensitive, `/` separated:
 * `*` (not `/`), `**` (anything), `?`, `[abc]`/`[!abc]`, `[a-z]`/`[!a-z]`, `{a,b}`, `\x`. */
export function compileGlob(pattern: string): RegExp {
  const chars = Array.from(pattern);
  let pos = 0;
  const fail = (what: string) => new Error(`glob ${JSON.stringify(pattern)}: ${what}`);

  function sequence(inAlternatives: boolean): GlobNode[] {
    const seq: GlobNode[] = [];
    while (pos < chars.length) {
      const c = chars[pos];
      if (inAlternatives && (c === "," || c === "}")) break;
      pos++;
      if (c === "{") {
        const alts: GlobNode[][] = [];
        for (;;) {
          alts.push(sequence(true));
          const next = chars[pos];
          if (next !== undefined) pos++;
          if (next !== ",") break;
        }
        seq.push({ k: "alternatives", alts });
      } else if (c === "[") {
        seq.push(charClass());
      } else if (c === "?") {
        seq.push({ k: "single" });
      } else if (c === "*" && chars[pos] === "*") {
        pos++;
        seq.push({ k: "super" });
      } else if (c === "*") {
        seq.push({ k: "any" });
      } else if (c === "\\") {
        if (pos < chars.length) seq.push({ k: "literal", c: chars[pos++] });
      } else {
        seq.push({ k: "literal", c });
      }
    }
    return seq;
  }

  function charClass(): GlobNode {
    const end = () => fail("unexpected end of pattern in a character class");
    const negated = chars[pos] === "!";
    if (negated) pos++;
    if (pos >= chars.length) throw end();
    const first = chars[pos];
    if (chars[pos + 1] === "-") {
      pos += 2;
      if (pos >= chars.length) throw end();
      const hi = chars[pos++];
      if (pos >= chars.length) throw end();
      if (chars[pos++] !== "]") throw fail("a character class holds one range or one list, then ']'");
      if (first === "\0" || hi === "\0") throw fail("empty character class");
      if ((hi.codePointAt(0) ?? 0) < (first.codePointAt(0) ?? 0)) throw fail(`range ${first}-${hi} is reversed`);
      return { k: "range", negated, lo: first, hi };
    }
    const list: string[] = [];
    for (;;) {
      if (pos >= chars.length) throw end();
      const c = chars[pos++];
      if (c === "]") break;
      if (c === "\\") {
        if (pos < chars.length) list.push(chars[pos++]);
      } else {
        list.push(c);
      }
    }
    if (list.length === 0) throw fail("empty character class");
    return { k: "list", negated, list };
  }

  const escape = (c: string) => (/[\\^$.*+?()[\]{}|/]/.test(c) ? `\\${c}` : c);
  const classChar = (c: string) => (/[\\\][^-]/.test(c) ? `\\${c}` : c);
  function emit(seq: GlobNode[]): string {
    let re = "";
    for (const n of seq) {
      switch (n.k) {
        case "literal":
          re += escape(n.c);
          break;
        case "any":
          re += "[^/]*";
          break;
        case "single":
          re += "[^/]";
          break;
        case "super":
          re += ".*";
          break;
        case "alternatives":
          re += `(?:${n.alts.map(emit).join("|")})`;
          break;
        case "range":
          re += `[${n.negated ? "^" : ""}${classChar(n.lo)}-${classChar(n.hi)}]`;
          break;
        case "list":
          re += `[${n.negated ? "^" : ""}${n.list.map(classChar).join("")}]`;
          break;
      }
    }
    return re;
  }

  return new RegExp(`^(?:${emit(sequence(false))})$`, "su");
}

const globs = new Map<string, RegExp>();

/** Whether `path` matches the whole of `pattern` (compiled once). */
export function matchGlob(pattern: string, path: string): boolean {
  let re = globs.get(pattern);
  if (!re) {
    re = compileGlob(pattern);
    globs.set(pattern, re);
  }
  return re.test(path);
}

/** A project-relative path of plain segments, or null: well-formed Unicode, no leading `/`,
 * `\`, control characters, empty, `.` or `..` segments, and no hidden files or directories. */
export function cleanPath(path: unknown): string | null {
  if (typeof path !== "string" || path.length === 0 || path.length > 1024) return null;
  if (!path.isWellFormed()) return null;
  if (path.startsWith("/") || path.includes("\\") || /[\u0000-\u001f\u007f]/.test(path)) return null;
  for (const seg of path.split("/")) {
    if (seg === "" || seg === "." || seg === ".." || seg.startsWith(".")) return null;
  }
  return path;
}

/** The lower-case extension of the last segment, without the dot (`""` when it has none). */
export function extensionOf(path: string): string {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/** A path as the site generator keys content: lower case, spaces as `-`. */
export const normalizedKey = (path: string): string => path.toLowerCase().replaceAll(" ", "-");

/** The area of the limits that `path` is in (a clean path, not denied, with an extension the
 * area allows), or null. The deny globs (lower case) match the path as the build keys it, so
 * `_Content.HTML` is the adapter `_content.html` it is to the build. */
export function areaOf(limits: Limits, path: unknown): Area | null {
  const clean = cleanPath(path);
  if (clean === null) return null;
  const key = normalizedKey(clean);
  if (limits.deny.some((g) => matchGlob(g, key))) return null;
  const ext = extensionOf(clean);
  return limits.areas.find((a) => a.ext.includes(ext) && matchGlob(a.glob, clean)) ?? null;
}

/** Whether globs `edit` (a user's, from their roles) let them write `path`. */
export function mayEdit(limits: Limits, edit: string[], path: unknown): boolean {
  return areaOf(limits, path) !== null && edit.some((g) => matchGlob(g, path as string));
}

/** The trailers of a commit message (`CMS-Entry: x` → `{"cms-entry": "x"}`), from its last
 * paragraph. */
export function parseTrailers(message: string | null | undefined): Record<string, string> {
  const out: Record<string, string> = {};
  const paragraphs = String(message ?? "").trim().split(/\n[ \t]*\n/);
  for (const line of paragraphs[paragraphs.length - 1].split("\n")) {
    const m = /^([A-Za-z][A-Za-z0-9-]*):[ \t]*(.*)$/.exec(line.trim());
    if (m && !(m[1].toLowerCase() in out)) out[m[1].toLowerCase()] = m[2];
  }
  return out;
}

/** A commit message: the subject, then the trailers (`[name, value]` pairs; empty values left
 * out). */
export function commitMessage(subject: string, trailers: [string, string | null | undefined][]): string {
  const one = (s: string) => s.replace(/\s+/g, " ").trim();
  const lines = trailers.filter((t): t is [string, string] => t[1] !== undefined && t[1] !== null && one(t[1]) !== "");
  return `${one(subject)}\n\n${lines.map(([k, v]) => `${k}: ${one(v)}`).join("\n")}\n`;
}

/** Bytes as base64. */
export function bytesToBase64(bytes: Uint8Array): string {
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000) {
    s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(s);
}

/** Base64 (whitespace ignored) as bytes. */
export function base64ToBytes(b64: string): Uint8Array {
  const s = atob(b64.replace(/\s+/g, ""));
  const out = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
  return out;
}

/** Text as UTF-8 base64, and back. */
export const textToBase64 = (text: string): string => bytesToBase64(new TextEncoder().encode(text));
export const base64ToText = (b64: string): string => new TextDecoder().decode(base64ToBytes(b64));

/** The decoded size of base64 text, in bytes. */
export function base64Size(b64: string): number {
  const s = b64.replace(/\s+/g, "");
  const pad = s.endsWith("==") ? 2 : s.endsWith("=") ? 1 : 0;
  return Math.floor((s.length * 3) / 4) - pad;
}

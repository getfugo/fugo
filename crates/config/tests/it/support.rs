//! Shared helpers: oracle cases recreated on disk, and a projection of a [`Config`] onto the
//! shape of the oracle's `config` command dumps (lower-case keys, zero values omitted).

use std::path::{Path, PathBuf};

use serde_json::{Map as JMap, Value as J, json};
use ssg_config::global::MaxAge;
use ssg_config::output::{Escaping, LinkPolicy, Listing, Placement, UglyPolicy};
use ssg_config::site::UglyUrls;
use ssg_config::{CliOverrides, Config, LoadOptions, SiteConfig};

mod dump;

pub use dump::*;

/// Reads a fixture under `testdata` as raw JSON.
pub fn fixture(rel: &str) -> J {
    ssg_testkit::fixture::oracle(rel)
}

/// An oracle case recreated in a temporary directory.
pub struct Site {
    pub tmp: tempfile::TempDir,
    pub options: LoadOptions,
}

impl Site {
    /// `$ROOT` of the oracle's paths.
    pub fn root(&self) -> &Path {
        self.tmp.path()
    }

    /// An oracle path or value: `$ROOT` expanded, and Go's default cache directory names
    /// ([`GO_CACHE_DIR`], also with `_<user>`) as this port names them.
    pub fn expand(&self, s: &str) -> String {
        s.replace("$ROOT", &self.root().to_string_lossy())
            .replace(GO_CACHE_DIR, &format!("/{}_cache", ssg_base::APP_NAME))
    }
}

/// The Go program's default cache directory, as the oracle recorded it.
const GO_CACHE_DIR: &str = "/hugo_cache";

/// The prefix of the Go program's environment variables, as the oracle recorded them.
const GO_ENV_PREFIX: &str = "HUGO";

/// Why a case cannot be expressed with this crate's API.
pub type NotApplicable = &'static str;

/// Recreates the case's files and builds the load options (`site_dir` is the project
/// directory's name under `$ROOT`).
pub fn materialize(case: &J, site_dir: &str) -> Result<Site, NotApplicable> {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path().to_path_buf();
    let expand = |s: &str| s.replace("$ROOT", &root.to_string_lossy());
    let dir = root.join(site_dir);
    std::fs::create_dir_all(&dir).expect("site dir");
    // The Go program's configuration files are our `config.*` (`fixture::local_path`).
    for (name, content) in case["files"].as_object().expect("files") {
        let path = dir.join(ssg_testkit::fixture::local_path(name));
        if name.ends_with('/') {
            std::fs::create_dir_all(&path).expect("dir");
        } else {
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
            std::fs::write(&path, expand(content.as_str().expect("text"))).expect("write");
        }
    }
    // The oracle recorded the Go program's environment variables ([`GO_ENV_PREFIX`]); the same
    // settings are read with this program's prefix (`ssg_base::ENV_PREFIX`), and none under the
    // Go names.
    let renamed = |k: &str| match k.strip_prefix(GO_ENV_PREFIX) {
        Some(rest) => format!("{}{rest}", ssg_base::ENV_PREFIX),
        None => k.to_owned(),
    };
    let mut env: Vec<(String, String)> = Vec::new();
    let mut vars: Vec<(&str, String)> = Vec::new();
    if let Some(p) = case["procEnv"].as_object() {
        for (k, v) in p {
            vars.push((k, expand(v.as_str().unwrap_or_default())));
        }
    }
    for e in case["environ"].as_array().into_iter().flatten() {
        if let Some((k, v)) = e.as_str().unwrap_or_default().split_once('=') {
            vars.push((k, expand(v)));
        }
    }
    for (k, v) in vars {
        let name = renamed(k);
        if ssg_config::env::is_read(&name) {
            env.push((name, v));
        } else if name.starts_with(ssg_base::ENV_PREFIX) && !name.ends_with("_ORACLE") {
            // Go read settings from the environment; this port reads files and flags only.
            return Err("a setting from the environment");
        }
    }
    // The oracle passes "production" when no environment was chosen; Go's environment variable
    // could then decide (such cases are not applicable: only the flag chooses here).
    let mut cli = CliOverrides {
        environment: case["environment"]
            .as_str()
            .filter(|e| *e != "production")
            .map(str::to_owned),
        ..CliOverrides::default()
    };
    if let Some(d) = case["configDir"].as_str().filter(|s| !s.is_empty()) {
        cli.config_dir = Some(d.into());
    }
    if let Some(flags) = case["flags"].as_object() {
        for (k, v) in flags {
            let s = || expand(v.as_str().unwrap_or_default());
            match k.as_str() {
                "baseURL" => cli.base_url = Some(s()),
                "buildDrafts" => cli.build_drafts = v.as_bool(),
                "cacheDir" => cli.cache_dir = Some(s().into()),
                "minify" | "minifyOutput" => cli.minify = v.as_bool(),
                "publishDir" => cli.destination = Some(s().into()),
                "themesDir" => cli.themes_dir = Some(s().into()),
                "internal.clock" => {} // the clock is not configuration
                _ => return Err("a flag the CLI does not have"),
            }
        }
    }
    let mut config_files = Vec::new();
    if let Some(f) = case["filename"].as_str() {
        for name in expand(f).split(',') {
            config_files.push(PathBuf::from(name));
        }
    }
    Ok(Site {
        options: LoadOptions {
            source: dir.clone(),
            config_files,
            cli,
            env,
        },
        tmp,
    })
}

/// This port runs no external CSS tools (PostCSS, Tailwind): its defaults drop the Go
/// program's entries for them, which the recorded dumps still have.
const GO_TOOL_ALLOW: [&str; 2] = ["^postcss$", "^tailwindcss$"];
const GO_CACHE_BUSTER: (&str, &str) = (r"(postcss|tailwind)\.config\.js", "(css|styles|scss|sass)");

/// `security.exec.allow` as Go has it: the default list with the Go program's tool entries
/// appended.
fn exec_allow_as_go(w: &ssg_config::global::Whitelist) -> J {
    match wl(w) {
        J::Array(mut p)
            if J::Array(p.clone()) == wl(&ssg_config::SecurityPolicy::default().exec_allow) =>
        {
            p.extend(GO_TOOL_ALLOW.map(|t| json!(t)));
            J::Array(p)
        }
        ours => ours,
    }
}

/// `[build]` as Go has it: the default cache busters are the Go program's one for the tools'
/// configuration files (this port's default has none).
fn build_as_go(b: &ssg_config::BuildConfig) -> J {
    let mut v = serde_json::to_value(b).expect("json");
    if b.cache_busters.is_empty() {
        v["cacheBusters"] = json!([{"source": GO_CACHE_BUSTER.0, "target": GO_CACHE_BUSTER.1}]);
    }
    v
}

/// `security.funcs.getenv` as Go spells it: our default `^FUGO_` is Go's default (the Go
/// program's environment variable prefix) renamed.
fn getenv_as_go(w: &ssg_config::global::Whitelist) -> J {
    match wl(w) {
        J::Array(p) => J::Array(
            p.into_iter()
                .map(|p| {
                    if p == "^FUGO_" {
                        json!(format!("^{GO_ENV_PREFIX}_"))
                    } else {
                        p
                    }
                })
                .collect(),
        ),
        other => other,
    }
}

/// A whitelist as configured: `"none"` or the patterns.
fn wl(w: &ssg_config::global::Whitelist) -> J {
    match w.patterns() {
        [one] if one.eq_ignore_ascii_case("none") => json!("none"),
        p => json!(p),
    }
}

fn markup(s: &SiteConfig) -> J {
    let m = &s.markup;
    let g = &m.goldmark;
    let e = &g.extensions;
    let t = &e.typographer;
    let h = &m.highlight;
    let toggle = |x: &ssg_config::markup::Toggle| json!({"enable": x.enable});
    json!({
        "defaultmarkdownhandler": m.default_markdown_handler,
        "asciidocext": serde_json::to_value(&m.asciidoc_ext).expect("json"),
        "goldmark": {
            "duplicateresourcefiles": g.duplicate_resource_files,
            "extensions": {
                "typographer": {
                    "disable": t.disable, "leftsinglequote": t.left_single_quote,
                    "rightsinglequote": t.right_single_quote, "leftdoublequote": t.left_double_quote,
                    "rightdoublequote": t.right_double_quote, "endash": t.en_dash, "emdash": t.em_dash,
                    "ellipsis": t.ellipsis, "leftanglequote": t.left_angle_quote,
                    "rightanglequote": t.right_angle_quote, "apostrophe": t.apostrophe,
                },
                "footnote": e.footnote.enable,
                "definitionlist": e.definition_list, "table": e.table,
                "strikethrough": e.strikethrough, "linkify": e.linkify,
                "linkifyprotocol": e.linkify_protocol, "tasklist": e.task_list,
                "passthrough": {"enable": e.passthrough.enable, "delimiters": {
                    "inline": e.passthrough.delimiters.inline, "block": e.passthrough.delimiters.block}},
                "cjk": {"enable": e.cjk.enable, "eastasianlinebreaks": e.cjk.east_asian_line_breaks,
                        "eastasianlinebreaksstyle": e.cjk.east_asian_line_breaks_style,
                        "escapedspace": e.cjk.escaped_space},
                "extras": {"delete": toggle(&e.extras.delete), "insert": toggle(&e.extras.insert),
                           "mark": toggle(&e.extras.mark), "subscript": toggle(&e.extras.subscript),
                           "superscript": toggle(&e.extras.superscript)},
            },
            "parser": {
                "autoheadingid": g.parser.auto_heading_id,
                "autoidtype": g.parser.auto_id_type,
                "autodefinitiontermid": g.parser.auto_definition_term_id,
                "wrapstandaloneimagewithinparagraph": g.parser.wrap_standalone_image_within_paragraph,
                "attribute": {"title": g.parser.attribute.title, "block": g.parser.attribute.block},
            },
            "renderer": {"hardwraps": g.renderer.hard_wraps, "xhtml": g.renderer.xhtml,
                         "unsafe": g.renderer.unsafe_html},
            "renderhooks": {"image": {"useembedded": g.render_hooks.image.use_embedded},
                            "link": {"useembedded": g.render_hooks.link.use_embedded}},
        },
        "highlight": {
            "style": h.style, "codefences": h.code_fences, "noclasses": h.no_classes,
            "linenos": h.line_nos, "linenumbersintable": h.line_numbers_in_table,
            "linenostart": h.line_no_start, "anchorlinenos": h.anchor_line_nos,
            "lineanchors": h.line_anchors, "hl_lines": h.hl_lines, "hl_inline": h.hl_inline,
            "tabwidth": h.tab_width, "guesssyntax": h.guess_syntax, "wrapperclass": h.wrapper_class,
        },
        "tableofcontents": {"startlevel": m.table_of_contents.start_level,
                            "endlevel": m.table_of_contents.end_level.map_or(-1, i16::from),
                            "ordered": m.table_of_contents.ordered},
    })
}

/// Tallies checks and collects mismatches.
#[derive(Default)]
pub struct Tally {
    pub checks: usize,
    pub passed: usize,
    pub failures: Vec<String>,
}

impl Tally {
    /// Records one comparison.
    pub fn check(&mut self, ok: bool, what: impl FnOnce() -> String) {
        self.checks += 1;
        if ok {
            self.passed += 1;
        } else {
            self.failures.push(what());
        }
    }
}

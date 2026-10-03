//! The docs corpus: every piece of code the legacy docs site highlights.
//!
//! - fenced code blocks of `testdata/legacy-docs/content/**/*.md`, read through ssg-markup, with the
//!   language the docs `render-codeblock` hook passes to `transform.Highlight` (`html` and
//!   `gotmpl` → `go-html-template`, `md` → `text`, else the `file` attribute's extension or
//!   `text`) and the fence's options; `goat` fences go to the goat hook instead;
//! - the inner TOML/YAML/JSON of `code-toggle` shortcodes (the shortcode highlights it after
//!   converting it to the three formats; here in the format it is written in);
//! - `highlight` shortcodes (language and option string) and `hl` shortcodes (inline, no
//!   classes).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ssg_base::id::PageId;
use ssg_base::{Map, Value};
use ssg_markup::{
    CodeBlockCtx, ExpandedMarkdown, HookEnv, HookError, HookOut, Hooks, MarkdownOptions,
    SourceContexts,
};

/// Where a piece of code comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    Fence,
    CodeToggle,
    HighlightShortcode,
    HlShortcode,
}

/// One piece of code the docs highlight.
#[derive(Clone, Debug)]
pub struct Item {
    pub file: String,
    pub source: Source,
    pub lang: String,
    pub code: String,
    /// Fence options (a map) or a shortcode's option string.
    pub options: Opts,
    /// A stable key of the tokens: FNV-1a of `lang`, NUL, `code`.
    pub key: String,
    /// A stable key of the HTML: FNV-1a of `lang`, NUL, `code`, NUL, the options as JSON.
    pub html_key: String,
}

#[derive(Clone, Debug)]
pub enum Opts {
    Map(Map),
    Str(String),
}

impl Opts {
    /// The options as JSON: an object, or the option string.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Map(m) => serde_json::to_value(m).expect("options"),
            Self::Str(s) => serde_json::Value::String(s.clone()),
        }
    }
}

/// 64-bit FNV-1a of the parts joined by NUL, as 16 hex digits.
pub fn fnv(parts: &[&str]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for (i, part) in parts.iter().enumerate() {
        let sep: &[u8] = if i == 0 { &[] } else { &[0] };
        for &b in sep.iter().chain(part.as_bytes()) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

fn item(file: &str, source: Source, lang: &str, code: String, options: Opts) -> Item {
    let opts = options.to_json().to_string();
    Item {
        key: fnv(&[lang, &code]),
        html_key: fnv(&[lang, &code, &opts]),
        file: file.to_owned(),
        source,
        lang: lang.to_owned(),
        code,
        options,
    }
}

/// Collects fenced code blocks.
#[derive(Default)]
struct Fences(Mutex<Vec<CodeBlockCtx>>);

impl Hooks for Fences {
    fn code_block(&self, _: &HookEnv, ctx: &CodeBlockCtx) -> Result<HookOut, HookError> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(ctx.clone());
        Ok(HookOut::Html(String::new()))
    }
}

/// The content of the legacy docs site (`testdata/legacy-docs/content`).
pub fn docs_content() -> PathBuf {
    ssg_testkit::fixture::legacy_docs().join("content")
}

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            markdown_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "md") {
            out.push(p);
        }
    }
}

/// The body after front matter.
fn body(src: &str) -> &str {
    for delim in ["---", "+++"] {
        if let Some(rest) = src.strip_prefix(delim).and_then(|r| r.strip_prefix('\n'))
            && let Some(end) = rest.find(&format!("\n{delim}\n"))
        {
            return &rest[end + delim.len() + 2..];
        }
    }
    src
}

/// Shortcode escapes in examples (`{{</* x */>}}`) as Go leaves them after expansion.
fn unescape_shortcodes(s: &str) -> String {
    s.replace("{{</*", "{{<")
        .replace("*/>}}", ">}}")
        .replace("{{%/*", "{{%")
        .replace("*/%}}", "%}}")
}

/// The docs render-codeblock hook's language.
fn docs_lang(ctx: &CodeBlockCtx) -> String {
    let ext = ctx
        .attributes
        .get("file")
        .and_then(Value::as_str)
        .and_then(|f| Path::new(f).extension())
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let lang = if !ctx.lang.is_empty() {
        ctx.lang.as_str()
    } else if !ext.is_empty() {
        ext
    } else {
        "text"
    };
    match lang {
        "html" | "gotmpl" => "go-html-template".to_owned(),
        "md" => "text".to_owned(),
        l => l.to_owned(),
    }
}

/// The inner text of every `{{< name … >}}…{{< /name >}}` pair: `(args, inner)`.
fn shortcodes<'a>(md: &'a str, name: &str) -> Vec<(&'a str, &'a str)> {
    let open = format!("{{{{< {name}");
    let mut out = Vec::new();
    let mut rest = md;
    while let Some(at) = rest.find(&open) {
        let after = &rest[at + open.len()..];
        if !after.starts_with([' ', '>']) {
            rest = after;
            continue;
        }
        let Some(end_tag) = after.find(">}}") else {
            break;
        };
        let args = &after[..end_tag];
        let inner_start = &after[end_tag + 3..];
        if args.trim_end().ends_with('/') {
            rest = inner_start;
            continue;
        }
        let Some((end, close_len)) = closing(inner_start, name) else {
            rest = inner_start;
            continue;
        };
        out.push((args.trim(), &inner_start[..end]));
        rest = &inner_start[end + close_len..];
    }
    out
}

/// The first `{{< /name >}}` (any spacing): its offset and length.
fn closing(s: &str, name: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some(at) = s[from..].find("{{<") {
        let start = from + at;
        let tag = s[start + 3..].trim_start();
        if let Some(tag) = tag.strip_prefix('/')
            && let Some(tag) = tag.trim_start().strip_prefix(name)
            && let Some(end) = tag.trim_start().strip_prefix(">}}")
        {
            return Some((start, s.len() - end.len() - start));
        }
        from = start + 3;
    }
    None
}

fn toggle_format(inner: &str) -> &'static str {
    let t = inner.trim_start();
    if t.starts_with('{') {
        "json"
    } else if t.lines().any(|l| {
        let l = l.trim();
        l.starts_with('[') || l.contains(" = ") || l.contains("= ")
    }) {
        "toml"
    } else {
        "yaml"
    }
}

/// Every piece of code the docs highlight, in file order.
pub fn load() -> Vec<Item> {
    let mut files = Vec::new();
    let root = docs_content();
    markdown_files(&root, &mut files);
    assert!(
        files.len() > 900,
        "docs content not found at {}",
        root.display()
    );
    let options = MarkdownOptions::default();
    let mut items = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(&root)
            .map_or_else(|_| path.display().to_string(), |p| p.display().to_string());
        let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let md = body(&src);
        let fences = Fences::default();
        let contexts = SourceContexts::default();
        let file: Arc<Path> = Arc::from(Path::new(&rel));
        let input = ExpandedMarkdown {
            text: md,
            page: PageId::from_raw(0),
            contexts: &contexts,
            file: &file,
        };
        ssg_markup::render(&input, &options, &fences, None)
            .unwrap_or_else(|e| panic!("{rel}: {e}"));
        for ctx in fences.0.into_inner().unwrap_or_default() {
            if ctx.lang == "goat" {
                continue;
            }
            let lang = docs_lang(&ctx);
            let code = unescape_shortcodes(ctx.inner.trim());
            items.push(item(
                &rel,
                Source::Fence,
                &lang,
                code,
                Opts::Map(ctx.options),
            ));
        }
        for (args, inner) in shortcodes(md, "code-toggle") {
            if args.contains("config=") || args.contains("dataKey=") {
                continue;
            }
            let lang = toggle_format(inner);
            let code = inner.trim().to_owned();
            items.push(item(
                &rel,
                Source::CodeToggle,
                lang,
                code,
                Opts::Str(String::new()),
            ));
        }
        for (args, inner) in shortcodes(md, "highlight") {
            let mut parts = args.splitn(2, ' ');
            let lang = parts.next().unwrap_or("");
            let opts = parts.next().unwrap_or("").trim().trim_matches('"');
            let code = inner.trim_matches('\n').to_owned();
            items.push(item(
                &rel,
                Source::HighlightShortcode,
                lang,
                code,
                Opts::Str(opts.to_owned()),
            ));
        }
        for (args, inner) in shortcodes(md, "hl") {
            let lang = if args.is_empty() { "go" } else { args };
            let code = inner.trim().to_owned();
            let opts = Opts::Str("hl_inline=true,noClasses=true".to_owned());
            items.push(item(&rel, Source::HlShortcode, lang, code, opts));
        }
    }
    items
}

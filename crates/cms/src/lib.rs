//! The CMS editor of `[cms]`: a page under the site (`/admin/`) where people edit content in the
//! browser, and the API it calls, which runs as a Cloudflare Worker next to the static files.
//!
//! A build with a `[cms]` table (usually in `config/production/cms.toml`, so only production
//! builds have it) writes, after the site:
//!
//! | File | What |
//! |---|---|
//! | `<path>/index.html`, `cms.js`, `cms.css` | the editor (static; `assets/admin/`) |
//! | `_worker.js` | the API: the settings and the content index ([`index`], served to signed-in people only: it lists drafts), then `assets/worker.js` ([`worker_source`]) |
//! | `.assetsignore` | keeps `_worker.js` out of the static files Cloudflare serves |
//! | `_headers` | the editor's pages may not be framed (no clickjacking of *Publish*) |
//!
//! The Worker signs editors in with Cloudflare Access (it checks the token Access adds to every
//! request), gives them the roles the `CMS_USERS` secret names, and commits as one bot to the
//! git host ([`config::Host`]: GitHub), with the editor as author. What a role may write is cut
//! down to the areas of [`paths`]. Nothing here runs a program; the JavaScript runs in the
//! browser and on Cloudflare.
//!
//! The editor and the Worker are written in TypeScript and Sass (`web/`); `tools/cms/build.sh`
//! type-checks them and builds them with the generator itself into `assets/admin/cms.js`,
//! `assets/admin/cms.css` and `assets/worker.js`, which are embedded here (a test of `ssg-cli`
//! checks that they are what `web/` builds to).

#![forbid(unsafe_code)]

pub mod config;
pub mod index;
pub mod paths;

use ssg_base::Sink;
use ssg_base::diag::Diagnostic;
use ssg_base::paths::OutputPath;
use ssg_config::Config;

pub use config::CmsConfig;

/// The editor's static files, published under `<path>/` (built from `web/`).
const ADMIN_FILES: &[(&str, &[u8])] = &[
    ("cms.js", include_bytes!("../assets/admin/cms.js")),
    ("cms.css", include_bytes!("../assets/admin/cms.css")),
];
const INDEX_HTML: &str = include_str!("../assets/admin/index.html");
/// The Worker module (built from `web/assets/worker.ts`, minified): its default export reads the
/// constants `__CMS_SETTINGS__` and `__CMS_INDEX__`, which [`worker_source`] puts before it.
const WORKER_JS: &str = include_str!("../assets/worker.js");

/// A `[cms]` setting that is wrong, or the editor's files could not be written.
#[derive(Debug, thiserror::Error)]
pub enum CmsError {
    #[error("config: {key}: {message}")]
    Config { key: String, message: String },
    #[error("cms: {0}")]
    Io(#[from] std::io::Error),
}

impl CmsError {
    pub(crate) fn config(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Config {
            key: key.into(),
            message: message.into(),
        }
    }
}

/// What [`publish`] wrote.
#[derive(Debug)]
pub struct Published {
    /// The editor's URL path (`/admin/`).
    pub path: String,
    /// Pages in the index.
    pub entries: usize,
    /// Notices: an output of the site the editor replaced, a project outside a git repository.
    pub warnings: Vec<Diagnostic>,
}

/// The configuration's `[cms]`, checked (`None` without one).
///
/// # Errors
/// See [`CmsConfig::from_tree`].
pub fn settings(cfg: &Config) -> Result<Option<CmsConfig>, CmsError> {
    CmsConfig::from_tree(cfg.raw.get("cms"))
}

/// Writes the editor, its index and the Worker into `sink`, when the configuration has `[cms]`.
///
/// # Errors
/// A wrong `[cms]` setting, or unreadable content directories or unwritable files.
pub fn publish(cfg: &Config, sink: &dyn Sink) -> Result<Option<Published>, CmsError> {
    let Some(cms) = settings(cfg)? else {
        return Ok(None);
    };
    let areas = paths::areas(cfg, &cms)?;
    for (name, role) in &cms.roles {
        if let Some(g) = role
            .edit
            .iter()
            .find(|g| !paths::reaches_an_area(g, &areas))
        {
            return Err(CmsError::config(
                format!("cms.roles.{name}.edit"),
                format!(
                    "{g:?} matches nothing the editor may write (content, data, i18n, the params and menus files of config/_default/, the media directory)"
                ),
            ));
        }
    }
    let mut warnings = Vec::new();
    let (dir, found) = paths::repo_dir(cfg, &cms);
    if !found {
        warnings.push(warning(
            "cms-no-repository",
            "cms: no .git directory above the project; the editor assumes the project is at the repository's root (set cms.git.dir to say where it is)",
        ));
    }
    let index = index::read(cfg, &cms)?;
    let entries = index.entries.len();

    let base = &cms.path;
    let index_path = OutputPath::new(&format!("{base}/index.html"));
    let page = index_html(&index.title);
    // A disk sink still has the editor of an earlier build: only another page is a conflict.
    if sink.exists(&index_path) && sink.read(&index_path).ok().as_deref() != Some(page.as_bytes()) {
        warnings.push(warning(
            "cms-path-taken",
            format!(
                "cms: the site has a page at /{base}/; the editor replaces it (set cms.path to move the editor)"
            ),
        ));
    }
    sink.write(&index_path, page.as_bytes())?;
    for (name, bytes) in ADMIN_FILES {
        sink.write(&OutputPath::new(&format!("{base}/{name}")), bytes)?;
    }
    let worker = worker_settings(cfg, &cms, &areas, &index, dir);
    let index_json = serde_json::to_string(&index).map_err(std::io::Error::other)?;
    sink.write(
        &OutputPath::new("_worker.js"),
        worker_source(&worker, &index_json).as_bytes(),
    )?;
    write_assetsignore(sink)?;
    write_headers(sink, &index.path)?;
    Ok(Some(Published {
        path: index.path,
        entries,
        warnings,
    }))
}

fn warning(id: &str, message: impl Into<String>) -> Diagnostic {
    let mut d = Diagnostic::warning(message);
    d.id = Some(id.to_owned());
    d
}

/// The editor's page, with the title escaped into it.
fn index_html(title: &str) -> String {
    let escaped = title
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    INDEX_HTML.replace("{{title}}", &escaped)
}

/// The settings the Worker is published with.
fn worker_settings(
    cfg: &Config,
    cms: &CmsConfig,
    areas: &[paths::Area],
    index: &index::Index,
    dir: String,
) -> serde_json::Value {
    let mut git = serde_json::to_value(&cms.git).unwrap_or_default();
    git["dir"] = serde_json::Value::String(dir);
    serde_json::json!({
        "version": 1,
        "path": index.path,
        "api": index.api,
        "site": cfg.default_site().base_url.as_str(),
        "workflow": cms.workflow,
        "git": git,
        "login": cms.login,
        "roles": cms.roles,
        "areas": areas,
        "deny": paths::DENY,
        "maxUpload": cms.max_upload,
    })
}

/// The published Worker: the settings and the content index (JSON text) as the constants the
/// Worker module reads, then the module.
#[must_use]
pub fn worker_source(settings: &serde_json::Value, index_json: &str) -> String {
    let settings = serde_json::to_string_pretty(settings).unwrap_or_else(|_| "{}".to_owned());
    // A JSON string is a JavaScript string literal (U+2028 and U+2029 included, since ES2019).
    let index = serde_json::to_string(index_json).unwrap_or_else(|_| "\"{}\"".to_owned());
    format!(
        "// The API of the CMS editor, written by the site's build ([cms]). Do not edit: the next\n\
         // build replaces it. Deploy it with the site (wrangler.jsonc: \"main\": \"public/_worker.js\").\n\n\
         const __CMS_SETTINGS__ = {settings};\n\nconst __CMS_INDEX__ = {index};\n\n{WORKER_JS}"
    )
}

/// Adds the editor's rule to `_headers` (Cloudflare's headers file for static files): its pages
/// may not be framed. A static `_headers` keeps its rules.
fn write_headers(sink: &dyn Sink, path: &str) -> std::io::Result<()> {
    let file = OutputPath::new("_headers");
    let rule = format!(
        "{path}*\n  X-Frame-Options: DENY\n  Content-Security-Policy: frame-ancestors 'none'\n  Referrer-Policy: same-origin\n"
    );
    let mut text = if sink.exists(&file) {
        String::from_utf8_lossy(&sink.read(&file)?).into_owned()
    } else {
        String::new()
    };
    if !text.contains(&rule) {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&rule);
    }
    sink.write(&file, text.as_bytes())
}

/// Adds `_worker.js` to `.assetsignore` (Cloudflare uploads every other file of the publish
/// directory as a static file), keeping the lines a static `.assetsignore` already has.
fn write_assetsignore(sink: &dyn Sink) -> std::io::Result<()> {
    let path = OutputPath::new(".assetsignore");
    let mut text = if sink.exists(&path) {
        String::from_utf8_lossy(&sink.read(&path)?).into_owned()
    } else {
        String::new()
    };
    for line in ["_worker.js", ".assetsignore"] {
        if !text.lines().any(|l| l.trim() == line) {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(line);
            text.push('\n');
        }
    }
    sink.write(&path, text.as_bytes())
}

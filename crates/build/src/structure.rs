//! The structure dump (REWRITE_PLAN.md §6.4, §7.2): with `FUGO_STRUCTURE_OUT=<file>` in the
//! environment, [`build`](crate::build) writes what it did in the schema of the Go structure
//! oracle (`ssg-structure/1`, `testdata/golden/README.md`), which
//! `tools/dev/structdiff.py` and our parity test compare with Go's dump of the same
//! site. Without the variable nothing is recorded.
//!
//! - `records`: every page and standalone job of wave 1 (pager 1 of a paginated page) per
//!   (language, page, output format): the target, `.RelPermalink` and `.Permalink` of that
//!   format, the layout and base template the session selected (v0.146 names;
//!   `templateFile`/`baseofFile` are `_embedded/<name>` for an embedded template), `written:
//!   false` when the template rendered nothing, and `pagers` (`.TotalPages`) when the render
//!   paginated a non-empty list. Jobs
//!   that lose a target collision are recorded too: they render (Go writes every colliding
//!   file; the last one wins).
//! - `aliases`: the front matter alias files and the language redirect (`kind: "redirect"`,
//!   `lang` and `path` empty); `pagerAliases`: the `page/1/` files. Each with the file
//!   (`from`), the page it redirects to and that page's permalink; `alias` is the file without
//!   `index.html` (the language key for the redirect, the file itself for a pager).
//! - `pages`: every page of the model (bundled pages excepted, as Go's page tree has none) with
//!   the output formats it has links in; `resources`: the bundle files in every page's
//!   resources (bundled pages excepted), per language, with `publish: false` when the page does
//!   not publish its resources.
//! - `config`: the languages and their base URLs; `layouts`: the templates of the layouts
//!   component (themes included, embedded ones not) with their category; Go's store keys and
//!   descriptors have no counterpart.
//!
//! Everything is sorted as Go sorts it, and written as its oracle writes it: one member per
//! line, one element per line for arrays of objects.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{Map, Value, json};
use ssg_base::{FormatId, PageId};
use ssg_layouts::{Origin, TemplateName, TemplateRole};
use ssg_publish::PublishError;
use ssg_render::{Job, RenderError, Session};
use ssg_site::PageRole;

use crate::BuildError;

/// The environment variable naming the dump file.
pub const ENV: &str = ssg_base::env_var!("STRUCTURE_OUT");

/// The schema of the dump (the Go oracle's).
const SCHEMA: &str = "ssg-structure/1";

/// What an alias file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AliasKind {
    FrontMatter,
    Redirect,
    Pager,
}

/// An alias file a job wrote (or lost to a collision).
#[derive(Debug)]
struct Alias {
    kind: AliasKind,
    from: String,
    page: PageId,
    format: FormatId,
}

/// What the waves did, recorded job by job (only with [`ENV`] set).
#[derive(Debug)]
pub(crate) struct Recorder {
    out: PathBuf,
    /// Per page or standalone job: whether its template rendered something.
    renders: BTreeMap<(PageId, FormatId), bool>,
    aliases: Vec<Alias>,
}

impl Recorder {
    /// A recorder when [`ENV`] names a file.
    pub(crate) fn from_env() -> Option<Self> {
        let out = std::env::var_os(ENV).filter(|v| !v.is_empty())?;
        Some(Self {
            out: PathBuf::from(out),
            renders: BTreeMap::new(),
            aliases: Vec::new(),
        })
    }

    /// Records a rendered job; `wrote`: its template rendered something.
    pub(crate) fn job(
        &mut self,
        session: &Session,
        job: &Job,
        wrote: bool,
    ) -> Result<(), RenderError> {
        let alias = |kind, page, format| -> Result<Option<Alias>, RenderError> {
            Ok(session.target(job)?.map(|from| Alias {
                kind,
                from: from.as_str().to_owned(),
                page,
                format,
            }))
        };
        let recorded = match *job {
            Job::Page { page, format } | Job::Standalone { page, format } => {
                self.renders.insert((page, format), wrote);
                None
            }
            Job::Pager { .. } => None,
            Job::Alias(ref a) => alias(AliasKind::FrontMatter, a.to, a.format)?,
            Job::PagerAlias { page, format } => alias(AliasKind::Pager, page, format)?,
            Job::LanguageRedirect => match session.language_redirect() {
                Some(a) => alias(AliasKind::Redirect, a.to, a.format)?,
                None => None,
            },
        };
        self.aliases.extend(recorded);
        Ok(())
    }

    /// Writes the dump.
    pub(crate) fn write(self, session: &Session) -> Result<(), BuildError> {
        let doc = self.dump(session);
        std::fs::write(&self.out, encode(&doc)).map_err(|source| {
            BuildError::Publish(PublishError::Io {
                path: self.out.clone(),
                source,
            })
        })
    }

    fn dump(&self, session: &Session) -> Map<String, Value> {
        let model = session.model();
        let cfg = &model.config;
        let lang = |p: PageId| cfg.sites[model.pages[p].lang].language.key.clone();
        let format_name = |f: FormatId| cfg.output_formats.get(f).name.clone();
        let links = |p: PageId, f: FormatId| {
            model.pages[p]
                .url(f)
                .and_then(|u| u.links.as_ref())
                .map(|l| (l.rel_permalink.escaped(), l.permalink.to_string()))
        };
        // `.TotalPages` (0 for an empty list, then omitted as Go omits it), not the number of
        // pager files (at least 1).
        let pagers: BTreeMap<(PageId, FormatId), usize> = session
            .handles()
            .pagination
            .recorded()
            .into_iter()
            .map(|(k, r)| (k, r.pagination.total_pages()))
            .filter(|&(_, n)| n > 0)
            .collect();

        let mut records = Vec::new();
        for (&(page, format), &wrote) in &self.renders {
            let p = &model.pages[page];
            let (rel, perma) = links(page, format).unwrap_or_default();
            let mut r = Map::new();
            r.insert("lang".into(), lang(page).into());
            r.insert("path".into(), p.path().into());
            r.insert("kind".into(), p.kind.as_str().into());
            r.insert("format".into(), format_name(format).into());
            r.insert(
                "layout".into(),
                p.meta.layout.clone().unwrap_or_default().into(),
            );
            // Only when the front matter `type` replaced the first segment (as Go writes it).
            let lookup = ssg_render::lookup_path(p);
            if lookup != p.key {
                r.insert("lookupPath".into(), lookup.to_path().into());
            }
            let sel = session.selection(page, format);
            let store = session.templates().store();
            for (key, file_key, name) in [
                ("template", "templateFile", sel.map(|s| &s.layout)),
                ("baseof", "baseofFile", sel.and_then(|s| s.base.as_ref())),
            ] {
                let (plain, file) = name.map(|n| names(store, n)).unwrap_or_default();
                if file != plain {
                    r.insert(file_key.into(), file.into());
                }
                r.insert(key.into(), plain.into());
            }
            r.insert(
                "target".into(),
                p.url(format)
                    .map(|u| u.paths.target.as_str().to_owned())
                    .unwrap_or_default()
                    .into(),
            );
            r.insert("relPermalink".into(), rel.into());
            r.insert("permalink".into(), perma.into());
            if !wrote {
                r.insert("written".into(), false.into());
            }
            if let Some(&n) = pagers.get(&(page, format)) {
                r.insert("pagers".into(), n.into());
            }
            records.push(r);
        }
        sort(&mut records, &["lang", "path", "kind", "format"]);

        let (mut aliases, mut pager_aliases) = (Vec::new(), Vec::new());
        for a in &self.aliases {
            let p = &model.pages[a.page];
            let redirect = a.kind == AliasKind::Redirect;
            let alias = match a.kind {
                AliasKind::Pager => a.from.clone(),
                AliasKind::Redirect | AliasKind::FrontMatter => {
                    let s = a.from.strip_suffix("index.html").unwrap_or(&a.from);
                    let s = if s.len() > 1 {
                        s.trim_end_matches('/')
                    } else {
                        s
                    };
                    if redirect {
                        s.trim_start_matches('/').to_owned()
                    } else {
                        s.to_owned()
                    }
                }
            };
            let row = json!({
                "alias": alias,
                "format": format_name(a.format),
                "from": a.from,
                "kind": match a.kind {
                    AliasKind::FrontMatter => "front matter",
                    AliasKind::Redirect => "redirect",
                    AliasKind::Pager => "pager",
                },
                "lang": if redirect { String::new() } else { lang(a.page) },
                "path": if redirect { String::new() } else { p.path() },
                "permalink": links(a.page, a.format).map(|(_, perma)| perma).unwrap_or_default(),
            });
            let Value::Object(row) = row else {
                unreachable!("an object")
            };
            if a.kind == AliasKind::Pager {
                pager_aliases.push(row);
            } else {
                aliases.push(row);
            }
        }
        let alias_keys = ["from", "lang", "path", "format", "alias"];
        sort(&mut aliases, &alias_keys);
        sort(&mut pager_aliases, &alias_keys);

        let mut pages = Vec::new();
        let mut resources = Vec::new();
        for p in model
            .pages
            .iter()
            .filter(|p| p.role == PageRole::Standalone)
        {
            let outputs: Vec<Value> = p
                .urls
                .iter()
                .filter(|u| u.links.is_some())
                .map(|u| format_name(u.format).into())
                .collect();
            let Value::Object(row) = json!({
                "kind": p.kind.as_str(),
                "lang": lang(p.id),
                "outputs": outputs,
                "path": p.path(),
            }) else {
                unreachable!("an object")
            };
            pages.push(row);
            let urls = cfg.sites[p.lang].site_urls();
            for &rid in &p.resources {
                let b = &model.bundle_resources[rid];
                if b.page.is_some() {
                    continue;
                }
                let mut r = Map::new();
                r.insert("lang".into(), lang(p.id).into());
                r.insert("path".into(), p.path().into());
                r.insert("name".into(), b.name_normalized.clone().into());
                r.insert(
                    "relPermalink".into(),
                    b.link()
                        .map(|l| {
                            ssg_base::UrlPath::new(&urls.prepend_base_path(l.as_str())).escaped()
                        })
                        .unwrap_or_default()
                        .into(),
                );
                if let Some(t) = b.target() {
                    r.insert("target".into(), t.as_str().to_owned().into());
                }
                if !p.meta.build.publish_resources {
                    r.insert("publish".into(), false.into());
                }
                resources.push(r);
            }
        }
        sort(&mut pages, &["lang", "path", "kind"]);
        sort(&mut resources, &["lang", "path", "name"]);

        let mut layouts = Vec::new();
        for t in session.templates().store().templates() {
            if matches!(t.origin, Origin::Embedded) {
                continue;
            }
            let category = match t.role {
                TemplateRole::Layout { .. } | TemplateRole::Standalone(_) => "layout",
                TemplateRole::Base { .. } => "baseof",
                TemplateRole::Partial { .. } => "partial",
                TemplateRole::Shortcode { .. } => "shortcode",
                TemplateRole::Hook { .. } => "markup",
            };
            let name = plain_name(&t.name);
            let Value::Object(row) = json!({"category": category, "file": name, "name": name})
            else {
                unreachable!("an object")
            };
            layouts.push(row);
        }
        sort(&mut layouts, &["file", "name", "category"]);

        let mut config = Map::new();
        config.insert(
            "baseURLs".into(),
            cfg.sites
                .iter()
                .map(|s| (s.language.key.clone(), s.base_url.as_str().into()))
                .collect::<Map<_, _>>()
                .into(),
        );
        config.insert(
            "defaultContentLanguage".into(),
            cfg.default_site().language.key.clone().into(),
        );
        config.insert(
            "languageIndex".into(),
            cfg.sites
                .iter()
                .enumerate()
                .map(|(i, s)| (s.language.key.clone(), i.into()))
                .collect::<Map<_, _>>()
                .into(),
        );
        config.insert(
            "defaultOutputFormat".into(),
            cfg.default_output_format.clone().into(),
        );

        let rows =
            |v: Vec<Map<String, Value>>| Value::Array(v.into_iter().map(Value::Object).collect());
        let mut doc = Map::new();
        doc.insert("aliases".into(), rows(aliases));
        doc.insert("config".into(), config.into());
        doc.insert("layouts".into(), rows(layouts));
        doc.insert("pagerAliases".into(), rows(pager_aliases));
        doc.insert("pages".into(), rows(pages));
        doc.insert("records".into(), rows(records));
        doc.insert("resources".into(), rows(resources));
        doc.insert("schema".into(), SCHEMA.into());
        doc
    }
}

/// A template's v0.146 name without the theme or embedded prefix (and without a synthesised
/// `@@` suffix).
fn plain_name(n: &TemplateName) -> String {
    let s = n.as_str();
    let s = s.split_once("@@").map_or(s, |(l, _)| l);
    if let Some(rest) = s.strip_prefix(ssg_layouts::EMBEDDED_PREFIX) {
        return rest.to_owned();
    }
    match s.strip_prefix("_theme") {
        Some(rest) => rest.split_once('/').map_or(s, |(_, r)| r).to_owned(),
        None => s.to_owned(),
    }
}

/// The name of a chosen template and its file (`_embedded/<name>` for an embedded one).
fn names(store: &ssg_layouts::LayoutStore, n: &TemplateName) -> (String, String) {
    let plain = plain_name(n);
    let embedded = store.get(n).map_or_else(
        || n.as_str().starts_with(ssg_layouts::EMBEDDED_PREFIX),
        |t| t.origin == Origin::Embedded,
    );
    let file = if embedded {
        format!("{}{plain}", ssg_layouts::EMBEDDED_PREFIX)
    } else {
        plain.clone()
    };
    (plain, file)
}

/// Sorts rows by the string values of `keys`, as the Go oracle does.
fn sort(rows: &mut [Map<String, Value>], keys: &[&str]) {
    let text = |r: &Map<String, Value>, k: &str| match r.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    };
    rows.sort_by(|a, b| {
        keys.iter()
            .map(|k| text(a, k).cmp(&text(b, k)))
            .find(|o| o.is_ne())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// The oracle's layout: sorted keys, one member per line, one element per line for arrays of
/// objects.
fn encode(doc: &Map<String, Value>) -> String {
    let mut out = String::from("{\n");
    for (i, (k, v)) in doc.iter().enumerate() {
        out.push_str(&Value::from(k.as_str()).to_string());
        out.push_str(": ");
        match v {
            Value::Array(items) if !items.is_empty() && items.iter().all(Value::is_object) => {
                out.push_str("[\n");
                for (j, e) in items.iter().enumerate() {
                    out.push_str(&e.to_string());
                    if j + 1 < items.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push(']');
            }
            v => out.push_str(&v.to_string()),
        }
        if i + 1 < doc.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

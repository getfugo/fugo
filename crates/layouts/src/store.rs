//! The layout store: every template of the project, its themes and the embedded set, classified
//! by its v0.146 name and indexed for the lookups.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use ssg_base::diag::Position;
use ssg_base::paths::ContentKey;
use ssg_base::{FormatId, IdVec, LangIdx, MediaTypeId, PageKind};
use ssg_config::Config;
use ssg_config::output::Escaping;
use ssg_vfs::{Component, Module, Vfs};

use crate::classify::{Classified, Outcome, classify};
use crate::env::LayoutEnv;
use crate::error::{IssueKind, LayoutIssue, TemplateError};
use crate::name::{HookKind, Origin, TemplateName, TemplateRole};
use crate::score::{self, Category, Desc};
use crate::source;

ssg_base::define_id!(
    /// A template of the store.
    pub(crate) Tid(u32)
);

/// A layout file handed to [`LayoutStore::from_sources`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutSource {
    /// The path in the layouts component (`docs/list.html`, `_partials/head.html`).
    pub rel: String,
    pub origin: Origin,
    pub source: String,
}

/// A template of the store.
#[derive(Clone, Debug)]
pub struct TemplateInfo {
    /// The Tera name (with the origin's prefix).
    pub name: TemplateName,
    pub role: TemplateRole,
    /// The directory the template applies to (`docs` for `docs/list.html`; the scope before
    /// `_shortcodes/` or `_markup/`; empty at the root and for partials).
    pub scope: ContentKey,
    pub lang: Option<LangIdx>,
    pub format: Option<FormatId>,
    pub media: Option<MediaTypeId>,
    /// How the template's output is escaped (from its output format).
    pub escaping: Escaping,
    pub origin: Origin,
    pub(crate) desc: Desc,
    pub(crate) key: String,
    pub(crate) path: String,
    pub(crate) ids: usize,
    pub(crate) source: Arc<str>,
    /// The `"baseof.html"` literal of a layout that requests Go's base resolution.
    pub(crate) extends_baseof: Option<Range<usize>>,
    /// The name to render under when the file suffix escapes differently from the format.
    pub(crate) alias: Option<TemplateName>,
}

impl TemplateInfo {
    /// The name to render this template under: its own name, or an alias whose suffix gives
    /// Tera the escaping of the template's output format (§4.5).
    #[must_use]
    pub fn render_name(&self) -> &TemplateName {
        self.alias.as_ref().unwrap_or(&self.name)
    }

    /// Whether the layout requests Go's base resolution (`{% extends "baseof.html" %}`).
    #[must_use]
    pub fn requests_base(&self) -> bool {
        self.extends_baseof.is_some()
    }

    /// The template's source.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The normalised path in the layouts component, with a leading slash.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn category(&self) -> Category {
        match self.role {
            TemplateRole::Layout { .. } | TemplateRole::Standalone(_) => Category::Layout,
            TemplateRole::Base { .. } => Category::Base,
            TemplateRole::Partial { .. } => Category::Partial,
            TemplateRole::Shortcode { .. } => Category::Shortcode,
            TemplateRole::Hook { .. } => Category::Hook,
        }
    }
}

/// Whether Tera escapes a template of this name (`autoescape_on`, §4.5).
pub(crate) fn name_escapes(name: &str) -> bool {
    crate::AUTOESCAPE_SUFFIXES.iter().any(|s| name.ends_with(s))
}

/// `name` with the alias suffix that makes Tera escape it as `escaping` says, if its own suffix
/// does not.
pub(crate) fn escaping_alias(name: &str, escaping: Escaping) -> Option<String> {
    match (escaping, name_escapes(name)) {
        (Escaping::Plain, true) => Some(format!("{name}@@plain")),
        (Escaping::Html, false) => Some(format!("{name}@@escaped.html")),
        _ => None,
    }
}

/// Every template of a project, classified and indexed.
#[derive(Debug)]
pub struct LayoutStore {
    pub(crate) env: LayoutEnv,
    pub(crate) templates: IdVec<Tid, TemplateInfo>,
    pub(crate) by_name: HashMap<TemplateName, Tid>,
    /// Layouts, base templates and hooks by key; each list sorted by path. Only the template
    /// that wins a duplicate (key, descriptor) is indexed.
    pub(crate) tree: BTreeMap<String, Vec<Tid>>,
    pub(crate) partials: BTreeMap<String, Vec<Tid>>,
    /// Shortcodes by key, then name.
    pub(crate) shortcodes: BTreeMap<String, BTreeMap<String, Vec<Tid>>>,
    /// The base templates each layout that requests base resolution may be wrapped in.
    pub(crate) bases: BTreeMap<Tid, Vec<Tid>>,
    /// What the literal `"baseof.html"` resolves to in Tera.
    pub(crate) root_base: Option<Tid>,
}

impl LayoutStore {
    /// Scans the layouts component of `vfs` (the project first, then the themes) and adds the
    /// embedded templates.
    ///
    /// # Errors
    /// A walk or read error, or [`TemplateError::Layouts`] with every legacy name, unknown name
    /// and Go-template file.
    pub fn scan(vfs: &Vfs, cfg: &Config) -> Result<Self, TemplateError> {
        let env = LayoutEnv::from_config(cfg);
        let mut sources = Vec::new();
        for f in vfs.walk(Component::Layouts)? {
            let module = vfs
                .mounts()
                .get(usize::from(f.mount_idx))
                .map_or(Module::Project, |m| m.module);
            let origin = match module {
                Module::Project => Origin::User(f.abs.clone()),
                Module::Theme(n) => {
                    Origin::Theme(u8::try_from(n + 1).unwrap_or(u8::MAX), f.abs.clone())
                }
            };
            let bytes = fs::read(&f.abs).map_err(|source| TemplateError::Io {
                path: f.abs.clone(),
                source,
            })?;
            let source = String::from_utf8(bytes).map_err(|e| TemplateError::Io {
                path: f.abs.clone(),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
            })?;
            sources.push(LayoutSource {
                rel: f.rel,
                origin,
                source,
            });
        }
        sources.extend(crate::embedded::sources());
        Self::from_sources(env, sources)
    }

    /// Builds the store from layout files (what [`LayoutStore::scan`] reads, or any other set).
    ///
    /// # Errors
    /// [`TemplateError::Layouts`] with every legacy name, unknown name, duplicate Tera name and
    /// Go-template file, sorted by position.
    pub fn from_sources(
        env: LayoutEnv,
        sources: impl IntoIterator<Item = LayoutSource>,
    ) -> Result<Self, TemplateError> {
        let mut issues = Vec::new();
        let mut templates: IdVec<Tid, TemplateInfo> = IdVec::new();
        let mut by_name = HashMap::new();
        for s in sources {
            let position = |line: usize| Position {
                file: Arc::from(match s.origin.file() {
                    Some(p) => p.to_path_buf(),
                    None => Path::new(&format!("{}{}", s.origin.prefix(), s.rel)).to_path_buf(),
                }),
                line: u32::try_from(line).unwrap_or(u32::MAX),
                col: 0,
            };
            let c = match classify(&env, &s.rel) {
                Outcome::Template(c) => *c,
                Outcome::Skip => continue,
                Outcome::Issue(kind) => {
                    issues.push(LayoutIssue {
                        position: position(0),
                        kind,
                    });
                    continue;
                }
            };
            if let Some((line, marker)) = source::go_marker(&s.source) {
                issues.push(LayoutIssue {
                    position: position(line),
                    kind: IssueKind::GoTemplate { marker },
                });
                continue;
            }
            let name = TemplateName::new(format!("{}{}", s.origin.prefix(), &c.path[1..]));
            if let Some(&other) = by_name.get(&name) {
                let other: &TemplateInfo = &templates[other];
                issues.push(LayoutIssue {
                    position: position(0),
                    kind: IssueKind::UnknownName {
                        reason: format!(
                            "same template name `{name}` as {}",
                            other.origin.file().map_or_else(
                                || other.name.to_string(),
                                |p| p.display().to_string()
                            )
                        ),
                    },
                });
                continue;
            }
            let t = info(name.clone(), c, s.origin, s.source);
            by_name.insert(name, templates.push(t));
        }
        if !issues.is_empty() {
            issues.sort();
            return Err(TemplateError::Layouts(issues));
        }
        Ok(Self::index(env, templates, by_name))
    }

    fn index(
        env: LayoutEnv,
        templates: IdVec<Tid, TemplateInfo>,
        by_name: HashMap<TemplateName, Tid>,
    ) -> Self {
        // The winner of each (category, key, shortcode name, descriptor): the earliest origin,
        // then the fewest identifiers, then the smallest path.
        let mut winners: BTreeMap<(u8, &str, &str, &Desc), Tid> = BTreeMap::new();
        for (id, t) in templates.iter_enumerated() {
            let sc_name = match &t.role {
                TemplateRole::Shortcode { name } => name.as_str(),
                _ => "",
            };
            let slot = winners
                .entry((category_rank(t.category()), &t.key, sc_name, &t.desc))
                .or_insert(id);
            let cur = &templates[*slot];
            if (t.origin.rank(), t.ids, &t.path) < (cur.origin.rank(), cur.ids, &cur.path) {
                *slot = id;
            }
        }
        let mut tree: BTreeMap<String, Vec<Tid>> = BTreeMap::new();
        let mut partials: BTreeMap<String, Vec<Tid>> = BTreeMap::new();
        let mut shortcodes: BTreeMap<String, BTreeMap<String, Vec<Tid>>> = BTreeMap::new();
        for &id in winners.values() {
            let t = &templates[id];
            let list = match &t.role {
                TemplateRole::Partial { .. } => partials.entry(t.key.clone()).or_default(),
                TemplateRole::Shortcode { name } => shortcodes
                    .entry(t.key.clone())
                    .or_default()
                    .entry(name.clone())
                    .or_default(),
                _ => tree.entry(t.key.clone()).or_default(),
            };
            list.push(id);
        }
        // The candidates of one key are offered in descriptor order; the chooser's tie-breaks
        // depend on the order when a later candidate has equal `w1` but other `w2`/`w3`.
        let in_order =
            |v: &mut Vec<Tid>| v.sort_by_cached_key(|&id| order_key(&env, &templates[id]));
        tree.values_mut().for_each(in_order);
        partials.values_mut().for_each(in_order);
        shortcodes
            .values_mut()
            .flat_map(BTreeMap::values_mut)
            .for_each(in_order);

        let active_bases: Vec<Tid> = tree
            .values()
            .flatten()
            .copied()
            .filter(|&id| templates[id].category() == Category::Base)
            .collect();
        let defaults = env.defaults();
        let mut bases = BTreeMap::new();
        for &id in tree.values().flatten() {
            let l = &templates[id];
            if l.category() != Category::Layout || l.extends_baseof.is_none() {
                continue;
            }
            let mut cands: Vec<Tid> = active_bases
                .iter()
                .copied()
                .filter(|&b| {
                    let b = &templates[b];
                    score::kind_fits_layout(b.desc.kind, l.desc.layout.as_deref())
                        && score::compare(defaults, Category::Base, &l.desc, &b.desc).matches()
                })
                .collect();
            // Root first, then in descriptor order (the first of equal candidates wins).
            cands.sort_by_cached_key(|&b| {
                let t = &templates[b];
                (depth(&t.key), t.key.clone(), order_key(&env, t))
            });
            if !cands.is_empty() {
                bases.insert(id, cands);
            }
        }
        let root_base = std::iter::once(String::new())
            .chain(templates.iter().filter_map(|t| match t.origin {
                Origin::Theme(..) | Origin::Embedded => Some(t.origin.prefix()),
                Origin::User(_) => None,
            }))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(|p| TemplateName::new(format!("{p}{}", source::BASEOF)))
            .filter_map(|n| by_name.get(&n).copied())
            .min_by_key(|&id| templates[id].origin.rank());
        Self {
            env,
            templates,
            by_name,
            tree,
            partials,
            shortcodes,
            bases,
            root_base,
        }
    }

    /// The environment the store was built with.
    #[must_use]
    pub fn env(&self) -> &LayoutEnv {
        &self.env
    }

    /// Every template (including those hidden by a template of the same key and descriptor),
    /// in scan order.
    pub fn templates(&self) -> impl ExactSizeIterator<Item = &TemplateInfo> {
        self.templates.iter()
    }

    /// The template named `name` (a variant or alias name gives its layout).
    #[must_use]
    pub fn get(&self, name: &TemplateName) -> Option<&TemplateInfo> {
        self.template(name.as_str())
    }

    /// The template with Tera name `name` (a variant or alias name gives its layout).
    #[must_use]
    pub fn template(&self, name: &str) -> Option<&TemplateInfo> {
        self.id_of(name).map(|id| &self.templates[id])
    }

    pub(crate) fn id_of(&self, name: &str) -> Option<Tid> {
        let name = name.split_once("@@").map_or(name, |(layout, _)| layout);
        self.by_name.get(&TemplateName::new(name)).copied()
    }

    /// The base template the literal `"baseof.html"` resolves to.
    #[must_use]
    pub fn root_base(&self) -> Option<&TemplateInfo> {
        self.root_base.map(|id| &self.templates[id])
    }

    /// The base templates layout `name` may be wrapped in (empty unless it requests base
    /// resolution), nearest key first.
    pub fn base_candidates(&self, name: &TemplateName) -> impl Iterator<Item = &TemplateInfo> {
        self.id_of(name.as_str())
            .and_then(|id| self.bases.get(&id))
            .into_iter()
            .flatten()
            .map(|&b| &self.templates[b])
    }
}

fn category_rank(c: Category) -> u8 {
    match c {
        Category::Layout => 0,
        Category::Base => 1,
        Category::Hook => 2,
        Category::Shortcode => 3,
        Category::Partial => 4,
    }
}

/// The order candidates of one key are offered in: category, then the descriptor's names (kind,
/// layout, output format, media type), language, variants, plain text.
type OrderKey = (
    u8,
    String,
    String,
    String,
    String,
    Option<LangIdx>,
    String,
    String,
    bool,
);

fn order_key(env: &LayoutEnv, t: &TemplateInfo) -> OrderKey {
    let d = &t.desc;
    (
        category_rank(t.category()),
        d.kind.map(PageKind::as_str).unwrap_or_default().to_owned(),
        d.layout.as_deref().unwrap_or_default().to_owned(),
        d.format
            .map(|f| env.formats.get(f).name.clone())
            .unwrap_or_default(),
        d.media
            .map(|m| env.media.get(m).type_string())
            .unwrap_or_default(),
        d.lang,
        d.variant1
            .map(HookKind::as_str)
            .unwrap_or_default()
            .to_owned(),
        d.variant2.as_deref().unwrap_or_default().to_owned(),
        d.flags.plain,
    )
}

pub(crate) fn depth(key: &str) -> usize {
    if key.is_empty() {
        0
    } else {
        key.split('/').count()
    }
}

fn info(name: TemplateName, c: Classified, origin: Origin, source: String) -> TemplateInfo {
    let extends_baseof = match c.role {
        TemplateRole::Layout { .. } | TemplateRole::Standalone(_) => {
            source::baseof_extends(&source)
        }
        _ => None,
    };
    let alias = escaping_alias(name.as_str(), c.escaping).map(TemplateName::new);
    let scope = match c.role {
        TemplateRole::Partial { .. } => ContentKey::home(),
        _ => ContentKey::from_source(&c.key),
    };
    TemplateInfo {
        name,
        role: c.role,
        scope,
        lang: c.desc.lang,
        format: c.desc.format,
        media: c.desc.media,
        escaping: c.escaping,
        origin,
        desc: c.desc,
        key: c.key,
        path: c.path,
        ids: c.ids,
        source: Arc::from(source),
        extends_baseof,
        alias,
    }
}

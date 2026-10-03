//! The lookups: a page's layout and base template, shortcodes, render hooks and partials.

use std::sync::Arc;

use ssg_base::paths::ContentKey;
use ssg_base::{FormatId, LangIdx, PageKind};
use ssg_config::output::Escaping;
use ssg_vfs::{Component, Parsed};

use crate::classify::main_kind;
use crate::env::{EmbeddedHooks, HookUse};
use crate::name::{HookKind, TemplateName};
use crate::score::{self, Best, Candidate, Category, Desc, Flags, LAYOUT_LIST, LAYOUT_SINGLE};
use crate::store::{LayoutStore, TemplateInfo, Tid, depth, escaping_alias};

/// What a page is rendered with: its layout job's lookup (§4.3).
#[derive(Clone, Copy, Debug)]
pub struct LayoutQuery<'a> {
    /// The page's path with the first segment replaced by its `type` when that differs from the
    /// section; templates in the directories from the root down to it are candidates.
    pub path: &'a ContentKey,
    /// The page kind (standalone kinds are looked up by their output format alone).
    pub kind: Option<PageKind>,
    /// The front-matter `layout`.
    pub layout: Option<&'a str>,
    /// Only a template whose layout identifier is exactly [`LayoutQuery::layout`] matches.
    pub exact_layout: bool,
    pub lang: Option<LangIdx>,
    pub format: FormatId,
}

/// The templates chosen for a (page, format).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Selection {
    /// The layout template.
    pub layout: TemplateName,
    /// The base template Go's base resolution chose, when the layout requests it.
    pub base: Option<TemplateName>,
    /// The name to render: the layout's, or the synthesised `<layout>@@<base>` variant when the
    /// base is not what `"baseof.html"` resolves to.
    pub render_as: TemplateName,
}

/// A render hook lookup.
#[derive(Clone, Copy, Debug)]
pub struct HookQuery<'a> {
    pub hook: HookKind,
    /// The variant (a code block's language).
    pub variant: Option<&'a str>,
    /// The page's path (as for [`LayoutQuery::path`]).
    pub path: &'a ContentKey,
    pub kind: Option<PageKind>,
    pub lang: Option<LangIdx>,
    pub format: FormatId,
    /// When the embedded link and image hooks may be used.
    pub embedded: EmbeddedHooks,
}

/// A shortcode lookup.
#[derive(Clone, Copy, Debug)]
pub struct ShortcodeQuery<'a> {
    pub name: &'a str,
    /// The page's path (as for [`LayoutQuery::path`]).
    pub path: &'a ContentKey,
    pub kind: Option<PageKind>,
    pub lang: Option<LangIdx>,
    pub format: FormatId,
    /// Called with `{{% %}}`: plain-text shortcode templates may serve an HTML page.
    pub markdown: bool,
}

/// Why a shortcode lookup found nothing.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ShortcodeMiss {
    #[error("no template for shortcode {0:?}")]
    NotFound(String),
    /// Templates exist, but none for this output format (a plain-text template in an HTML page
    /// needs the `{{% %}}` delimiters).
    #[error(
        "no template of shortcode {name:?} fits this output format (found {candidates:?}); plain-text shortcodes in HTML pages need the {{{{% %}}}} delimiters"
    )]
    Incompatible {
        name: String,
        candidates: Vec<TemplateName>,
    },
}

impl LayoutStore {
    fn query_desc(&self, kind: Option<PageKind>, lang: Option<LangIdx>, format: FormatId) -> Desc {
        let kind = main_kind(kind);
        let f = self.env.formats.get(format);
        Desc {
            kind,
            layout: kind.map(|k| {
                Arc::from(if k == PageKind::Page {
                    LAYOUT_SINGLE
                } else {
                    LAYOUT_LIST
                })
            }),
            lang,
            format: Some(format),
            media: Some(f.media_type),
            flags: Flags {
                plain: f.escaping == Escaping::Plain,
                ..Flags::default()
            },
            ..Desc::default()
        }
    }

    /// The keys from the root down to `path` that hold templates, with their distance to it.
    fn walk<'s>(&'s self, path: &ContentKey) -> impl Iterator<Item = (usize, &'s Vec<Tid>)> + 's {
        let key = path.as_str().to_owned();
        let d = depth(&key);
        ancestors(&key)
            .into_iter()
            .filter_map(move |k| self.tree.get(&k).map(|v| (d - depth(&k), v)))
    }

    fn candidate(&self, id: Tid) -> Candidate<'_, Tid> {
        let t = &self.templates[id];
        Candidate {
            item: id,
            path: &t.path,
            embedded: t.origin.is_embedded(),
            hook: t.category() == Category::Hook,
        }
    }

    /// The layout (and base template) of a page in an output format: Go's scorer over the
    /// templates from the root down to the page's path. `None` when no template matches, or
    /// when the layout requests base resolution and no base template fits.
    #[must_use]
    pub fn select(&self, q: &LayoutQuery<'_>) -> Option<Selection> {
        let mut desc = self.query_desc(q.kind, q.lang, q.format);
        desc.user_layout = q.layout.map(|l| Arc::from(l.to_lowercase()));
        desc.flags.exact_layout = q.exact_layout;
        let defaults = self.env.defaults();

        let mut best = Best::new();
        for (distance, ids) in self.walk(q.path) {
            for &id in ids {
                let t = &self.templates[id];
                if t.category() != Category::Layout {
                    continue;
                }
                let mut w = score::compare(defaults, Category::Layout, &desc, &t.desc);
                w.distance = distance;
                best.offer(self.candidate(id), w);
            }
        }
        let (id, _) = best.winner()?;
        let layout = &self.templates[id];
        let Some(cands) = self.bases.get(&id) else {
            return Some(Selection {
                layout: layout.name.clone(),
                base: None,
                render_as: layout.render_name().clone(),
            });
        };

        let key = q.path.as_str();
        let d = depth(key);
        let mut best = Best::new();
        for &b in cands {
            let base = &self.templates[b];
            if !in_path(key, &base.key) {
                continue;
            }
            let mut w = score::compare(defaults, Category::Base, &desc, &base.desc);
            w.distance = d - depth(&base.key);
            // Every variant carries the layout's path.
            best.offer(
                Candidate {
                    item: b,
                    path: &layout.path,
                    embedded: false,
                    hook: false,
                },
                w,
            );
        }
        let (b, _) = best.winner()?;
        Some(self.selection(id, b))
    }

    fn selection(&self, layout: Tid, base: Tid) -> Selection {
        let l = &self.templates[layout];
        let b = &self.templates[base];
        let render_as = if Some(base) == self.root_base {
            l.render_name().clone()
        } else {
            variant_name(l, b)
        };
        Selection {
            layout: l.name.clone(),
            base: Some(b.name.clone()),
            render_as,
        }
    }

    /// The render hook for a Markdown element: `_markup/render-<kind>[-<variant>]` templates from
    /// the root down to the page's path; a hook for another format of the same media type is a
    /// fallback. Link and image hooks follow [`EmbeddedHooks`].
    #[must_use]
    pub fn hook(&self, q: &HookQuery<'_>) -> Option<TemplateName> {
        let mut desc = self.query_desc(q.kind, q.lang, q.format);
        desc.variant1 = Some(q.hook);
        desc.variant2 = q.variant.map(|v| Arc::from(v.to_lowercase()));
        let policy = q.embedded.get(q.hook);
        let defaults = self.env.defaults();
        let mut best = Best::new();
        for (distance, ids) in self.walk(q.path) {
            for &id in ids {
                let t = &self.templates[id];
                if t.category() != Category::Hook
                    || t.desc.variant1 != desc.variant1
                    || (desc.variant2.is_some()
                        && t.desc.variant2.is_some()
                        && t.desc.variant2 != desc.variant2)
                {
                    continue;
                }
                let embedded = t.origin.is_embedded();
                let allowed = match policy {
                    HookUse::Always => embedded,
                    HookUse::Fallback => true,
                    HookUse::Never => !embedded,
                };
                if !allowed {
                    continue;
                }
                let mut w = score::compare(defaults, Category::Hook, &desc, &t.desc);
                w.distance = distance;
                best.offer(self.candidate(id), w);
            }
        }
        best.winner()
            .map(|(id, _)| self.templates[id].render_name().clone())
    }

    /// The template of a shortcode for a page and output format.
    ///
    /// # Errors
    /// [`ShortcodeMiss`] when the shortcode has no template, or none that fits the format.
    pub fn shortcode(&self, q: &ShortcodeQuery<'_>) -> Result<TemplateName, ShortcodeMiss> {
        let name = q.name.to_lowercase();
        let mut desc = self.query_desc(q.kind, q.lang, q.format);
        desc.flags.allow_plain = q.markdown;
        let defaults = self.env.defaults();
        let key = q.path.as_str();
        let d = depth(key);
        let mut best = Best::new();
        let mut candidates = Vec::new();
        for k in ancestors(key) {
            let Some(ids) = self.shortcodes.get(&k).and_then(|m| m.get(&name)) else {
                continue;
            };
            for &id in ids {
                let t = &self.templates[id];
                candidates.push(t.render_name().clone());
                let mut w = score::compare(defaults, Category::Shortcode, &desc, &t.desc);
                w.distance = d - depth(&k);
                best.offer(self.candidate(id), w);
            }
        }
        match best.winner() {
            Some((id, _)) => Ok(self.templates[id].render_name().clone()),
            None if candidates.is_empty() => Err(ShortcodeMiss::NotFound(name)),
            None => Err(ShortcodeMiss::Incompatible { name, candidates }),
        }
    }

    /// Whether any template of shortcode `name` exists (in any scope).
    #[must_use]
    pub fn has_shortcode(&self, name: &str) -> bool {
        let name = name.to_lowercase();
        self.shortcodes.values().any(|m| m.contains_key(&name))
    }

    /// The template of partial `name` (`footer`, `helpers/picture.html`, `head.th.html`): the
    /// suffix, language and output format come from the name; without a suffix the HTML format
    /// is meant.
    #[must_use]
    pub fn partial(&self, name: &str) -> Option<TemplateName> {
        let rel = name.trim_start_matches('/');
        let Parsed::File(info) = self.env.parser.parse(Component::Layouts, rel) else {
            return None;
        };
        let (mut media, escaping) = self.env.resolve(info.format, &info.ext);
        let mut format = info.format;
        let mut plain = escaping == Some(Escaping::Plain);
        if format.is_none() && media.is_none() {
            let html = self.env.formats.by_name("html")?;
            let f = self.env.formats.get(html);
            format = Some(html);
            media = Some(f.media_type);
            plain = f.escaping == Escaping::Plain;
        }
        let desc = Desc {
            lang: info.lang,
            format,
            media,
            flags: Flags {
                plain,
                ..Flags::default()
            },
            ..Desc::default()
        };
        let dir = match info.dir() {
            "/" => String::new(),
            d => format!("{}/", &d[1..]),
        };
        let key = format!("_partials/{dir}{}", info.name);
        let defaults = self.env.defaults();
        let mut best = Best::new();
        for &id in self.partials.get(&key)? {
            let w = score::compare(defaults, Category::Partial, &desc, &self.templates[id].desc);
            best.offer(self.candidate(id), w);
        }
        // The name chooses the partial; the format only ranks its files: a partial without a
        // file for the named format still resolves (`partial "footer.json"` gives
        // `footer.html` when that is the only file).
        best.any()
            .map(|id| self.templates[id].render_name().clone())
    }
}

/// `key` and its ancestors, root first (`""`, `a`, `a/b`).
fn ancestors(key: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    if !key.is_empty() {
        out.extend(
            key.match_indices('/')
                .map(|(i, _)| key[..i].to_owned())
                .chain(std::iter::once(key.to_owned())),
        );
    }
    out
}

/// Whether `ancestor` is `key` or one of its ancestors.
fn in_path(key: &str, ancestor: &str) -> bool {
    ancestor.is_empty()
        || key == ancestor
        || key
            .strip_prefix(ancestor)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// The name of layout `l` rendered in base `b`.
pub(crate) fn variant_name(l: &TemplateInfo, b: &TemplateInfo) -> TemplateName {
    let name = format!("{}@@{}", l.name, b.name);
    match escaping_alias(&name, l.escaping) {
        Some(alias) => TemplateName::new(alias),
        None => TemplateName::new(name),
    }
}

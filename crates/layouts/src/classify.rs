//! A layout file's v0.146 name → its role, lookup key and descriptor; legacy names are refused
//! with the name to use instead.

use std::sync::Arc;

use ssg_base::PageKind;
use ssg_config::output::Escaping;
use ssg_vfs::{Component, LayoutRole, Parsed, PathInfo};

use crate::env::LayoutEnv;
use crate::error::IssueKind;
use crate::name::{HookKind, StandaloneKind, TemplateRole};
use crate::score::{Desc, Flags};

/// A classified layout file.
#[derive(Clone, Debug)]
pub(crate) struct Classified {
    pub role: TemplateRole,
    /// The lookup key: the directory (`""`, `blog`, `blog/sub`) for layouts, base templates,
    /// hooks (without `/_markup`) and shortcodes (without `/_shortcodes/...`); the path without
    /// identifiers (`_partials/helpers/picture`) for partials.
    pub key: String,
    pub desc: Desc,
    /// The normalised path with a leading slash (`/docs/list.html`): the tie-break of equal
    /// candidates and the base of the Tera name.
    pub path: String,
    /// The number of identifiers in the file name (fewer wins among duplicates).
    pub ids: usize,
    pub escaping: Escaping,
}

/// What a layout file is.
#[derive(Clone, Debug)]
pub(crate) enum Outcome {
    Template(Box<Classified>),
    /// Not a template: dot files, editor backups, disabled languages.
    Skip,
    Issue(IssueKind),
}

/// Classifies `rel`, a path in the layouts component.
pub(crate) fn classify(env: &LayoutEnv, rel: &str) -> Outcome {
    let file_name = rel.rsplit('/').next().unwrap_or_default();
    if file_name.starts_with('.') || file_name.ends_with('~') || file_name.is_empty() {
        return Outcome::Skip;
    }
    let info = match env.parser.parse(Component::Layouts, rel) {
        Parsed::File(info) => info,
        Parsed::DisabledLanguage => return Outcome::Skip,
    };
    if let Some(rename) = legacy_rename(&info) {
        return Outcome::Issue(IssueKind::LegacyName { rename });
    }
    match derive(env, &info) {
        Ok(c) => Outcome::Template(Box::new(c)),
        Err(reason) => Outcome::Issue(IssueKind::UnknownName { reason }),
    }
}

/// The v0.146 name of a legacy layout path, if `info` is one.
fn legacy_rename(info: &PathInfo) -> Option<String> {
    let path = &info.path[1..];
    let (first, rest) = path.split_once('/').unwrap_or((path, ""));
    let file = path.rsplit('/').next().unwrap_or_default();
    let dir = &path[..path.len() - file.len()];
    match first {
        "_default" if !rest.is_empty() => return Some(rest.to_owned()),
        "partials" if !rest.is_empty() => return Some(format!("_partials/{rest}")),
        "shortcodes" if !rest.is_empty() => return Some(format!("_shortcodes/{rest}")),
        _ => {}
    }
    // `<id>-baseof.<ext>` → `baseof.<id>.<ext>`.
    if let Some((id, tail)) = file.split_once("-baseof")
        && !id.is_empty()
        && (tail.is_empty() || tail.starts_with('.'))
    {
        return Some(format!("{dir}baseof.{id}{tail}"));
    }
    // The pre-v0.146 taxonomy and term templates.
    let parts = info.layout.as_ref()?;
    let (stem, suffix) = file
        .split_once('.')
        .map_or((file, String::new()), |(a, b)| (a, format!(".{b}")));
    if (dir == "taxonomy/" && stem == "list") || (dir == "term/" && stem == "term") {
        return Some(format!("term{suffix}"));
    }
    if matches!(parts.role, LayoutRole::Template | LayoutRole::Baseof)
        && parts.layout.as_deref() == Some("index")
    {
        let renamed = file.replacen("index", "home", 1);
        return Some(format!("{dir}{renamed}"));
    }
    None
}

fn derive(env: &LayoutEnv, info: &PathInfo) -> Result<Classified, String> {
    let parts = info
        .layout
        .as_ref()
        .ok_or_else(|| "not a layouts path".to_owned())?;
    let path = info.path.clone();
    let dir = match info.dir() {
        "/" => "",
        d => &d[1..],
    };
    let first = path[1..].split('/').next().unwrap_or_default();
    if matches!(first, "_internal" | "_server") && path[1..].contains('/') {
        return Err(format!("`{first}/` is reserved for internal templates"));
    }

    let (media, escaping) = env.resolve(info.format, &info.ext);
    let mut desc = Desc {
        kind: parts.kind,
        layout: parts.layout.as_deref().map(Arc::from),
        lang: info.lang,
        format: info.format,
        media,
        flags: Flags {
            plain: escaping == Some(Escaping::Plain),
            ..Flags::default()
        },
        ..Desc::default()
    };
    // A layout identifier that repeats the format or the kind adds nothing.
    if let Some(l) = desc.layout.as_deref()
        && (desc.format.is_some_and(|f| env.formats.get(f).name == l)
            || desc.kind.is_some_and(|k| k.as_str() == l))
    {
        desc.layout = None;
    }
    let ids = usize::from(info.lang.is_some())
        + usize::from(info.format.is_some())
        + usize::from(parts.kind.is_some())
        + usize::from(parts.layout.is_some())
        + usize::from(!info.ext.is_empty());
    let dir_name = |name: &str| {
        if dir.is_empty() {
            name.to_owned()
        } else {
            format!("{dir}/{name}")
        }
    };

    let (role, key) = match parts.role {
        LayoutRole::Partial => {
            desc.layout = None;
            if desc.format.is_none() && desc.media.is_none() {
                let html = env
                    .formats
                    .by_name("html")
                    .ok_or_else(|| "no html output format".to_owned())?;
                desc.format = Some(html);
                desc.media = Some(env.formats.get(html).media_type);
            }
            let key = dir_name(&info.name);
            let name = key.strip_prefix("_partials/").unwrap_or(&key).to_owned();
            (TemplateRole::Partial { name }, key)
        }
        LayoutRole::Shortcode => {
            let full = dir_name(&info.name);
            let (scope, name) = match full.split_once("_shortcodes/") {
                Some((scope, name)) => (scope.trim_end_matches('/').to_owned(), name.to_owned()),
                None => return Err("not under `_shortcodes/`".to_owned()),
            };
            (TemplateRole::Shortcode { name }, scope)
        }
        LayoutRole::Markup => {
            let key = dir
                .strip_suffix("_markup")
                .map_or(dir, |d| d.trim_end_matches('/'))
                .to_owned();
            let hook = desc
                .layout
                .take()
                .and_then(|l| l.strip_prefix("render-").map(str::to_owned))
                .ok_or_else(|| {
                    "a render hook's name is render-<kind>[-<variant>] (`_markup/render-link.html`)"
                        .to_owned()
                })?;
            let (kind, variant) = match hook.split_once('-') {
                Some((k, v)) if !k.is_empty() => (k, Some(v)),
                _ => (hook.as_str(), None),
            };
            let kind = HookKind::parse(kind).ok_or_else(|| {
                format!(
                    "unknown render hook kind `{kind}` (link, image, heading, codeblock, \
                     blockquote, table, passthrough)"
                )
            })?;
            desc.variant1 = Some(kind);
            desc.variant2 = variant.map(Arc::from);
            (
                TemplateRole::Hook {
                    kind,
                    variant: variant.map(str::to_owned),
                },
                key,
            )
        }
        LayoutRole::Baseof => (
            TemplateRole::Base {
                kind: desc.kind,
                layout: desc.layout.as_deref().map(str::to_owned),
            },
            dir.to_owned(),
        ),
        LayoutRole::Template => {
            let standalone = StandaloneKind::ALL.into_iter().find(|s| {
                dir.is_empty()
                    && desc.kind.is_none()
                    && desc.layout.is_none()
                    && info.name == s.format_name()
                    && desc
                        .format
                        .is_some_and(|f| env.formats.get(f).name == s.format_name())
            });
            let role = match standalone {
                Some(s) => TemplateRole::Standalone(s),
                None => TemplateRole::Layout {
                    kind: desc.kind,
                    layout: desc.layout.as_deref().map(str::to_owned),
                },
            };
            (role, dir.to_owned())
        }
    };
    if desc.media.is_none() {
        return Err(format!(
            "no output format or media type for the file suffix `{}`",
            info.ext
        ));
    }
    let escaping = if desc.flags.plain {
        Escaping::Plain
    } else {
        Escaping::Html
    };
    Ok(Classified {
        role,
        key,
        desc,
        path,
        ids,
        escaping,
    })
}

/// The main kinds a query's kind is compared as (standalone kinds have no kind in lookups).
pub(crate) fn main_kind(kind: Option<PageKind>) -> Option<PageKind> {
    kind.filter(|k| k.is_content())
}

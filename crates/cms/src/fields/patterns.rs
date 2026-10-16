//! Patterns of keys in `[cms.fields]` (`"rating.*"`, `"nutrition.**.total"`): the settings of
//! one key are those of the patterns that match it, the longer pattern's over the shorter's, with
//! the key's own over them, setting by setting. A pattern names no key of its own: it reaches the
//! keys of the content and those the settings name.

use std::collections::BTreeMap;

use ssg_base::glob::{self, Case, Glob, GlobError, GlobOpts, Separator};

use super::Field;

/// Whether a key of `[cms.fields]` is a pattern of keys: `*` (any part of a key), `**` (any
/// part of the path, across keys), `?`, `[…]`, `{…,…}` or `\`.
pub(crate) fn is_pattern(key: &str) -> bool {
    key.contains(['*', '?', '[', '{', '\\'])
}

/// A pattern of keys, matched ignoring case; `.` separates the keys of a path.
pub(crate) fn compile(pattern: &str) -> Result<Glob, GlobError> {
    glob::compile(
        pattern,
        GlobOpts {
            case: Case::Fold,
            separator: Separator::Dot,
        },
    )
}

/// The settings of `[cms.fields]`, of keys and of patterns.
pub(crate) struct Settings<'a> {
    /// By key, lower-cased.
    keys: BTreeMap<String, &'a Field>,
    /// The shortest pattern first: a longer one's settings go over its.
    patterns: Vec<(Glob, &'a Field)>,
}

impl<'a> Settings<'a> {
    pub(crate) fn new(fields: &'a BTreeMap<String, Field>) -> Self {
        let mut keys = BTreeMap::new();
        let mut patterns = Vec::new();
        for (key, field) in fields {
            if !is_pattern(key) {
                keys.insert(key.to_lowercase(), field);
            } else if let Ok(g) = compile(key) {
                // (the configuration's check rejects a pattern that does not compile)
                patterns.push((key.chars().count(), g, field));
            }
        }
        patterns.sort_by_key(|p| p.0);
        Settings {
            keys,
            patterns: patterns.into_iter().map(|(_, g, f)| (g, f)).collect(),
        }
    }

    /// The keys the settings name, lower-cased (not their patterns).
    pub(crate) fn keys(&self) -> impl Iterator<Item = &str> {
        self.keys.keys().map(String::as_str)
    }

    /// The settings of `key` (lower-cased): `None` when neither the key nor a pattern has any.
    pub(crate) fn of(&self, key: &str) -> Option<Field> {
        let matching = self.patterns.iter().filter(|(g, _)| g.is_match(key));
        let own = self.keys.get(key).copied();
        matching
            .map(|(_, f)| *f)
            .chain(own)
            .fold(None, |base, top| {
                Some(match base {
                    Some(base) => over(base, top),
                    None => top.clone(),
                })
            })
    }
}

/// The settings `top` over `base`, setting by setting: a setting `top` leaves out stays as `base`
/// gives it (a yes/no setting is on when either turns it on).
fn over(base: Field, top: &Field) -> Field {
    Field {
        label: top.label.clone().or(base.label),
        widget: top.widget.or(base.widget),
        options: if top.options.is_empty() {
            base.options
        } else {
            top.options.clone()
        },
        multiple: top.multiple || base.multiple,
        help: top.help.clone().or(base.help),
        default: top.default.clone().or(base.default),
        min: top.min.clone().or(base.min),
        max: top.max.clone().or(base.max),
        step: top.step.clone().or(base.step),
        required: top.required || base.required,
        shared: top.shared.or(base.shared),
        collapsed: top.collapsed || base.collapsed,
        summary: top.summary.clone().or(base.summary),
    }
}

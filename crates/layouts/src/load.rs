//! Loading the store into one Tera instance (§4.1 step 2).

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, PoisonError};

use ssg_base::paths::ContentKey;
use ssg_base::{FormatId, LangIdx, PageKind};

use crate::env::EmbeddedHooks;
use crate::error::TemplateError;
use crate::lookup::{HookQuery, LayoutQuery, Selection, ShortcodeMiss, ShortcodeQuery};
use crate::name::{HookKind, Origin, TemplateName};
use crate::source;
use crate::store::LayoutStore;

/// The selections a build renders with; [`load`] registers the base variants they need.
#[derive(Clone, Debug, Default)]
pub struct Selections(BTreeSet<Selection>);

impl Selections {
    /// No selections.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a selection.
    pub fn insert(&mut self, s: Selection) {
        self.0.insert(s);
    }

    /// Every selection, sorted.
    pub fn iter(&self) -> impl Iterator<Item = &Selection> {
        self.0.iter()
    }
}

impl FromIterator<Selection> for Selections {
    fn from_iter<I: IntoIterator<Item = Selection>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// The loaded templates: one Tera instance plus the store the lookups run on.
pub struct Templates {
    tera: tera::Tera,
    store: Arc<LayoutStore>,
    /// `uses_variable` answers, memoised per (template, variable).
    uses: Mutex<HashMap<(TemplateName, String), bool>>,
}

impl std::fmt::Debug for Templates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Templates")
            .field("templates", &self.store.templates.len())
            .finish_non_exhaustive()
    }
}

/// Loads every template of `store` into a new Tera instance: the fallback prefixes
/// (`_theme1/` … `_embedded/`) first, then `register` (functions, filters and tests: Tera checks
/// every name when a template is added), then ONE `add_raw_templates` call with the templates,
/// their escaping aliases and the base variants `sel` needs. Autoescaping is on for `.html`,
/// `.htm`, `.xml` and `.svg` names.
///
/// # Errors
/// [`TemplateError::Tera`]: syntax errors, unknown names, inheritance errors, include cycles.
pub fn load(
    store: Arc<LayoutStore>,
    sel: &Selections,
    register: &dyn Fn(&mut tera::Tera),
) -> Result<Templates, TemplateError> {
    let mut tera = tera::Tera::default();
    let themes: BTreeSet<u8> = store
        .templates
        .iter()
        .filter_map(|t| match t.origin {
            Origin::Theme(n, _) => Some(n),
            _ => None,
        })
        .collect();
    let prefixes: Vec<String> = themes
        .iter()
        .map(|n| format!("_theme{n}/"))
        .chain(std::iter::once(crate::EMBEDDED_PREFIX.to_owned()))
        .collect();
    tera.set_fallback_prefixes(prefixes)
        .map_err(TemplateError::Tera)?;
    tera.autoescape_on(crate::AUTOESCAPE_SUFFIXES);
    register(&mut tera);

    let mut raw: Vec<(String, Arc<str>)> = Vec::new();
    for t in &store.templates {
        raw.push((t.name.to_string(), Arc::clone(&t.source)));
        if let Some(alias) = &t.alias {
            raw.push((alias.to_string(), Arc::clone(&t.source)));
        }
    }
    let mut seen = BTreeSet::new();
    for s in sel.iter() {
        let Some(base) = &s.base else { continue };
        let (Some(l), Some(b)) = (store.get(&s.layout), store.get(base)) else {
            continue;
        };
        if &s.render_as == l.render_name() || !seen.insert(&s.render_as) {
            continue;
        }
        if let Some(src) = source::rewrite_extends(&l.source, b.name.as_str()) {
            raw.push((s.render_as.to_string(), Arc::from(src)));
        }
    }
    tera.add_raw_templates(raw.iter().map(|(n, s)| (n.as_str(), &**s)))
        .map_err(TemplateError::Tera)?;
    Ok(Templates {
        tera,
        store,
        uses: Mutex::new(HashMap::new()),
    })
}

impl Templates {
    /// The Tera instance.
    #[must_use]
    pub fn tera(&self) -> &tera::Tera {
        &self.tera
    }

    /// The store the lookups run on.
    #[must_use]
    pub fn store(&self) -> &Arc<LayoutStore> {
        &self.store
    }

    /// Whether template `t` may read top-level variable `var` (Tera's
    /// `get_template_variables`: the template, its parents and its includes; `if` branches are
    /// not evaluated). `false` for an unknown template.
    #[must_use]
    pub fn uses_variable(&self, t: &TemplateName, var: &str) -> bool {
        let key = (t.clone(), var.to_owned());
        let mut memo = self.uses.lock().unwrap_or_else(PoisonError::into_inner);
        *memo.entry(key).or_insert_with(|| {
            self.tera
                .get_template_variables(t.as_str())
                .is_ok_and(|vars| vars.contains(var))
        })
    }

    /// The layout (and base) of a page in an output format; see [`LayoutStore::select`].
    #[must_use]
    pub fn select(&self, q: &LayoutQuery<'_>) -> Option<Selection> {
        self.store.select(q)
    }

    /// The template of shortcode `name` for a page in `fmt` (called with `<` delimiters).
    #[must_use]
    pub fn shortcode(
        &self,
        name: &str,
        scope: &ContentKey,
        fmt: FormatId,
        lang: LangIdx,
    ) -> Option<TemplateName> {
        self.store
            .shortcode(&ShortcodeQuery {
                name,
                path: scope,
                kind: Some(PageKind::Page),
                lang: Some(lang),
                format: fmt,
                markdown: false,
            })
            .ok()
    }

    /// The template of a shortcode, with the reason when there is none.
    ///
    /// # Errors
    /// See [`LayoutStore::shortcode`].
    pub fn shortcode_query(&self, q: &ShortcodeQuery<'_>) -> Result<TemplateName, ShortcodeMiss> {
        self.store.shortcode(q)
    }

    /// The render hook for `h` (with `variant`, a code block's language) in a page at `scope`.
    #[must_use]
    pub fn hook(
        &self,
        h: HookKind,
        variant: Option<&str>,
        scope: &ContentKey,
        fmt: FormatId,
        lang: LangIdx,
        embedded: EmbeddedHooks,
    ) -> Option<TemplateName> {
        self.store.hook(&HookQuery {
            hook: h,
            variant,
            path: scope,
            kind: Some(PageKind::Page),
            lang: Some(lang),
            format: fmt,
            embedded,
        })
    }

    /// The template of partial `name`; see [`LayoutStore::partial`].
    #[must_use]
    pub fn partial(&self, name: &str) -> Option<TemplateName> {
        self.store.partial(name)
    }
}

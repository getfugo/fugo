//! Theme configurations merged into the project's (the end of step 4 of the A1 pipeline),
//! with Go's `_merge` strategies.
//!
//! The project's values always win: a theme only adds keys the project does not have. Which
//! keys it may add is decided per table of the project by a [`MergeStrategy`]: the table's
//! `_merge` when written, else the default for its key, else its parent's strategy.
//!
//! | table | default |
//! |---|---|
//! | `params`, at any level (`languages.X.params` too) | `deep` |
//! | `menus` and `languages.X.menus` | `shallow` |
//! | `outputFormats`, `mediaTypes` | `shallow` |
//! | any other key of the root | `none`; the root's `_merge` when it is written |
//! | a table below those | its parent's strategy |
//!
//! - `deep` adds the theme's keys at every level, `shallow` only to the table it is set on (not
//!   to the tables inside it), `none` adds nothing to its table; a table present on both
//!   sides is merged by its own strategy (so `languages.X.params` merges although `languages`
//!   is `none`).
//! - At the root, a table the project does not have is taken from the theme when its default
//!   strategy is not `none`; a value that is not a table (`title`, `baseURL`, …) only when the
//!   root's `_merge` is `deep`; `_merge = "none"` at the root ignores every theme's
//!   configuration.
//! - A project without `[languages]` has one implicit language (`defaultContentLanguage`, else
//!   `en`): languages a theme defines are added, but the implicit language takes nothing from
//!   the theme (unless the root is `deep`). Every project language has a `params` table for the
//!   theme's `languages.X.params`.
//! - Themes are merged in precedence order (see [`crate::Theme`]), so an earlier theme wins
//!   over a later one. `_merge` written in a theme applies to the tables it brings in when a
//!   later theme is merged into them.
//! - A theme's `theme`, `module`, `themesDir` and bootstrap settings (`environment`,
//!   `configDir`, `cacheDir`, `workingDir`) describe the theme or were needed before the
//!   themes were found; they are never merged.

use std::collections::BTreeMap;
use std::sync::Arc;

use ssg_base::{Map, Value};

use crate::de;

/// The key holding a table's strategy.
pub const MERGE_KEY: &str = "_merge";

/// Root keys of a theme's configuration that are never merged into the project's.
const THEME_ONLY_KEYS: [&str; 7] = [
    "theme",
    "module",
    "themesdir",
    "environment",
    "configdir",
    "cachedir",
    "workingdir",
];

/// How a theme's values are merged into a table of the project (`_merge`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeStrategy {
    /// The theme adds nothing to the table.
    None,
    /// The theme adds the keys the table does not have, but nothing to the tables inside it.
    Shallow,
    /// The theme adds the keys the table does not have, at every level.
    Deep,
}

impl MergeStrategy {
    /// The strategy `_merge` names: `none`, `shallow` or `deep` (ignoring case); any other
    /// value means `deep`, as in Go.
    #[must_use]
    pub fn parse(v: &Value) -> Self {
        match de::weak_string(v)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "none" => Self::None,
            "shallow" => Self::Shallow,
            _ => Self::Deep,
        }
    }

    /// The `_merge` written in `table`.
    #[must_use]
    pub fn written(table: &Map) -> Option<Self> {
        table.get(MERGE_KEY).map(Self::parse)
    }

    /// The default strategy of the project's table at `path` (lower-case keys from the root;
    /// not empty) whose parent has strategy `parent` (the root's: only when written there).
    #[must_use]
    pub fn default_for(path: &[&str], parent: Option<Self>) -> Self {
        let at_root = path.len() == 1;
        match path.last().copied().unwrap_or_default() {
            "params" => Self::Deep,
            "outputformats" | "mediatypes" if at_root => Self::Shallow,
            "menus" if at_root || (path.len() == 3 && path[0] == "languages") => Self::Shallow,
            _ => parent.unwrap_or(Self::None),
        }
    }
}

/// A table path from the root (lower-case keys).
type Path = Vec<Arc<str>>;

/// The merge of theme configurations into a project tree: the strategies of the project's
/// tables that have no `_merge` of their own (their defaults, and those of the tables the
/// merge creates).
struct Merger {
    defaults: BTreeMap<Path, MergeStrategy>,
}

impl Merger {
    /// Records the default strategy of every table of the project below the root that has no
    /// `_merge` (tables inside lists have none: lists are values).
    fn new(root: &Map) -> Self {
        fn walk(
            m: &Map,
            path: &mut Path,
            parent: Option<MergeStrategy>,
            out: &mut BTreeMap<Path, MergeStrategy>,
        ) {
            for (k, v) in m.iter() {
                let Value::Map(child) = v else { continue };
                path.push(Arc::from(k));
                let strategy = MergeStrategy::written(child).unwrap_or_else(|| {
                    let keys: Vec<&str> = path.iter().map(AsRef::as_ref).collect();
                    let s = MergeStrategy::default_for(&keys, parent);
                    out.insert(path.clone(), s);
                    s
                });
                walk(child, path, Some(strategy), out);
                path.pop();
            }
        }
        let mut defaults = BTreeMap::new();
        walk(
            root,
            &mut Vec::new(),
            MergeStrategy::written(root),
            &mut defaults,
        );
        Self { defaults }
    }

    /// The strategy of the project table `table` at `path`: its `_merge`, else its default.
    fn own(&self, table: &Map, path: &Path) -> Option<MergeStrategy> {
        MergeStrategy::written(table).or_else(|| self.defaults.get(path).copied())
    }

    /// Adds the keys of the theme's table `src` that the project's table `dst` (at `path`)
    /// lacks, as its strategy allows; `parent` is the strategy of the table holding `dst`
    /// (`None` at the top of a merge). A table without a strategy of its own (one a theme
    /// brought in) takes its parent's, else `shallow`.
    fn merge_table(
        &mut self,
        dst: &mut Map,
        path: &mut Path,
        parent: Option<MergeStrategy>,
        src: &Map,
    ) {
        let strategy = self
            .own(dst, path)
            .or(parent)
            .unwrap_or(MergeStrategy::Shallow);
        let adds = strategy != MergeStrategy::None && parent != Some(MergeStrategy::Shallow);
        for (k, v) in src.iter() {
            if k == MERGE_KEY {
                continue;
            }
            match dst.get_mut(k) {
                Some(Value::Map(d)) => {
                    if let Value::Map(s) = v {
                        path.push(Arc::from(k));
                        self.merge_table(Arc::make_mut(d), path, Some(strategy), s);
                        path.pop();
                    }
                }
                Some(_) => {}
                None if adds => {
                    dst.insert(k, v.clone());
                }
                None => {}
            }
        }
    }

    /// Merges one theme's configuration into the project's root.
    fn merge_theme(&mut self, root: &mut Map, theme: &Map) {
        let root_strategy = MergeStrategy::written(root);
        if root_strategy == Some(MergeStrategy::None) {
            return;
        }
        let theme: Map = theme
            .iter()
            .filter(|(k, _)| !THEME_ONLY_KEYS.contains(k))
            .map(|(k, v)| (k, v.clone()))
            .collect();
        // Each table of the theme into the project's table of the same key, or into a new
        // table with the key's default strategy.
        let mut created = Vec::new();
        for (k, v) in theme.iter() {
            let Value::Map(src) = v else { continue };
            let mut path: Path = vec![Arc::from(k)];
            match root.get_mut(k) {
                Some(Value::Map(dst)) => self.merge_table(Arc::make_mut(dst), &mut path, None, src),
                // The project's value wins (Go fails here).
                Some(_) => {}
                None => {
                    let strategy = MergeStrategy::default_for(&[k], root_strategy);
                    self.defaults.insert(path.clone(), strategy);
                    let mut table = Map::new();
                    self.merge_table(&mut table, &mut path, None, src);
                    created.push(k.to_owned());
                    root.insert(k, Value::map(table));
                }
            }
        }
        // Then the root itself: other values only with `_merge = "deep"` at the root.
        self.merge_table(
            root,
            &mut Vec::new(),
            Some(root_strategy.unwrap_or(MergeStrategy::Shallow)),
            &theme,
        );
        for k in created {
            if root
                .get(&k)
                .and_then(Value::as_map)
                .is_some_and(Map::is_empty)
            {
                root.remove(&k);
            }
        }
    }
}

/// Merges the theme configurations `themes` (highest precedence first; keys normalised) into
/// the project's configuration tree `root` (see the module docs).
pub fn merge_themes<'a>(root: &mut Map, themes: impl IntoIterator<Item = &'a Map>) {
    let mut merger = Merger::new(root);
    let implicit = ImplicitLanguages::add(root, &mut merger);
    for theme in themes {
        merger.merge_theme(root, theme);
    }
    implicit.remove_unused(root);
}

/// The tables Go's languages step adds to the project before themes are merged: the implicit
/// language of a project without `[languages]` (no strategy of its own), and a `params` table
/// (`deep`) in every language without one. Those still empty after the merge are removed.
struct ImplicitLanguages {
    /// The implicit language, when the project has no `[languages]`.
    language: Option<String>,
    /// Languages given an empty `params` table.
    params: Vec<String>,
}

impl ImplicitLanguages {
    fn add(root: &mut Map, merger: &mut Merger) -> Self {
        let language = (!root.contains_key("languages")).then(|| {
            let key = root
                .get("defaultcontentlanguage")
                .and_then(de::weak_string)
                .filter(|s| !s.is_empty())
                .map_or_else(|| "en".to_owned(), |s| s.to_lowercase());
            let mut languages = Map::new();
            languages.insert(key.as_str(), Value::map(Map::new()));
            root.insert("languages", Value::map(languages));
            key
        });
        let mut params = Vec::new();
        if let Some(Value::Map(languages)) = root.get_mut("languages") {
            for (key, table) in Arc::make_mut(languages).iter_mut() {
                let Value::Map(table) = table else { continue };
                if !table.contains_key("params") {
                    Arc::make_mut(table).insert("params", Value::map(Map::new()));
                    let path: Path = ["languages", key, "params"].map(Arc::from).to_vec();
                    merger.defaults.insert(path, MergeStrategy::Deep);
                    params.push(key.to_owned());
                }
            }
        }
        Self { language, params }
    }

    fn remove_unused(self, root: &mut Map) {
        let Some(Value::Map(languages)) = root.get_mut("languages") else {
            return;
        };
        let languages = Arc::make_mut(languages);
        for key in &self.params {
            if let Some(Value::Map(table)) = languages.get_mut(key)
                && table
                    .get("params")
                    .and_then(Value::as_map)
                    .is_some_and(Map::is_empty)
            {
                Arc::make_mut(table).remove("params");
            }
        }
        let only_implicit = self.language.as_deref().is_some_and(|key| {
            languages.len() == 1
                && languages
                    .get(key)
                    .and_then(Value::as_map)
                    .is_some_and(Map::is_empty)
        });
        if only_implicit {
            root.remove("languages");
        }
    }
}

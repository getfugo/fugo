//! Phase A4's translations: the i18n files of the project and its themes.

use std::cmp::Reverse;

use ssg_config::Config;
use ssg_locale::{Translations, TranslationsBuilder};
use ssg_vfs::{Component, Vfs};

use crate::RenderError;

/// The translations of every i18n file of `vfs`, in increasing precedence (the last theme
/// first, the project last): a message replaces an earlier one with the same key and
/// language.
pub(crate) fn load(vfs: &Vfs, cfg: &Config) -> Result<Translations, RenderError> {
    let mut files = vfs.walk(Component::I18n)?;
    // `walk` sorts by path, then mount precedence; the builder wants the lowest first. The sort
    // is stable, so files of one mount stay in path order.
    files.sort_by_key(|f| Reverse(f.mount_idx));
    let mut builder = TranslationsBuilder::new(&cfg.default_site().language.key);
    for f in &files {
        let content = std::fs::read_to_string(&f.abs).map_err(|source| RenderError::Io {
            path: f.abs.clone(),
            source,
        })?;
        builder
            .add_file(&f.abs, &content)
            .map_err(|e| RenderError::I18n(Box::new(e)))?;
    }
    Ok(builder.build(cfg.sites.iter().map(|s| s.language.key.as_str())))
}

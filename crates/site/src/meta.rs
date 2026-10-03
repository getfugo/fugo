//! Phase B2: cascade, then typed front matter and dates, for every placed page.
//!
//! The cascade indexes are built by one walk per language in tree order; the per-page work
//! (applying the cascade, [`meta_from_params`], resolving dates) runs in parallel.

use rayon::prelude::*;
use ssg_base::diag::{Diagnostic, Position};
use ssg_base::paths::ContentKey;
use ssg_base::{IdVec, Idx, LangIdx, PageKind, Params, Value};
use ssg_config::Config;
use ssg_page::{
    Cascade, Cjk, DateResolver, FileCtx, MatchCtx, MetaCtx, PageMeta, meta_from_adapter,
    meta_from_params,
};

use crate::ModelError;
use crate::cascade::CascadeIndex;
use crate::tree::{Assembly, PageRole};

/// The cascade indexes of every language: the site's `[[cascade]]` at the root, then each
/// branch page's own cascade, in tree order.
pub(crate) fn cascade_indexes(
    cfg: &Config,
    a: &Assembly,
) -> Result<IdVec<LangIdx, CascadeIndex>, ModelError> {
    cfg.sites
        .iter_enumerated()
        .map(|(lang, site)| {
            let site_cascade =
                Cascade::from_config(&site.cascade).map_err(ModelError::SiteCascade)?;
            let mut index = CascadeIndex::new(site_cascade);
            for &i in a.trees[lang.index()].values() {
                let p = &a.pages[i];
                if p.kind.is_branch() {
                    index.add_branch(&p.key, &p.page.cascade);
                }
            }
            Ok(index)
        })
        .collect()
}

/// A page's params after the cascade, and its typed meta.
pub(crate) fn metas(
    cfg: &Config,
    a: &Assembly,
    cascades: &IdVec<LangIdx, CascadeIndex>,
) -> Result<(Vec<PageMeta>, Vec<Diagnostic>), ModelError> {
    let resolvers: IdVec<LangIdx, DateResolver> =
        cfg.sites.iter().map(DateResolver::from_site).collect();
    let results: Vec<Result<PageMeta, ModelError>> = a
        .pages
        .par_iter()
        .map(|p| {
            let site = &cfg.sites[p.page.lang];
            let own = if p.kind.is_branch() && p.role == PageRole::Standalone {
                &p.page.cascade
            } else {
                &Cascade::default()
            };
            let cascade = cascades[p.page.lang].in_force(&p.key, own);
            let match_ctx = MatchCtx {
                kind: p.kind,
                path: &p.key.to_path(),
                lang: &site.language.key,
                environment: &cfg.environment,
            };
            let src = &p.page.source;
            let adapter = p.page.adapter.as_deref();
            let ctx = MetaCtx {
                kind: p.kind,
                formats: &cfg.output_formats,
                media_types: &cfg.media_types,
                sitemap: &site.sitemap,
                cjk_default: if site.has_cjk_language {
                    Cjk::Detect
                } else {
                    Cjk::No
                },
                // An adapter page has no file of its own: no extension, no file dates.
                ext: if adapter.is_some() {
                    ""
                } else {
                    &src.file_info.ext
                },
                dates: &resolvers[p.page.lang],
                file: adapter.is_none().then(|| FileCtx {
                    base_filename: src.base_filename(),
                    mod_time: src.mod_time,
                    git_author_date: None,
                }),
                time_zone: &site.language.time_zone,
            };
            if let Some(adapter) = adapter {
                // The cascade's fields fill the `add_page` map, its params the params.
                let mut fields = adapter.fields.clone();
                let mut params = Params::default();
                if !cascade.is_empty() {
                    cascade.apply_split(&match_ctx, &mut fields, &mut params);
                }
                let mut meta = meta_from_adapter(adapter, &fields, params, &ctx).map_err(|e| {
                    ModelError::Adapter {
                        path: src.file.abs.clone(),
                        message: format!("page {:?}: {e}", p.key.to_path()),
                    }
                })?;
                if meta.cjk == Cjk::Detect {
                    meta.cjk = if src.body().chars().any(is_cjk) {
                        Cjk::Yes
                    } else {
                        Cjk::No
                    };
                }
                return Ok(meta);
            }
            let mut params = p.page.params.clone();
            apply_cascade(&cascade, &match_ctx, &mut params);
            let mut meta = meta_from_params(params, &ctx).map_err(|source| ModelError::Page {
                path: src.file.abs.clone(),
                source,
            })?;
            if meta.cjk == Cjk::Detect {
                meta.cjk = if src.body().chars().any(is_cjk) {
                    Cjk::Yes
                } else {
                    Cjk::No
                };
                meta.params
                    .insert("iscjklanguage", &Value::Bool(meta.cjk == Cjk::Yes));
            }
            Ok(meta)
        })
        .collect();
    let mut metas = Vec::with_capacity(results.len());
    let mut diagnostics = Vec::new();
    for (p, r) in a.pages.iter().zip(results) {
        let meta = r?;
        for key in &meta.unparsable_dates {
            diagnostics.push(
                Diagnostic::error(format!("front matter {key:?} is not a date"))
                    .with_id("front-matter-date")
                    .at(Position {
                        file: p.page.source.file.abs.clone().into(),
                        line: 1,
                        col: 1,
                    }),
            );
        }
        metas.push(meta);
    }
    Ok((metas, diagnostics))
}

/// Han, Hiragana, Katakana and Hangul (`hasCJKLanguage` detection).
fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{1100}'..='\u{11FF}'
        | '\u{2E80}'..='\u{2FDF}'
        | '\u{3005}'
        | '\u{3007}'
        | '\u{3021}'..='\u{3029}'
        | '\u{3038}'..='\u{303B}'
        | '\u{3041}'..='\u{30FF}'
        | '\u{3130}'..='\u{318F}'
        | '\u{31F0}'..='\u{31FF}'
        | '\u{32D0}'..='\u{32FE}'
        | '\u{3400}'..='\u{4DBF}'
        | '\u{4E00}'..='\u{9FFF}'
        | '\u{A960}'..='\u{A97F}'
        | '\u{AC00}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FAFF}'
        | '\u{FF66}'..='\u{FFDC}'
        | '\u{20000}'..='\u{3134F}')
}

/// Fills the page's missing front matter from the cascade entries that match it.
fn apply_cascade(c: &Cascade, m: &MatchCtx<'_>, params: &mut Params) {
    if !c.is_empty() {
        c.apply(m, params);
    }
}

/// The front matter of a page without a file (an auto node of T23b) after the cascade in force
/// at its key.
#[must_use]
pub fn cascaded_params(
    cfg: &Config,
    index: &CascadeIndex,
    lang: LangIdx,
    kind: PageKind,
    key: &ContentKey,
) -> Params {
    let mut params = Params::default();
    apply_cascade(
        index.received(key),
        &MatchCtx {
            kind,
            path: &key.to_path(),
            lang: &cfg.sites[lang].language.key,
            environment: &cfg.environment,
        },
        &mut params,
    );
    params
}

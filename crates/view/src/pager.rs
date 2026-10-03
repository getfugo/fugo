//! Pager values (`paginator()`, `paginate()`) and the files and links of a page's pagers.

use ssg_base::{FormatId, PageId};
use ssg_nav::{PagerSlice, PaginationItems};
use ssg_page::{Links, TargetPaths, links};
use ssg_site::{Model, ModelError};

use crate::cache::ViewGeneration;
use crate::pagination::Recorded;
use crate::views::{PagerLink, PagerView};

/// Why a pager's file or link could not be made.
#[derive(Debug, thiserror::Error)]
pub enum TargetError {
    #[error("{path} ({format}): {source}")]
    Links {
        path: String,
        format: String,
        #[source]
        source: ssg_page::PageError,
    },
    #[error("{path} has no output in format {format}")]
    NoOutput { path: String, format: String },
    #[error(transparent)]
    Model(#[from] Box<ModelError>),
}

/// The file and links of page `id` in format `f`: its own output for `pager` `None`, else
/// pager `n` below it (`/<pagination.path>/<n>`; `n` = 1 is the `page/1/` alias file).
///
/// # Errors
/// The page has no output in `f`, or the pager's URL cannot be made.
pub fn page_target(
    model: &Model,
    id: PageId,
    f: FormatId,
    pager: Option<u32>,
) -> Result<(TargetPaths, Links), TargetError> {
    let p = &model.pages[id];
    let site = &model.config.sites[p.lang];
    let format = model.config.output_formats.get(f);
    let t = match pager {
        Some(n) => {
            let element = format!("/{}/{n}", site.pagination.path);
            model.pager_paths(id, f, &element).map_err(Box::new)?
        }
        None => match p.url(f) {
            Some(u) => u.paths.clone(),
            None => {
                return Err(TargetError::NoOutput {
                    path: p.path(),
                    format: format.name.clone(),
                });
            }
        },
    };
    let l = links(&t, &site.site_urls(), format).map_err(|source| TargetError::Links {
        path: p.path(),
        format: format.name.clone(),
        source,
    })?;
    Ok((t, l))
}

/// The relative link of pager `number` of (`id`, `f`): the page's own for pager 1.
///
/// # Errors
/// See [`page_target`].
pub fn pager_url(
    model: &Model,
    id: PageId,
    f: FormatId,
    number: u32,
) -> Result<String, TargetError> {
    let (_, l) = page_target(model, id, f, (number > 1).then_some(number))?;
    Ok(l.rel_permalink.escaped())
}

/// Pager `number` of a recorded pagination as a value of generation `g` (its pages are that
/// generation's summaries; grouped paginations give `[{key, pages}]`); `url` makes the pager
/// links.
///
/// # Errors
/// What `url` returns.
pub fn pager_view<E>(
    g: &ViewGeneration,
    rec: &Recorded,
    number: u32,
    url: impl Fn(u32) -> Result<String, E>,
) -> Result<PagerView, E> {
    let p = &rec.pagination;
    let total = rec.total_pages();
    let link = |n: u32| -> Result<PagerLink, E> {
        Ok(PagerLink {
            page_number: n,
            url: url(n)?,
        })
    };
    let pages = match (
        p.items(),
        p.pager(usize::try_from(number).unwrap_or(usize::MAX)),
    ) {
        (_, None) => g.list(&[]),
        (PaginationItems::Pages(_), Some(pager)) => g.list(p.pages_of(&pager)),
        (PaginationItems::Groups(groups), Some(pager)) => {
            let slices = match &pager.slice {
                PagerSlice::Groups(s) => s.clone(),
                PagerSlice::Pages(_) => Vec::new(),
            };
            tera::Value::from(
                slices
                    .iter()
                    .map(|s| {
                        let group = &groups[s.group];
                        let mut m = tera::Map::new();
                        m.insert("key".into(), group.key.to_tera());
                        m.insert("pages".into(), g.list(&group.pages[s.range.clone()]));
                        tera::Value::from(m)
                    })
                    .collect::<Vec<_>>(),
            )
        }
    };
    Ok(PagerView {
        page_number: number,
        url: url(number)?,
        pages,
        pager_size: p.size(),
        total_pages: total,
        total_number_of_elements: p.total_items(),
        has_prev: number > 1,
        has_next: number < total,
        prev: if number > 1 {
            Some(link(number - 1)?)
        } else {
            None
        },
        next: if number < total {
            Some(link(number + 1)?)
        } else {
            None
        },
        first: link(1)?,
        last: link(total)?,
        pagers: (1..=total).map(link).collect::<Result<_, _>>()?,
    })
}

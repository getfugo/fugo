//! `paginator()` and `paginate(pages=, size=?)` (REWRITE_PLAN.md §3.3).
//!
//! The first call per (page, format) records its pagination and position in the
//! [`PaginationRecorder`]; `paginator()` later returns the record, `paginate` with an equal
//! pagination too, and `paginate` with another list or size is an error naming both call
//! positions. The pager shown is `__nh.pager` (wave 2 renders pagers 2..N), also inside
//! `partial()`, whose child scope keeps the pager.

use std::path::Path;
use std::sync::Arc;

use ssg_base::diag::Position;
use ssg_nav::{PageGroup, Pagination, PaginationItems, default_pagination_list};
use ssg_view::{
    PaginationRecorder, Phase, Recorded, RenderScope, ViewCache, pager_url, pager_view,
};
use tera::{Kwargs, State, TeraResult, Value};

use crate::call::{
    Registrar, SiteFunction, chain, field, generation, list, msg, need_scope, page_ids, to_data,
};
use crate::{Frames, Handles};

pub(crate) fn register(r: &mut Registrar<'_>, h: &Handles) {
    r.function(
        "paginator",
        Paginate {
            views: Arc::clone(&h.views),
            recorder: Arc::clone(&h.pagination),
            frames: Arc::clone(&h.frames),
            custom: false,
        },
    );
    r.function(
        "paginate",
        Paginate {
            views: Arc::clone(&h.views),
            recorder: Arc::clone(&h.pagination),
            frames: Arc::clone(&h.frames),
            custom: true,
        },
    );
}

/// `paginator()` (`custom: false`) and `paginate(pages=, size=?)` (`custom: true`).
struct Paginate {
    views: Arc<ViewCache>,
    recorder: Arc<PaginationRecorder>,
    frames: Arc<Frames>,
    custom: bool,
}

impl Paginate {
    fn name(&self) -> &'static str {
        if self.custom { "paginate" } else { "paginator" }
    }

    /// Where the call runs: the partial template of the scope's frame, else the layout of the
    /// page (Tera does not tell functions their call position).
    fn position(&self, s: &RenderScope) -> Position {
        let model = self.views.model();
        let file = match s.frame.and_then(|f| self.frames.template(f)) {
            Some(t) => t.to_string(),
            None => format!(
                "layout of {} ({})",
                model.pages[s.page].path(),
                model.config.output_formats.get(s.format).name
            ),
        };
        Position {
            file: Arc::from(Path::new(&file)),
            line: 0,
            col: 0,
        }
    }

    /// The pagination `paginate(pages=, size=)` asks for.
    fn custom_pagination(&self, kw: &Kwargs, s: &RenderScope) -> TeraResult<Pagination> {
        let model = self.views.model();
        let pages = kw.must_get::<Value>("pages")?;
        let size_arg: Vec<ssg_base::Value> = kw
            .get::<Value>("size")?
            .filter(|v| !v.is_none())
            .map(|v| vec![to_data(&v)])
            .unwrap_or_default();
        let configured = model.config.sites[s.lang].pagination.pager_size;
        let size = ssg_nav::resolve_pager_size(&size_arg, configured)
            .map_err(|e| chain("paginate(size=)", e))?;
        let grouped = pages
            .as_array()
            .and_then(<[Value]>::first)
            .is_some_and(|g| field(g, "pages").is_some() && field(g, "key").is_some());
        let items = if grouped {
            let groups = list(&pages, "paginate(pages=)")?
                .iter()
                .map(|g| {
                    Ok(PageGroup {
                        key: to_data(field(g, "key").unwrap_or(&Value::none())),
                        pages: page_ids(
                            &self.views,
                            field(g, "pages").unwrap_or(&Value::none()),
                            "paginate(pages=) group",
                        )?
                        .into(),
                    })
                })
                .collect::<TeraResult<Vec<_>>>()?;
            PaginationItems::Groups(groups.into())
        } else {
            PaginationItems::Pages(page_ids(&self.views, &pages, "paginate(pages=)")?.into())
        };
        Pagination::new(items, size).map_err(|e| chain("paginate", e))
    }

    fn recorded(&self, kw: &Kwargs, s: &RenderScope) -> TeraResult<Arc<Recorded>> {
        let at = Some(self.position(s));
        if self.custom {
            let p = self.custom_pagination(kw, s)?;
            return self
                .recorder
                .paginate(s.page, s.format, at, p)
                .map_err(|e| msg(format!("paginate: {e}")));
        }
        if let Some(r) = self.recorder.get(s.page, s.format) {
            return Ok(r);
        }
        let model = self.views.model();
        let size = model.config.sites[s.lang].pagination.pager_size.max(1);
        let items = PaginationItems::Pages(default_pagination_list(self.views.nav(), s.page));
        let p = Pagination::new(items, size).map_err(|e| chain("paginator", e))?;
        Ok(self.recorder.paginator(s.page, s.format, at, || p))
    }
}

impl SiteFunction for Paginate {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let name = self.name();
        let s = need_scope(st, name)?;
        if s.phase != Phase::Layout {
            return Err(msg(format!(
                "`{name}` is only available in layouts (and their partials), not in {:?} renders",
                s.phase
            )));
        }
        let rec = self.recorded(kw, &s)?;
        let model = self.views.model();
        let pager = pager_view(
            generation(&self.views, Some(&s)),
            &rec,
            s.pager.unwrap_or(1),
            |n| pager_url(model, s.page, s.format, n).map_err(|e| chain(name, e)),
        )?;
        Ok(Value::from_serializable(&pager))
    }
}

//! Phase E5 (REWRITE_PLAN.md §3.1, §3.4), the deferred wave: every `defer(...)` key registered
//! by waves 1 and 2 renders its template once (in parallel over keys); every post-process
//! placeholder handed out so far is resolved (its pending transforms run now, after every page
//! was rendered); placeholders inside deferred output are resolved too. The replacement map
//! goes to `Publisher::patch_held`, which re-extracts the URL tokens of the patched outputs,
//! then minifies and writes them.

use std::collections::BTreeMap;

use rayon::prelude::*;
use ssg_publish::Publisher;
use ssg_render::{RenderError, Session};
use ssg_resources::PpField;

use crate::{BuildError, RenderPool};

/// Phase E5; returns the number of held outputs written.
pub(crate) fn run(
    session: &Session,
    publisher: &Publisher,
    pool: &RenderPool,
) -> Result<usize, BuildError> {
    let handles = session.handles();
    let entries = handles.deferred.entries();
    let rendered: Vec<Result<(String, String), RenderError>> = pool.run(|| {
        entries
            .par_iter()
            .map(|(key, d)| {
                session
                    .render_deferred(key, d)
                    .map(|text| (format!("__nh_defer_{key}__"), text))
            })
            .collect()
    });
    let store = &handles.store;
    let mut repl = BTreeMap::new();
    for r in rendered {
        let (placeholder, text) = r?;
        let text = store.resolve_post_process(&text)?.unwrap_or(text);
        repl.insert(placeholder, text);
    }
    for pp in store.post_processes() {
        for field in PpField::ALL {
            let placeholder = pp.placeholder(field);
            let value = store
                .resolve_post_process(&placeholder)?
                .unwrap_or_default();
            repl.insert(placeholder, value);
        }
    }
    if publisher.held().is_empty() {
        return Ok(0);
    }
    Ok(pool.run(|| publisher.patch_held(&repl))?)
}

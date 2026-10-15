//! What the content editor gets from the build: the URL of each content file's page, and in
//! each page the mark of the element that holds its text.

use std::collections::BTreeMap;

use ssg_base::{FormatId, PageId};
use ssg_config::Config;
use ssg_render::Session;
use ssg_site::Model;

/// Whether the build writes the editor (`[cms]`).
pub(crate) fn in_build(cfg: &Config) -> bool {
    cfg.raw.get("cms").is_some()
}

/// `text`, the output of `page` in HTML format `format`, with the element that holds the page's
/// text marked for the editor's preview ([`ssg_cms::body`]).
pub(crate) fn mark_body(session: &Session, page: PageId, format: FormatId, text: String) -> String {
    session
        .layout_content(page, format)
        .and_then(|c| ssg_cms::body::mark_body(&text, &c.html))
        .unwrap_or(text)
}

/// The URL path of the page of each content file the build rendered (its link in its first
/// format, below the site's root: `/posts/hello/`, `/th/posts/hello/`), by the file's path in the
/// project. The editor keeps it as an alias of a page it moves.
pub(crate) fn content_urls(cfg: &Config, m: &Model) -> BTreeMap<String, String> {
    m.pages
        .iter()
        .filter(|p| p.rendered() && p.linked())
        .filter_map(|p| {
            let source = p.source.as_ref()?;
            let rel = source.file.abs.strip_prefix(&cfg.project_dir).ok()?;
            let link = p.urls.first()?.paths.link.to_string();
            Some((rel.to_string_lossy().replace('\\', "/"), link))
        })
        .collect()
}

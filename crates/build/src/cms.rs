//! What the content editor gets from the build: the URL of each content file's page.

use std::collections::BTreeMap;

use ssg_config::Config;
use ssg_site::Model;

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

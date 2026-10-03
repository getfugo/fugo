//! Titles of pages without a `title`.

use ssg_base::{PageKind, inflect, title};
use ssg_config::site::TitleConfig;

/// The title of a page that has no file and no `title` (pages backed by a file keep an empty
/// title). `raw` is the site title for the home page, the directory name as written for a
/// section, the plural for a taxonomy and the term as first written for a term.
#[must_use]
pub fn default_title(kind: PageKind, raw: &str, cfg: &TitleConfig) -> String {
    let case = |s: &str| {
        if cfg.capitalize {
            title::title_case(s, cfg.case_style)
        } else {
            s.to_owned()
        }
    };
    match kind {
        PageKind::Section => {
            let name = if cfg.pluralize {
                inflect::pluralize(raw)
            } else {
                raw.to_owned()
            };
            case(&name)
        }
        PageKind::Taxonomy => {
            if cfg.capitalize {
                case(raw).replace('-', " ")
            } else {
                raw.to_owned()
            }
        }
        PageKind::Term => case(raw),
        PageKind::NotFound => "404 Page not found".to_owned(),
        PageKind::Home
        | PageKind::Page
        | PageKind::Sitemap
        | PageKind::SitemapIndex
        | PageKind::RobotsTxt => raw.to_owned(),
    }
}

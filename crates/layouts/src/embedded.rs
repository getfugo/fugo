//! The Go implementation's embedded templates rewritten in Tera: the files under
//! `crates/layouts/embedded/` (T32), listed by the build script. Their Tera names carry
//! [`crate::EMBEDDED_PREFIX`].

use crate::name::Origin;
use crate::store::LayoutSource;

/// `(path under embedded/, source)`, sorted by path.
pub const TEMPLATES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

/// The embedded templates as layout sources.
pub(crate) fn sources() -> impl Iterator<Item = LayoutSource> {
    TEMPLATES.iter().map(|(rel, src)| LayoutSource {
        rel: (*rel).to_owned(),
        origin: Origin::Embedded,
        source: (*src).to_owned(),
    })
}

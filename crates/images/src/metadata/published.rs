//! What a build does with the metadata of the images it publishes.

use ssg_config::ImagingConfig;

use super::{Notice, rewrite_metadata};

/// The metadata of published images (`[imaging]`): the originals' own removed
/// (`stripMetadata`), and a notice written into every JPEG, PNG and WebP image, processed or
/// not (`copyright`, `author`, `titleFromFileName`; the configuration allows them only with
/// `stripMetadata`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageMetadata {
    /// Remove the metadata of the images that are not processed.
    pub strip: bool,
    /// The copyright notice (empty: none).
    pub copyright: String,
    /// The author (empty: none).
    pub author: String,
    /// The file name without its extension as the title (a processed image's: its source's).
    pub title_from_file_name: bool,
}

impl Default for ImageMetadata {
    /// `[imaging]`'s default: the metadata removed, nothing written.
    fn default() -> Self {
        Self {
            strip: true,
            ..Self::KEEP
        }
    }
}

impl ImageMetadata {
    /// Images published as they are, as the Go implementation publishes them.
    pub const KEEP: Self = Self {
        strip: false,
        copyright: String::new(),
        author: String::new(),
        title_from_file_name: false,
    };

    /// The settings of `[imaging]`.
    #[must_use]
    pub fn from_config(c: &ImagingConfig) -> Self {
        Self {
            strip: c.strip_metadata,
            copyright: c.copyright.clone(),
            author: c.author.clone(),
            title_from_file_name: c.title_from_file_name,
        }
    }

    /// What an image that is not processed, published as `name` (a path or a file name), is
    /// published with instead of `bytes`: `None` to publish them as they are.
    #[must_use]
    pub fn original(&self, bytes: &[u8], name: &str) -> Option<Vec<u8>> {
        if !self.strip {
            return None;
        }
        rewrite_metadata(bytes, &self.notice(stem(name)))
    }

    /// What a processed image, published as `name` (`<source>_hu_<hash>.<ext>`), is published
    /// with instead of `bytes`, which have no metadata: its source's notice; `None` when there
    /// is none.
    #[must_use]
    pub fn processed(&self, bytes: &[u8], name: &str) -> Option<Vec<u8>> {
        let stem = stem(name);
        let source = stem.rsplit_once("_hu_").map_or(stem, |(s, _)| s);
        let notice = self.notice(source);
        if !self.strip || notice.is_empty() {
            return None;
        }
        rewrite_metadata(bytes, &notice)
    }

    fn notice<'a>(&'a self, stem: &'a str) -> Notice<'a> {
        Notice {
            title: if self.title_from_file_name { stem } else { "" },
            author: &self.author,
            copyright: &self.copyright,
        }
    }
}

/// The file name of `name` without its extension (`/a/pic.v2.jpg` → `pic.v2`).
fn stem(name: &str) -> &str {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    match file.rfind('.') {
        Some(i) if i > 0 => &file[..i],
        _ => file,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_file_names() {
        assert_eq!(stem("/a/pic.v2.jpg"), "pic.v2");
        assert_eq!(stem("pic"), "pic");
        assert_eq!(stem(".hidden"), ".hidden");
        let m = ImageMetadata {
            title_from_file_name: true,
            ..ImageMetadata::default()
        };
        assert_eq!(m.notice(stem("b/harbour_view.jpg")).title, "harbour_view");
        assert_eq!(ImageMetadata::default().notice("x").title, "");
    }
}

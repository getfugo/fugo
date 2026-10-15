//! What a build does with the metadata of the images it publishes.

use ssg_config::ImagingConfig;

use super::{Credit, Notice, rewrite_metadata};

/// The metadata of published images (`[imaging]`): the originals' own removed
/// (`stripMetadata`), and a notice written into every JPEG, PNG and WebP image, processed or
/// not (`copyright`, `author`, `titleFromFileName`; the configuration allows them only with
/// `stripMetadata`), with the copyright and author of an image that gives its own instead
/// ([`Credit`]).
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
    /// The build's year: `{year}` of an image's own copyright notice
    /// ([`credit`](Self::credit)); 0 in [`KEEP`](Self::KEEP) and the default.
    pub year: i16,
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
        year: 0,
    };

    /// The settings of `[imaging]` for a build in `year` (`Config::year`), which `{year}` in
    /// a copyright notice stands for.
    #[must_use]
    pub fn from_config(c: &ImagingConfig, year: i16) -> Self {
        Self {
            strip: c.strip_metadata,
            copyright: c.copyright_in(year),
            author: c.author.clone(),
            title_from_file_name: c.title_from_file_name,
            year,
        }
    }

    /// What an image that is not processed, published as `name` (a path or a file name), is
    /// published with instead of `bytes` (`credit`: the image's own copyright and author, in
    /// place of `[imaging]`'s): `None` to publish them as they are.
    #[must_use]
    pub fn original(&self, bytes: &[u8], name: &str, credit: Option<&Credit>) -> Option<Vec<u8>> {
        if !self.strip {
            return None;
        }
        rewrite_metadata(bytes, &self.notice(stem(name), credit))
    }

    /// What a processed image, published as `name` (`<source>_hu_<hash>.<ext>`), is published
    /// with instead of `bytes`, which have no metadata: its source's notice (`credit`: the
    /// source's own copyright and author, in place of `[imaging]`'s); `None` when there is
    /// none.
    #[must_use]
    pub fn processed(&self, bytes: &[u8], name: &str, credit: Option<&Credit>) -> Option<Vec<u8>> {
        let stem = stem(name);
        let source = stem.rsplit_once("_hu_").map_or(stem, |(s, _)| s);
        let notice = self.notice(source, credit);
        if !self.strip || notice.is_empty() {
            return None;
        }
        rewrite_metadata(bytes, &notice)
    }

    /// The notice of an image named `stem`: the title, and `credit`'s copyright and author in
    /// place of both of `[imaging]`'s.
    fn notice<'a>(&'a self, stem: &'a str, credit: Option<&'a Credit>) -> Notice<'a> {
        let (author, copyright) = match credit {
            Some(c) => (&c.author, &c.copyright),
            None => (&self.author, &self.copyright),
        };
        Notice {
            title: if self.title_from_file_name { stem } else { "" },
            author,
            copyright,
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
        assert_eq!(
            m.notice(stem("b/harbour_view.jpg"), None).title,
            "harbour_view"
        );
        assert_eq!(ImageMetadata::default().notice("x", None).title, "");
    }

    #[test]
    fn the_copyright_takes_the_year() {
        let c = ImagingConfig {
            copyright: "© 2019–{year} A, {year}".to_owned(),
            ..ImagingConfig::default()
        };
        let m = ImageMetadata::from_config(&c, 2031);
        assert_eq!(m.copyright, "© 2019–2031 A, 2031");
        assert_eq!(m.year, 2031);
        assert_eq!(m.notice("x", None).copyright, "© 2019–2031 A, 2031");
    }

    /// A credit replaces both the copyright and the author, an empty one included; the title
    /// stays the file's.
    #[test]
    fn a_credit_replaces_copyright_and_author() {
        let m = ImageMetadata {
            copyright: "© Site".to_owned(),
            author: "Site".to_owned(),
            title_from_file_name: true,
            ..ImageMetadata::default()
        };
        let credit = Credit {
            copyright: String::new(),
            author: "Ada".to_owned(),
        };
        let n = m.notice("eggs", Some(&credit));
        assert_eq!((n.title, n.author, n.copyright), ("eggs", "Ada", ""));
        let n = m.notice("eggs", None);
        assert_eq!((n.author, n.copyright), ("Site", "© Site"));
    }
}

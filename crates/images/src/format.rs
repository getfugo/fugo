//! Image file formats.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An image format that sources can be decoded from and results encoded to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ImageFormat {
    Jpeg,
    Png,
    Gif,
    Tiff,
    Bmp,
    Webp,
}

impl ImageFormat {
    /// Every format, in declaration order.
    pub const ALL: [Self; 6] = [
        Self::Jpeg,
        Self::Png,
        Self::Gif,
        Self::Tiff,
        Self::Bmp,
        Self::Webp,
    ];

    /// The format of a file extension or spec token (`jpg`, `.JPEG`, `tif`, …), ignoring case
    /// and a leading dot.
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Self> {
        let ext = ext.strip_prefix('.').unwrap_or(ext).to_ascii_lowercase();
        Some(match ext.as_str() {
            "jpg" | "jpeg" | "jpe" | "jif" | "jfif" => Self::Jpeg,
            "png" => Self::Png,
            "gif" => Self::Gif,
            "tif" | "tiff" => Self::Tiff,
            "bmp" => Self::Bmp,
            "webp" => Self::Webp,
            _ => return None,
        })
    }

    /// The format of a media subtype (`jpeg`, `png`, …).
    #[must_use]
    pub fn from_subtype(subtype: &str) -> Option<Self> {
        Some(match subtype {
            "jpeg" | "jpg" => Self::Jpeg,
            "png" => Self::Png,
            "gif" => Self::Gif,
            "tiff" => Self::Tiff,
            "bmp" => Self::Bmp,
            "webp" => Self::Webp,
            _ => return None,
        })
    }

    /// The extension of files written in this format, with its dot (`.jpg`).
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => ".jpg",
            Self::Png => ".png",
            Self::Gif => ".gif",
            Self::Tiff => ".tif",
            Self::Bmp => ".bmp",
            Self::Webp => ".webp",
        }
    }

    /// The media type (`image/jpeg`).
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Gif => "image/gif",
            Self::Tiff => "image/tiff",
            Self::Bmp => "image/bmp",
            Self::Webp => "image/webp",
        }
    }

    /// Whether the format can store transparent pixels.
    #[must_use]
    pub const fn supports_transparency(self) -> bool {
        !matches!(self, Self::Jpeg)
    }

    /// Whether the encoder takes a quality (JPEG and WebP).
    #[must_use]
    pub const fn has_quality(self) -> bool {
        matches!(self, Self::Jpeg | Self::Webp)
    }

    /// The largest width or height the format can store, when an image can reach it (WebP's
    /// 14-bit sizes, the 16-bit ones of JPEG and GIF).
    #[must_use]
    pub const fn max_side(self) -> Option<u32> {
        match self {
            Self::Webp => Some(16383),
            Self::Jpeg | Self::Gif => Some(65535),
            Self::Png | Self::Tiff | Self::Bmp => None,
        }
    }

    pub(crate) fn from_codec(f: image::ImageFormat) -> Option<Self> {
        Some(match f {
            image::ImageFormat::Jpeg => Self::Jpeg,
            image::ImageFormat::Png => Self::Png,
            image::ImageFormat::Gif => Self::Gif,
            image::ImageFormat::Tiff => Self::Tiff,
            image::ImageFormat::Bmp => Self::Bmp,
            image::ImageFormat::WebP => Self::Webp,
            _ => return None,
        })
    }
}

impl fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.extension()[1..])
    }
}

impl Serialize for ImageFormat {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ImageFormat {
    /// Any extension [`ImageFormat::from_extension`] accepts.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::from_extension(&s)
            .ok_or_else(|| serde::de::Error::custom(format_args!("unknown image format {s:?}")))
    }
}

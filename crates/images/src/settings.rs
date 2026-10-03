//! The site-wide `[imaging]` settings, typed.

use regex::Regex;
use ssg_base::Value;
use ssg_config::ImagingConfig;

use crate::color::Color;
use crate::error::ImageError;
use crate::spec::{Anchor, Hint, Resample};

/// The EXIF fields excluded when neither `includeFields` nor `excludeFields` is set.
pub const DEFAULT_EXIF_EXCLUDE: &str = "GPS|Exif|Exposure[M|P|B]|Contrast|Resolution|Sharp|JPEG|Metering|Sensing|Saturation|ColorSpace|Flash|WhiteBalance";

/// `[imaging]`: the defaults of processing specs and the EXIF reader settings.
#[derive(Clone, Debug)]
pub struct Imaging {
    pub resample: Resample,
    /// The anchor of crop and fill when the spec names none (`smart` by default).
    pub anchor: Anchor,
    pub hint: Hint,
    /// JPEG and WebP quality, 1–100.
    pub quality: u8,
    /// The colour transparent results are flattened onto for formats without transparency.
    pub background: Color,
    pub exif: ExifSettings,
}

impl Default for Imaging {
    fn default() -> Self {
        Self {
            resample: Resample::Box,
            anchor: Anchor::Smart,
            hint: Hint::Photo,
            quality: 75,
            background: Color::WHITE,
            exif: ExifSettings::default(),
        }
    }
}

/// `[imaging.exif]`.
#[derive(Clone, Debug)]
pub struct ExifSettings {
    /// Only tags whose name matches are kept (case-insensitive unless the pattern starts
    /// with `(`).
    pub include: Option<Regex>,
    /// Tags whose name matches are dropped.
    pub exclude: Option<Regex>,
    pub date: ExifPart,
    pub lat_long: ExifPart,
}

/// Whether a derived EXIF value (date, position) is computed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExifPart {
    Enabled,
    Disabled,
}

impl Default for ExifSettings {
    fn default() -> Self {
        Self {
            include: None,
            exclude: fields_regex("excludeFields", DEFAULT_EXIF_EXCLUDE)
                .ok()
                .flatten(),
            date: ExifPart::Enabled,
            lat_long: ExifPart::Enabled,
        }
    }
}

fn fields_regex(key: &'static str, expr: &str) -> Result<Option<Regex>, ImageError> {
    let expr = expr.trim();
    if expr.is_empty() {
        return Ok(None);
    }
    let pattern = if expr.starts_with('(') {
        expr.to_owned()
    } else {
        format!("(?i){expr}")
    };
    Regex::new(&pattern)
        .map(Some)
        .map_err(|e| ImageError::Config {
            key,
            reason: e.to_string(),
        })
}

impl ExifSettings {
    fn from_map(m: &ssg_base::Map) -> Result<Self, ImageError> {
        let get = |name: &str| {
            m.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v)
        };
        let text = |key: &'static str, name: &str| -> Result<String, ImageError> {
            match get(name) {
                None | Some(Value::Null) => Ok(String::new()),
                Some(Value::String(s)) => Ok(s.to_string()),
                Some(other) => Err(ImageError::Config {
                    key,
                    reason: format!("expected a string, found {other:?}"),
                }),
            }
        };
        let flag = |key: &'static str, name: &str| -> Result<ExifPart, ImageError> {
            match get(name) {
                None | Some(Value::Null | Value::Bool(false)) => Ok(ExifPart::Enabled),
                Some(Value::Bool(true)) => Ok(ExifPart::Disabled),
                Some(other) => Err(ImageError::Config {
                    key,
                    reason: format!("expected a boolean, found {other:?}"),
                }),
            }
        };
        let include = text("imaging.exif.includeFields", "includeFields")?;
        let mut exclude = text("imaging.exif.excludeFields", "excludeFields")?;
        if include.trim().is_empty() && exclude.trim().is_empty() {
            DEFAULT_EXIF_EXCLUDE.clone_into(&mut exclude);
        }
        Ok(Self {
            include: fields_regex("imaging.exif.includeFields", &include)?,
            exclude: fields_regex("imaging.exif.excludeFields", &exclude)?,
            date: flag("imaging.exif.disableDate", "disableDate")?,
            lat_long: flag("imaging.exif.disableLatLong", "disableLatLong")?,
        })
    }

    /// Whether a tag of this name is kept.
    #[must_use]
    pub fn keeps(&self, tag: &str) -> bool {
        !self.exclude.as_ref().is_some_and(|re| re.is_match(tag))
            && self.include.as_ref().is_none_or(|re| re.is_match(tag))
    }
}

impl Imaging {
    /// Types `[imaging]` as the config crate decoded it.
    ///
    /// # Errors
    /// An unknown resample filter, anchor or hint, a quality outside 1–100, an invalid
    /// background colour, or invalid EXIF settings.
    pub fn from_config(c: &ImagingConfig) -> Result<Self, ImageError> {
        let config = |key: &'static str| move |reason: String| ImageError::Config { key, reason };
        let named = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_owned());
        let d = Self::default();
        Ok(Self {
            resample: c
                .resample_filter
                .trim()
                .parse()
                .map_err(config("imaging.resampleFilter"))?,
            anchor: named(&c.anchor)
                .map_or(Ok(d.anchor), |s| s.parse())
                .map_err(config("imaging.anchor"))?,
            hint: named(&c.hint)
                .map_or(Ok(d.hint), |s| s.parse())
                .map_err(config("imaging.hint"))?,
            quality: u8::try_from(c.quality)
                .ok()
                .filter(|q| (1..=100).contains(q))
                .ok_or_else(|| config("imaging.quality")(format!("{} is not 1–100", c.quality)))?,
            background: c.bg_color.parse()?,
            exif: ExifSettings::from_map(&c.exif)?,
        })
    }
}

//! The text filter (`images.Text`): text drawn onto the image, wrapped and aligned as the Go
//! implementation's `resources/images/text.go` does.
//!
//! Layout, in 26.6 fixed point where Go uses it ([`crate::font`]):
//!
//! * `font_height` is the ceiling of the face's ascent; the available width is
//!   `width − 20 − x` (left), `x` (right) or `2·min(width − 20 − x, x)` (center);
//! * `\r` is dropped and the text split at `\n`; each line is split into whitespace-separated
//!   words and a word starts a new line when `ceil(width of the line so far, with its trailing
//!   space) + ceil(width of the word) >= available` — so a first word that is too wide leaves
//!   an empty line before it, as in Go;
//! * the first baseline is `y + font_height`, moved up by half (`center`) or all (`bottom`) of
//!   `lines·font_height + (lines − 1)·line_spacing`; lines are trimmed and placed at `x`,
//!   `x − ceil(width)` (right) or `x − ceil(width)/2` (center), `font_height + line_spacing`
//!   apart.

use std::path::PathBuf;

use image::RgbaImage;
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::error::ImageError;
use crate::font::{Face, FontId, ceil_26_6};

named_enum! {
    /// The horizontal alignment of text lines at `x`.
    pub enum AlignX ("horizontal alignment") {
        Left = "left",
        Center = "center",
        Right = "right",
    }
}

named_enum! {
    /// The vertical alignment of the text block at `y`.
    pub enum AlignY ("vertical alignment") {
        Top = "top",
        Center = "center",
        Bottom = "bottom",
    }
}

/// The font of a text filter: a font file, or font bytes registered with
/// [`ImageQueue::add_font`](crate::ImageQueue::add_font) (an integer in template maps).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FontInput {
    Registered(FontId),
    File(PathBuf),
}

/// The options of the text filter.
///
/// In template maps (the Go implementation's option names, any case; `line_spacing`, `align_x` and
/// `align_y` are accepted too): `text` (required), `color` (`#rrggbb`, white by default), `size`
/// (pixels, 20), `x` and `y` (10), `alignx` (`left`, `center`, `right`), `aligny` (`top`, `center`,
/// `bottom`), `linespacing` (pixels between lines, 2), `font` (Go Regular by default). Numbers may
/// be given as strings; `x`, `y` and `linespacing` are truncated to integers like Go's
/// `cast.ToInt`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawText")]
pub struct TextSpec {
    pub text: String,
    pub color: Color,
    pub size: f64,
    pub x: i64,
    pub y: i64,
    #[serde(rename = "alignx")]
    pub align_x: AlignX,
    #[serde(rename = "aligny")]
    pub align_y: AlignY,
    #[serde(rename = "linespacing")]
    pub line_spacing: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font: Option<FontInput>,
}

impl TextSpec {
    /// `text` with the Go implementation's defaults.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: Color::WHITE,
            size: 20.0,
            x: 10,
            y: 10,
            align_x: AlignX::Left,
            align_y: AlignY::Top,
            line_spacing: 2,
            font: None,
        }
    }

    /// The largest size, in pixels.
    pub const MAX_SIZE: f64 = 10_000.0;
    /// The largest magnitude of `x`, `y` and `line_spacing`, in pixels.
    pub const MAX_OFFSET: i64 = 1 << 24;

    pub(crate) fn check(self) -> Result<Self, ImageError> {
        if !self.size.is_finite() || self.size <= 0.0 || self.size > Self::MAX_SIZE {
            return Err(ImageError::filter(
                "text",
                format!(
                    "size {} must be a positive number up to {}",
                    self.size,
                    Self::MAX_SIZE
                ),
            ));
        }
        for (name, v) in [
            ("x", self.x),
            ("y", self.y),
            ("linespacing", self.line_spacing),
        ] {
            if !(-Self::MAX_OFFSET..=Self::MAX_OFFSET).contains(&v) {
                return Err(ImageError::filter(
                    "text",
                    format!("{name} {v} is out of range (±{})", Self::MAX_OFFSET),
                ));
            }
        }
        Ok(self)
    }
}

/// A number option: an integer, a float or a numeric string.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Number {
    Int(i64),
    Float(f64),
}

impl Number {
    /// `cast.ToInt`: floats are truncated toward zero.
    fn to_int(self) -> i64 {
        match self {
            Self::Int(v) => v,
            Self::Float(v) => v.trunc() as i64,
        }
    }

    fn to_float(self) -> f64 {
        match self {
            Self::Int(v) => v as f64,
            Self::Float(v) => v,
        }
    }
}

impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct NumberVisitor;

        impl Visitor<'_> for NumberVisitor {
            type Value = Number;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a number or a numeric string")
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Number, E> {
                Ok(Number::Int(v))
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Number, E> {
                i64::try_from(v)
                    .map(Number::Int)
                    .map_err(|_| E::custom(format!("{v} is too large")))
            }

            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Number, E> {
                if v.is_finite() {
                    Ok(Number::Float(v))
                } else {
                    Err(E::custom(format!("{v} is not a finite number")))
                }
            }

            fn visit_str<E: de::Error>(self, s: &str) -> Result<Number, E> {
                let t = s.trim();
                if let Ok(v) = t.parse::<i64>() {
                    return Ok(Number::Int(v));
                }
                t.parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .map(Number::Float)
                    .ok_or_else(|| E::custom(format!("{s:?} is not a number")))
            }
        }

        d.deserialize_any(NumberVisitor)
    }
}

/// The text filter as written in a template map.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawText {
    #[serde(alias = "Text", alias = "TEXT")]
    text: String,
    #[serde(default, alias = "Color", alias = "COLOR")]
    color: Option<Color>,
    #[serde(default, alias = "Size", alias = "SIZE")]
    size: Option<Number>,
    #[serde(default, alias = "X")]
    x: Option<Number>,
    #[serde(default, alias = "Y")]
    y: Option<Number>,
    #[serde(
        default,
        alias = "alignX",
        alias = "AlignX",
        alias = "align_x",
        alias = "ALIGNX"
    )]
    alignx: Option<AlignX>,
    #[serde(
        default,
        alias = "alignY",
        alias = "AlignY",
        alias = "align_y",
        alias = "ALIGNY"
    )]
    aligny: Option<AlignY>,
    #[serde(
        default,
        alias = "lineSpacing",
        alias = "LineSpacing",
        alias = "line_spacing",
        alias = "LINESPACING"
    )]
    linespacing: Option<Number>,
    #[serde(default, alias = "Font", alias = "FONT")]
    font: Option<FontInput>,
}

impl TryFrom<RawText> for TextSpec {
    type Error = ImageError;

    fn try_from(r: RawText) -> Result<Self, ImageError> {
        let d = Self::new(r.text);
        Self {
            color: r.color.unwrap_or(d.color),
            size: r.size.map_or(d.size, Number::to_float),
            x: r.x.map_or(d.x, Number::to_int),
            y: r.y.map_or(d.y, Number::to_int),
            align_x: r.alignx.unwrap_or(d.align_x),
            align_y: r.aligny.unwrap_or(d.align_y),
            line_spacing: r.linespacing.map_or(d.line_spacing, Number::to_int),
            font: r.font,
            ..d
        }
        .check()
    }
}

/// One laid-out line: its text (trimmed) and pen position (pixels, `y` on the baseline).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Line {
    pub text: String,
    pub x: i64,
    pub y: i64,
}

/// The Go implementation's line breaking and placement of `spec` on an image `width` pixels wide.
pub(crate) fn layout(face: &Face<'_>, spec: &TextSpec, width: u32) -> Vec<Line> {
    let max_width = i64::from(width) - 20;
    let available = match spec.align_x {
        AlignX::Right => spec.x,
        AlignX::Center => (max_width - spec.x).min(spec.x) * 2,
        AlignX::Left => max_width - spec.x,
    };
    let font_height = ceil_26_6(face.ascent());

    let mut lines: Vec<String> = Vec::new();
    for line in spec.text.replace('\r', "").split('\n') {
        let mut current = String::new();
        for word in line.split_whitespace() {
            let word_width = ceil_26_6(face.measure(word));
            let current_width = ceil_26_6(face.measure(&current));
            if current_width + word_width >= available {
                lines.push(std::mem::take(&mut current));
            }
            current.push_str(word);
            current.push(' ');
        }
        lines.push(current);
    }

    let count = i64::try_from(lines.len()).unwrap_or(i64::MAX);
    let total_height = count * font_height + (count - 1) * spec.line_spacing;
    let mut y = spec.y + font_height;
    match spec.align_y {
        AlignY::Top => {}
        AlignY::Center => y -= total_height / 2,
        AlignY::Bottom => y -= total_height,
    }
    lines
        .into_iter()
        .map(|line| {
            let text = line.trim().to_owned();
            let width = ceil_26_6(face.measure(&text));
            let x = match spec.align_x {
                AlignX::Right => spec.x - width,
                AlignX::Center => spec.x - width / 2,
                AlignX::Left => spec.x,
            };
            let placed = Line { text, x, y };
            y += font_height + spec.line_spacing;
            placed
        })
        .collect()
}

/// Draws `spec` with the font `font` (its bytes) onto `img`.
pub(crate) fn draw(img: &mut RgbaImage, spec: &TextSpec, font: &[u8]) -> Result<(), ImageError> {
    let face = Face::new(font, spec.size, "text filter font")?;
    for line in layout(&face, spec, img.width()) {
        face.draw(img, &line.text, (line.x << 6, line.y << 6), spec.color);
    }
    Ok(())
}

#[cfg(test)]
mod tests;

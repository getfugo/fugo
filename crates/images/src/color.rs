//! Colours given as hexadecimal strings (`#fff`, `#abc123`, `#12345678`).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::ImageError;

/// A non-premultiplied 8-bit RGBA colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Color(pub [u8; 4]);

impl Color {
    pub const WHITE: Self = Self([255, 255, 255, 255]);
    pub const TRANSPARENT: Self = Self([0, 0, 0, 0]);

    /// Whether the colour is fully opaque.
    #[must_use]
    pub const fn is_opaque(self) -> bool {
        self.0[3] == 255
    }
}

impl FromStr for Color {
    type Err = ImageError;

    /// Parses `rgb`, `rgba`, `rrggbb` or `rrggbbaa` hexadecimal digits, with or without a
    /// leading `#`, in any case.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ImageError::Color(s.to_owned());
        let hex = s.strip_prefix('#').unwrap_or(s);
        let digit = |c: u8| (c as char).to_digit(16).ok_or_else(err);
        let bytes = hex.as_bytes();
        let mut out = [255u8; 4];
        match bytes.len() {
            3 | 4 => {
                for (o, &c) in out.iter_mut().zip(bytes) {
                    let d = u8::try_from(digit(c)?).map_err(|_| err())?;
                    *o = d * 17;
                }
            }
            6 | 8 => {
                for (o, pair) in out.iter_mut().zip(bytes.chunks(2)) {
                    let hi = digit(pair[0])?;
                    let lo = digit(pair[1])?;
                    *o = u8::try_from(hi * 16 + lo).map_err(|_| err())?;
                }
            }
            _ => return Err(err()),
        }
        Ok(Self(out))
    }
}

impl fmt::Display for Color {
    /// `#rrggbb`, or `#rrggbbaa` when not opaque.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b, a] = self.0;
        write!(f, "#{r:02x}{g:02x}{b:02x}")?;
        if a != 255 {
            write!(f, "{a:02x}")?;
        }
        Ok(())
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

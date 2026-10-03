//! EXIF metadata (`.Exif`): the capture date, the GPS position and the tags `[imaging.exif]`
//! keeps, read with `kamadak-exif` from JPEG, TIFF, PNG and WebP files.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

use exif::{Field, In, Reader, Tag, Value as ExifValue};
use jiff::civil::DateTime;
use ssg_base::{Date, Value};

use crate::settings::{ExifPart, ExifSettings};

/// The EXIF metadata of an image.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Exif {
    /// When the picture was taken (`DateTimeOriginal`), as a wall-clock time.
    pub date: Option<Date>,
    /// Degrees north (negative south) and east (negative west).
    pub lat_long: Option<(f64, f64)>,
    /// The kept tags by name (exiftool names, e.g. `ModifyDate`, `ISO`, `ExposureTime`).
    pub tags: BTreeMap<String, Value>,
}

/// The EXIF orientation (1–8) of an encoded image, if it has one.
#[must_use]
pub fn orientation(bytes: &[u8]) -> Option<u8> {
    let exif = Reader::new()
        .read_from_container(&mut Cursor::new(bytes))
        .ok()?;
    let v = exif
        .get_field(Tag::Orientation, In::PRIMARY)?
        .value
        .get_uint(0)?;
    u8::try_from(v).ok().filter(|v| (1..=8).contains(v))
}

/// Reads the EXIF metadata of an encoded image. `None` when it has none (or it cannot be
/// parsed).
#[must_use]
pub fn read(bytes: &[u8], settings: &ExifSettings) -> Option<Exif> {
    let exif = Reader::new()
        .read_from_container(&mut Cursor::new(bytes))
        .ok()?;
    let primary = |tag| exif.get_field(tag, In::PRIMARY);
    let date = (settings.date == ExifPart::Enabled)
        .then(|| parse_date(primary(Tag::DateTimeOriginal)?))
        .flatten();
    let lat_long = (settings.lat_long == ExifPart::Enabled)
        .then(|| {
            let lat = gps_degrees(
                primary(Tag::GPSLatitude)?,
                primary(Tag::GPSLatitudeRef),
                b'S',
            )?;
            let long = gps_degrees(
                primary(Tag::GPSLongitude)?,
                primary(Tag::GPSLongitudeRef),
                b'W',
            )?;
            Some((lat, long))
        })
        .flatten();
    let tags = exif
        .fields()
        .filter(|f| f.ifd_num == In::PRIMARY)
        .filter_map(|f| {
            let name = tag_name(f.tag)?;
            settings
                .keeps(&name)
                .then(|| to_value(&f.value).map(|v| (name, v)))
                .flatten()
        })
        .collect();
    Some(Exif {
        date,
        lat_long,
        tags,
    })
}

/// The exiftool name of a tag (the names the Go implementation's templates use), or `None` for tags
/// without a known name.
fn tag_name(tag: Tag) -> Option<String> {
    let renamed = match tag {
        Tag::DateTime => "ModifyDate",
        Tag::DateTimeDigitized => "CreateDate",
        Tag::PixelXDimension => "ExifImageWidth",
        Tag::PixelYDimension => "ExifImageHeight",
        Tag::PhotographicSensitivity => "ISO",
        Tag::ExposureBiasValue => "ExposureCompensation",
        Tag::FocalLengthIn35mmFilm => "FocalLengthIn35mmFormat",
        Tag::ImageWidth => "ImageWidth",
        Tag::ImageLength => "ImageHeight",
        Tag::OffsetTime => "OffsetTime",
        Tag::LensSpecification => "LensInfo",
        Tag::BodySerialNumber => "SerialNumber",
        _ => {
            let name = tag.to_string();
            // Unknown tags print as `Tag(Context, number)`.
            return (!name.contains('(')).then_some(name);
        }
    };
    Some(renamed.to_owned())
}

fn ascii(field: &Field) -> Option<String> {
    match &field.value {
        ExifValue::Ascii(parts) => parts
            .first()
            .map(|p| String::from_utf8_lossy(p).trim().to_owned()),
        _ => None,
    }
}

/// `YYYY:MM:DD HH:MM:SS` as a wall-clock time (the offset tags are ignored, as in Go).
fn parse_date(field: &Field) -> Option<Date> {
    let text = ascii(field)?;
    DateTime::strptime("%Y:%m:%d %H:%M:%S", &text)
        .ok()
        .map(Date::Local)
}

fn gps_degrees(field: &Field, reference: Option<&Field>, negative: u8) -> Option<f64> {
    let ExifValue::Rational(parts) = &field.value else {
        return None;
    };
    let v = parts
        .iter()
        .zip([1.0, 60.0, 3600.0])
        .map(|(r, div)| r.to_f64() / div)
        .sum::<f64>();
    let south_or_west = reference
        .and_then(ascii)
        .is_some_and(|r| r.as_bytes().first() == Some(&negative));
    v.is_finite().then_some(if south_or_west { -v } else { v })
}

fn rational(num: i64, denom: i64) -> Value {
    if denom == 1 {
        Value::Int(num)
    } else if denom == 0 {
        Value::Null
    } else {
        Value::String(Arc::from(format!("{num}/{denom}")))
    }
}

/// A tag value: text as a string, one number as a number (rationals as `"n/d"` strings, as
/// the Go implementation prints them), several numbers as a space-separated string.
fn to_value(v: &ExifValue) -> Option<Value> {
    fn many<T: ToString>(items: &[T]) -> Value {
        Value::String(Arc::from(
            items
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" "),
        ))
    }
    fn number<T: Copy + Into<i64> + ToString>(items: &[T]) -> Option<Value> {
        match items {
            [] => None,
            [one] => Some(Value::Int((*one).into())),
            _ => Some(many(items)),
        }
    }
    match v {
        ExifValue::Ascii(parts) => Some(Value::String(Arc::from(
            parts
                .iter()
                .map(|p| String::from_utf8_lossy(p).trim().to_owned())
                .collect::<Vec<_>>()
                .join(" "),
        ))),
        ExifValue::Byte(b) => number(b),
        ExifValue::Short(s) => number(s),
        ExifValue::Long(l) => number(l),
        ExifValue::SByte(b) => number(b),
        ExifValue::SShort(s) => number(s),
        ExifValue::SLong(l) => number(l),
        ExifValue::Rational(r) => match r.as_slice() {
            [one] => Some(rational(i64::from(one.num), i64::from(one.denom))),
            [] => None,
            _ => Some(many(
                &r.iter()
                    .map(|x| format!("{}/{}", x.num, x.denom))
                    .collect::<Vec<_>>(),
            )),
        },
        ExifValue::SRational(r) => match r.as_slice() {
            [one] => Some(rational(i64::from(one.num), i64::from(one.denom))),
            [] => None,
            _ => Some(many(
                &r.iter()
                    .map(|x| format!("{}/{}", x.num, x.denom))
                    .collect::<Vec<_>>(),
            )),
        },
        ExifValue::Float(f) => f.first().map(|&v| Value::Float(f64::from(v))),
        ExifValue::Double(d) => d.first().map(|&v| Value::Float(v)),
        // Undefined bytes: text when printable (`ExifVersion` "0230"), one byte as a
        // number (`FileSource`); other blobs (maker notes) are not useful in templates.
        ExifValue::Undefined(b, _) => {
            let text = b.strip_suffix(&[0]).unwrap_or(b);
            if !text.is_empty() && text.iter().all(|c| c.is_ascii_graphic() || *c == b' ') {
                Some(Value::String(Arc::from(
                    String::from_utf8_lossy(text).trim(),
                )))
            } else {
                number(b)
            }
        }
        ExifValue::Unknown(..) => None,
    }
}

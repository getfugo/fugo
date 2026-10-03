//! Decoding sources and encoding results.

use std::fs::File;
use std::io::{BufRead, BufReader, Cursor, Seek};
use std::path::Path;

use image::codecs::bmp::BmpEncoder;
use image::codecs::gif::GifEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::tiff::TiffEncoder;
use image::{ColorType, ExtendedColorType, ImageEncoder, RgbaImage};

use crate::error::ImageError;
use crate::format::ImageFormat;
use crate::gift::{GoType, Pix, Source};
use crate::jpeg;
use crate::jpegdec::{self, GoJpeg};
use crate::pixels::flatten;
use crate::plan::{Encode, Size};
use crate::spec::Hint;

/// Decoded pixels, and whether the source was greyscale (kept greyscale when possible).
pub(crate) struct Decoded {
    pub image: RgbaImage,
    pub gray: bool,
    /// How Go's decoder holds these pixels: the smart crop analysis reads them as gift reads
    /// that type (`gift.rs`).
    pub go: GoType,
    /// The 16-bit samples of a 16-bit source (non-premultiplied RGBA), for the analysis
    /// ([`decode`] with `analysis`).
    pub wide: Option<Vec<u16>>,
    /// A JPEG as Go's decoder decodes it ([`decode`] with `analysis`), for the analysis.
    pub go_jpeg: Option<GoJpeg>,
}

impl Decoded {
    /// The pixels as the smart crop analysis reads them: Go's decoding of a JPEG when it
    /// was asked for and Go can decode the file, else these pixels as Go's type holds them.
    pub(crate) fn source(&self) -> Source<'_> {
        let source = |width, height, ty, pix| Source {
            width,
            height,
            ty,
            pix,
        };
        match &self.go_jpeg {
            Some(GoJpeg::Gray {
                width,
                height,
                pix,
                stride,
            }) => source(
                *width,
                *height,
                GoType::Gray,
                Pix::Gray8 {
                    pix,
                    stride: *stride,
                },
            ),
            Some(GoJpeg::YCbCr {
                width,
                height,
                planes,
            }) => source(*width, *height, GoType::YCbCr, Pix::YCbCr(planes)),
            Some(GoJpeg::Rgba { width, height, pix }) => {
                source(*width, *height, GoType::Rgba, Pix::Rgba8(pix))
            }
            Some(GoJpeg::Cmyk { width, height, pix }) => {
                source(*width, *height, GoType::Cmyk, Pix::Cmyk(pix))
            }
            None => {
                let (w, h) = self.image.dimensions();
                let pix = match &self.wide {
                    Some(wide) => Pix::Rgba16(wide),
                    None => Pix::Rgba8(self.image.as_raw()),
                };
                source(w as usize, h as usize, self.go, pix)
            }
        }
    }
}

/// The type Go's decoders give an image (`image/png`, `image/jpeg`, `image/gif`,
/// `golang.org/x/image/{bmp,tiff,webp}`), from the format and the decoded layout.
/// Paletted PNGs (IHDR colour type 3) are told apart by their header; WebP is read as
/// `*image.NRGBA` (as gift reads the lossy formats without their planes).
fn go_type(format: Option<image::ImageFormat>, color: ColorType, bytes: &[u8]) -> GoType {
    use image::ImageFormat as F;
    let paletted_png = bytes.len() > 25 && &bytes[12..16] == b"IHDR" && bytes[25] == 3;
    match (format, color) {
        (Some(F::Gif), _) => GoType::Paletted,
        (Some(F::Png), _) if paletted_png => GoType::Paletted,
        (Some(F::Jpeg), ColorType::L8 | ColorType::L16) => GoType::Gray,
        (Some(F::Jpeg), _) => GoType::YCbCr,
        (Some(F::WebP), _) => GoType::Nrgba,
        (_, ColorType::L8) => GoType::Gray,
        (_, ColorType::Rgb8) => GoType::Rgba,
        (_, ColorType::L16) => GoType::Gray16,
        (_, ColorType::Rgb16) => GoType::Rgba64,
        (_, ColorType::La16 | ColorType::Rgba16) => GoType::Nrgba64,
        _ => GoType::Nrgba,
    }
}

/// The size and format of an encoded image, from its header.
pub(crate) fn probe(bytes: &[u8], what: &str) -> Result<(Size, ImageFormat), ImageError> {
    probe_reader(image::ImageReader::new(Cursor::new(bytes)), what)
}

/// [`probe`] of a file, reading its header only.
pub(crate) fn probe_file(path: &Path) -> Result<(Size, ImageFormat), ImageError> {
    let file = File::open(path).map_err(|e| ImageError::io(path, e))?;
    probe_reader(
        image::ImageReader::new(BufReader::new(file)),
        &path.display().to_string(),
    )
}

fn probe_reader<R: BufRead + Seek>(
    reader: image::ImageReader<R>,
    what: &str,
) -> Result<(Size, ImageFormat), ImageError> {
    let decode_err = |source| ImageError::Decode {
        what: what.to_owned(),
        source,
    };
    let reader = reader
        .with_guessed_format()
        .map_err(|e| decode_err(image::ImageError::IoError(e)))?;
    let format = reader
        .format()
        .and_then(ImageFormat::from_codec)
        .ok_or_else(|| {
            decode_err(image::ImageError::Unsupported(
                image::error::UnsupportedError::from_format_and_kind(
                    image::error::ImageFormatHint::Unknown,
                    image::error::UnsupportedErrorKind::Format(
                        image::error::ImageFormatHint::Unknown,
                    ),
                ),
            ))
        })?;
    let size = reader.into_dimensions().map_err(decode_err)?;
    Ok((size, format))
}

/// Decodes an image (the first frame of an animation). With `analysis`, a JPEG is also
/// decoded as Go decodes it and a 16-bit source keeps its 16-bit samples, for the smart crop
/// analysis ([`Decoded::source`]); a JPEG Go's decoder rejects is analysed from these pixels.
pub(crate) fn decode(bytes: &[u8], what: &str, analysis: bool) -> Result<Decoded, ImageError> {
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ImageError::Decode {
            what: what.to_owned(),
            source: image::ImageError::IoError(e),
        })?;
    let format = reader.format();
    let img = reader.decode().map_err(|source| ImageError::Decode {
        what: what.to_owned(),
        source,
    })?;
    let color = img.color();
    let gray = matches!(color, ColorType::L8 | ColorType::L16);
    let go = go_type(format, color, bytes);
    let wide = (analysis && matches!(go, GoType::Gray16 | GoType::Rgba64 | GoType::Nrgba64))
        .then(|| img.to_rgba16().into_raw());
    let go_jpeg = (analysis && format == Some(image::ImageFormat::Jpeg))
        .then(|| jpegdec::decode(bytes).ok())
        .flatten();
    Ok(Decoded {
        image: img.into_rgba8(),
        gray,
        go,
        wide,
        go_jpeg,
    })
}

fn is_opaque(img: &RgbaImage) -> bool {
    img.pixels().all(|p| p.0[3] == 255)
}

fn is_gray(img: &RgbaImage) -> bool {
    img.pixels().all(|p| p.0[0] == p.0[1] && p.0[1] == p.0[2])
}

/// The channel layout written: RGBA when some pixel is transparent, else RGB, or luma for a
/// greyscale source whose result is still grey.
fn packed(img: &RgbaImage, gray_source: bool) -> (Vec<u8>, ExtendedColorType) {
    if !is_opaque(img) {
        return (img.as_raw().clone(), ExtendedColorType::Rgba8);
    }
    if gray_source && is_gray(img) {
        return (
            img.pixels().map(|p| p.0[0]).collect(),
            ExtendedColorType::L8,
        );
    }
    let rgb = img
        .pixels()
        .flat_map(|p| [p.0[0], p.0[1], p.0[2]])
        .collect();
    (rgb, ExtendedColorType::Rgb8)
}

/// The libwebp settings of a hint (libwebp's presets) at `quality`, with sharp YUV.
fn webp_config(quality: u8, hint: Hint) -> Result<webp::WebPConfig, String> {
    let mut c = webp::WebPConfig::new().map_err(|()| "libwebp config version mismatch")?;
    c.quality = f32::from(quality);
    let (sns, sharpness, strength) = match hint {
        Hint::Picture => (80, 4, 35),
        Hint::Photo => (80, 3, 30),
        Hint::Drawing => (25, 6, 10),
        Hint::Icon | Hint::Text => (0, c.filter_sharpness, 0),
    };
    c.sns_strength = sns;
    c.filter_sharpness = sharpness;
    c.filter_strength = strength;
    // Bit 2: dithering, for photos only.
    if hint == Hint::Photo {
        c.preprocessing |= 2;
    } else {
        c.preprocessing &= !2;
    }
    if hint == Hint::Text {
        c.segments = 2;
    }
    c.use_sharp_yuv = 1;
    Ok(c)
}

/// Encodes `img` as the plan says: flattened onto the background when the format cannot
/// store transparency or a background was asked for.
pub(crate) fn encode(
    img: RgbaImage,
    gray_source: bool,
    enc: &Encode,
) -> Result<Vec<u8>, ImageError> {
    let encode_err = |reason: String| ImageError::Encode {
        format: enc.format,
        reason,
    };
    let background = match enc.background {
        Some(bg) => Some(bg),
        None if !enc.format.supports_transparency() => Some(enc.default_background),
        None => None,
    };
    let img = match background {
        Some(mut bg) if !is_opaque(&img) => {
            if !enc.format.supports_transparency() {
                bg.0[3] = 255;
            }
            flatten(&img, bg)
        }
        _ => img,
    };
    let (w, h) = img.dimensions();
    let mut out = Vec::new();
    match enc.format {
        ImageFormat::Jpeg => {
            // Go's encoder, as the Go implementation uses it: 4:2:0 from the RGB of the (opaque,
            // flattened) result, or one component for a greyscale source whose result is still grey
            // (Go's `*image.Gray`).
            let (bytes, layout) = packed(&img, gray_source);
            let pixels = match layout {
                ExtendedColorType::L8 => jpeg::Pixels::Gray(&bytes),
                ExtendedColorType::Rgb8 => jpeg::Pixels::Rgb(&bytes),
                _ => jpeg::Pixels::Rgba(&bytes),
            };
            out = jpeg::encode(pixels, w, h, i32::from(enc.quality))
                .map_err(|e| encode_err(e.to_string()))?;
        }
        ImageFormat::Png => {
            let (bytes, layout) = packed(&img, gray_source);
            PngEncoder::new(&mut out)
                .write_image(&bytes, w, h, layout)
                .map_err(|e| encode_err(e.to_string()))?;
        }
        ImageFormat::Gif => {
            let mut e = GifEncoder::new_with_speed(&mut out, 10);
            e.encode_frame(image::Frame::new(img))
                .map_err(|e| encode_err(e.to_string()))?;
        }
        ImageFormat::Tiff => {
            let (bytes, layout) = packed(&img, gray_source);
            TiffEncoder::new(Cursor::new(&mut out))
                .write_image(&bytes, w, h, layout)
                .map_err(|e| encode_err(e.to_string()))?;
        }
        ImageFormat::Bmp => {
            let (bytes, layout) = packed(&img, gray_source);
            BmpEncoder::new(&mut out)
                .write_image(&bytes, w, h, layout)
                .map_err(|e| encode_err(e.to_string()))?;
        }
        ImageFormat::Webp => {
            let config = webp_config(enc.quality, enc.hint).map_err(encode_err)?;
            let memory = if is_opaque(&img) {
                let rgb: Vec<u8> = img
                    .pixels()
                    .flat_map(|p| [p.0[0], p.0[1], p.0[2]])
                    .collect();
                webp::Encoder::from_rgb(&rgb, w, h).encode_advanced(&config)
            } else {
                webp::Encoder::from_rgba(img.as_raw(), w, h).encode_advanced(&config)
            }
            .map_err(|e| encode_err(format!("{e:?}")))?;
            out.extend_from_slice(&memory);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webp_presets_match_libwebp() {
        let photo = webp_config(75, Hint::Photo).expect("config");
        assert!((photo.quality - 75.0).abs() < f32::EPSILON);
        assert_eq!(photo.lossless, 0);
        assert_eq!(photo.use_sharp_yuv, 1);
        assert_eq!(
            (
                photo.sns_strength,
                photo.filter_sharpness,
                photo.filter_strength
            ),
            (80, 3, 30)
        );
        assert_eq!(photo.preprocessing & 2, 2);
        let text = webp_config(90, Hint::Text).expect("config");
        assert_eq!(
            (text.sns_strength, text.filter_strength, text.segments),
            (0, 0, 2)
        );
        assert_eq!(text.preprocessing & 2, 0);
        let drawing = webp_config(50, Hint::Drawing).expect("config");
        assert_eq!(
            (
                drawing.sns_strength,
                drawing.filter_sharpness,
                drawing.filter_strength
            ),
            (25, 6, 10)
        );
    }
}

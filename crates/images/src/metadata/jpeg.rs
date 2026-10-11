//! JPEG: the segments without APPn and COM, and nothing after the end of the image; the EXIF
//! and XMP segments written after the start of the image (and its JFIF header).

use super::{Blocks, Notice, orientation};

const APP0: u8 = 0xE0;
const APP1: u8 = 0xE1;
const APP2: u8 = 0xE2;
const APP14: u8 = 0xEE;
const COM: u8 = 0xFE;
const SOS: u8 = 0xDA;
const EOI: u8 = 0xD9;

const XMP_NAMESPACE: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

/// `bytes` (a JPEG, SOI first) with the blocks of `notice` and the orientation of its first
/// EXIF segment as its only metadata.
pub(super) fn rewrite(bytes: &[u8], notice: &Notice) -> Vec<u8> {
    let (body, exif) = strip(bytes);
    let blocks = Blocks::new(exif.and_then(orientation), notice);
    if blocks.is_empty() {
        return body;
    }
    let mut segments = Vec::new();
    if let Some(tiff) = &blocks.exif {
        push_segment(&mut segments, APP1, &[b"Exif\0\0", tiff.as_slice()]);
    }
    if let Some(xmp) = &blocks.xmp {
        push_segment(&mut segments, APP1, &[XMP_NAMESPACE, xmp.as_slice()]);
    }
    // After SOI and a JFIF header, which comes first.
    let mut at = 2;
    if body.get(2..4) == Some(&[0xFF, APP0]) && body.get(6..11) == Some(b"JFIF\0") {
        at += 2 + usize::from(u16::from_be_bytes([body[4], body[5]]));
    }
    [&body[..at], &segments, &body[at..]].concat()
}

/// A segment with `payload` (its parts); none when it is too long for one.
fn push_segment(out: &mut Vec<u8>, code: u8, payload: &[&[u8]]) {
    let len = 2 + payload.iter().map(|p| p.len()).sum::<usize>();
    let Ok(len) = u16::try_from(len) else {
        return;
    };
    out.extend_from_slice(&[0xFF, code]);
    out.extend_from_slice(&len.to_be_bytes());
    for p in payload {
        out.extend_from_slice(p);
    }
}

/// `bytes` without its metadata segments and without what follows the end of the image (the
/// other images of a multi-picture file, an appended video, vendor trailers); the payload of
/// its first EXIF segment.
///
/// Every segment that is not APPn or COM is kept, and of those APP0 `JFIF`, APP2
/// `ICC_PROFILE` and APP14 `Adobe` (its colour transform). Bytes where a marker is due are
/// skipped, as decoders skip them; a file that ends early keeps what it has.
fn strip(bytes: &[u8]) -> (Vec<u8>, Option<&[u8]>) {
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&bytes[..2]);
    let mut exif = None;
    let mut i = 2;
    while i < bytes.len() {
        if bytes[i] != 0xFF {
            i += 1;
            continue;
        }
        // Fill bytes, then the marker's code.
        while bytes.get(i) == Some(&0xFF) {
            i += 1;
        }
        let Some(&code) = bytes.get(i) else {
            break;
        };
        i += 1;
        match code {
            0x00 => continue,
            EOI => {
                out.extend_from_slice(&[0xFF, EOI]);
                break;
            }
            // Markers without a length: TEM, RSTn, SOI.
            0x01 | 0xD0..=0xD8 => {
                out.extend_from_slice(&[0xFF, code]);
                continue;
            }
            _ => {}
        }
        let Some(&[hi, lo]) = bytes.get(i..i + 2) else {
            break;
        };
        let len = usize::from(u16::from_be_bytes([hi, lo]));
        if len < 2 {
            break;
        }
        let end = (i + len).min(bytes.len());
        let payload = &bytes[i + 2..end];
        if keeps(code, payload) {
            out.extend_from_slice(&[0xFF, code]);
            out.extend_from_slice(&bytes[i..end]);
        } else if code == APP1 && payload.starts_with(b"Exif\0\0") && exif.is_none() {
            exif = Some(payload);
        }
        i = end;
        if code == SOS {
            let data_end = entropy_end(bytes, i);
            out.extend_from_slice(&bytes[i..data_end]);
            i = data_end;
        }
    }
    (out, exif)
}

/// Whether a segment that may hold metadata is kept: only the JFIF header, the colour
/// profile and Adobe's colour transform are.
fn keeps(code: u8, payload: &[u8]) -> bool {
    match code {
        APP0 => payload.starts_with(b"JFIF\0"),
        APP2 => payload.starts_with(b"ICC_PROFILE\0"),
        APP14 => payload.starts_with(b"Adobe"),
        APP1 | 0xE3..=0xED | 0xEF | COM => false,
        _ => true,
    }
}

/// Where the entropy-coded data that starts at `from` ends: at the next marker that is not a
/// restart marker (`0xFF 0x00` is a stuffed byte), else at the end of `bytes`.
fn entropy_end(bytes: &[u8], from: usize) -> usize {
    let mut i = from;
    while let Some(at) = bytes[i..].iter().position(|&b| b == 0xFF) {
        i += at;
        match bytes.get(i + 1) {
            Some(0x00 | 0xD0..=0xD7) => i += 2,
            Some(_) => return i,
            None => break,
        }
    }
    bytes.len()
}

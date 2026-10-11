//! PNG: the critical chunks and the ancillary chunks that change how the image looks; the
//! `eXIf` and XMP (`iTXt`) chunks written after `IHDR`.

use super::{Blocks, Notice, orientation};

pub(super) const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// The ancillary chunks kept: transparency, colour, physical size, palette data, and the
/// frames of an animated PNG. Text (`tEXt`, `zTXt`, `iTXt`, which holds XMP), `tIME`, `eXIf`
/// and unknown chunks are not.
const KEPT: [&[u8; 4]; 16] = [
    b"tRNS", b"cHRM", b"gAMA", b"iCCP", b"sBIT", b"sRGB", b"cICP", b"mDCV", b"cLLI", b"bKGD",
    b"hIST", b"pHYs", b"sPLT", b"acTL", b"fcTL", b"fdAT",
];

/// The `iTXt` header of an XMP packet: its keyword, no compression, no language, no
/// translated keyword.
const XMP_ITXT: &[u8] = b"XML:com.adobe.xmp\0\0\0\0\0";

/// `bytes` (a PNG, signature first) with the blocks of `notice` and the orientation of its
/// first `eXIf` as its only metadata.
pub(super) fn rewrite(bytes: &[u8], notice: &Notice) -> Vec<u8> {
    let (body, exif) = strip(bytes);
    let blocks = Blocks::new(exif.and_then(orientation), notice);
    let mut chunks = Vec::new();
    if let Some(tiff) = &blocks.exif {
        push_chunk(&mut chunks, b"eXIf", &[tiff]);
    }
    if let Some(xmp) = &blocks.xmp {
        push_chunk(&mut chunks, b"iTXt", &[XMP_ITXT, xmp]);
    }
    // After IHDR, the first chunk.
    let at = match body.get(8..16) {
        Some(&[l0, l1, l2, l3, b'I', b'H', b'D', b'R']) => {
            (20 + u32::from_be_bytes([l0, l1, l2, l3]) as usize).min(body.len())
        }
        _ => return body,
    };
    [&body[..at], &chunks, &body[at..]].concat()
}

/// `bytes` with its critical chunks and those of [`KEPT`], up to `IEND` (a file that ends early
/// keeps what it has); the data of its first `eXIf`.
fn strip(bytes: &[u8]) -> (Vec<u8>, Option<&[u8]>) {
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(SIGNATURE);
    let mut exif = None;
    let mut i = SIGNATURE.len();
    while let Some(&[l0, l1, l2, l3, k0, k1, k2, k3]) = bytes.get(i..i + 8) {
        let kind = [k0, k1, k2, k3];
        let len = u32::from_be_bytes([l0, l1, l2, l3]) as usize;
        let data_end = (i + 8).saturating_add(len).min(bytes.len());
        // The data, then its CRC.
        let end = data_end.saturating_add(4).min(bytes.len());
        // Critical chunks have an upper-case first letter.
        if k0.is_ascii_uppercase() || KEPT.contains(&&kind) {
            out.extend_from_slice(&bytes[i..end]);
        } else if &kind == b"eXIf" && exif.is_none() {
            exif = Some(&bytes[i + 8..data_end]);
        }
        if &kind == b"IEND" {
            break;
        }
        i = end;
    }
    (out, exif)
}

/// A chunk with `data` (its parts); none when it is too long for one.
fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[&[u8]]) {
    let Ok(len) = u32::try_from(data.iter().map(|d| d.len()).sum::<usize>()) else {
        return;
    };
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(kind);
    let mut crc = crc32fast::Hasher::new();
    crc.update(kind);
    for d in data {
        out.extend_from_slice(d);
        crc.update(d);
    }
    out.extend_from_slice(&crc.finalize().to_be_bytes());
}

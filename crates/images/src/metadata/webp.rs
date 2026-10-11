//! WebP: the chunks of the RIFF container that hold the image, its colour profile and its
//! animation; the `EXIF` and `XMP ` chunks written at the end, in the extended format (a simple
//! file gets the `VP8X` header that format needs).

use super::{Blocks, Notice, orientation};

/// The chunks kept. `EXIF`, `XMP ` and unknown chunks are not.
const KEPT: [&[u8; 4]; 7] = [
    b"VP8X", b"ICCP", b"ANIM", b"ANMF", b"ALPH", b"VP8 ", b"VP8L",
];

/// The flags of `VP8X`: alpha, an `EXIF` and an `XMP ` chunk.
const ALPHA_FLAG: u8 = 0x10;
const EXIF_FLAG: u8 = 0x08;
const XMP_FLAG: u8 = 0x04;

/// `bytes` (a WebP: `RIFF`, its size, `WEBP`) with the blocks of `notice` and the orientation
/// of its first `EXIF` as its only metadata, the flags of `VP8X` and the RIFF size to match.
/// Nothing after the RIFF data is kept; a file that ends early keeps what it has.
pub(super) fn rewrite(bytes: &[u8], notice: &Notice) -> Vec<u8> {
    let riff_size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    let riff_end = riff_size.saturating_add(8).min(bytes.len());
    let mut chunks: Vec<(&[u8], &[u8])> = Vec::new();
    let mut exif = None;
    let mut i = 12;
    while let Some(&[k0, k1, k2, k3, l0, l1, l2, l3]) = bytes[..riff_end].get(i..i + 8) {
        let len = u32::from_le_bytes([l0, l1, l2, l3]) as usize;
        let data_end = (i + 8).saturating_add(len).min(riff_end);
        // Chunks are padded to an even size.
        let end = data_end.saturating_add(len & 1).min(riff_end);
        let kind = [k0, k1, k2, k3];
        if KEPT.contains(&&kind) {
            chunks.push((&bytes[i..i + 4], &bytes[i..end]));
        } else if &kind == b"EXIF" && exif.is_none() {
            exif = Some(&bytes[i + 8..data_end]);
        }
        i = end;
    }
    let blocks = Blocks::new(exif.and_then(orientation), notice);
    let header = match chunks.first() {
        Some((b"VP8X", _)) => None,
        // A simple file needs the extended format's header for metadata.
        Some((_, image)) if !blocks.is_empty() => vp8x(image),
        _ => None,
    };
    let mut out = bytes[..12].to_vec();
    if let Some(h) = &header {
        out.extend_from_slice(h);
    }
    let flags =
        (chunks.first().is_some_and(|(k, _)| *k == b"VP8X") || header.is_some()).then_some(12 + 8);
    for (_, chunk) in &chunks {
        out.extend_from_slice(chunk);
    }
    if let Some(at) = flags {
        out[at] &= !(EXIF_FLAG | XMP_FLAG);
        for (kind, flag, data) in [
            (b"EXIF", EXIF_FLAG, &blocks.exif),
            (b"XMP ", XMP_FLAG, &blocks.xmp),
        ] {
            if let Some(data) = data
                && let Ok(len) = u32::try_from(data.len())
            {
                out.extend_from_slice(kind);
                out.extend_from_slice(&len.to_le_bytes());
                out.extend_from_slice(data);
                if data.len() % 2 == 1 {
                    out.push(0);
                }
                out[at] |= flag;
            }
        }
    }
    let size = u32::try_from(out.len() - 8).unwrap_or(u32::MAX);
    out[4..8].copy_from_slice(&size.to_le_bytes());
    out
}

/// The `VP8X` chunk of a simple file whose image chunk is `image`: its canvas size (and alpha,
/// for a lossless image that uses it), from the bitstream's header.
fn vp8x(image: &[u8]) -> Option<Vec<u8>> {
    let data = image.get(8..)?;
    let (width, height, alpha) = match &image[..4] {
        b"VP8 " => {
            // A key frame: 3 bytes of frame tag, the start code, 14-bit width and height.
            let &[_, _, _, 0x9D, 0x01, 0x2A, w0, w1, h0, h1] = data.get(..10)? else {
                return None;
            };
            let w = u32::from(u16::from_le_bytes([w0, w1]) & 0x3FFF);
            let h = u32::from(u16::from_le_bytes([h0, h1]) & 0x3FFF);
            (w, h, false)
        }
        b"VP8L" => {
            // The signature, then width - 1 and height - 1 (14 bits each) and the alpha hint.
            if *data.first()? != 0x2F {
                return None;
            }
            let bits = u32::from_le_bytes(data.get(1..5)?.try_into().ok()?);
            let alpha = (bits >> 28) & 1 == 1;
            ((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1, alpha)
        }
        _ => return None,
    };
    if width == 0 || height == 0 {
        return None;
    }
    let mut out = b"VP8X\x0a\0\0\0".to_vec();
    out.extend_from_slice(&[if alpha { ALPHA_FLAG } else { 0 }, 0, 0, 0]);
    out.extend_from_slice(&(width - 1).to_le_bytes()[..3]);
    out.extend_from_slice(&(height - 1).to_le_bytes()[..3]);
    Some(out)
}

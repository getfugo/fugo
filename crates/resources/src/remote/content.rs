//! What fetched content is: its media type sniffed as Go's `http.DetectContentType` does, and its
//! file name from `Content-Disposition`.

use super::*;

/// The media type of `content` (sniffed), narrowed by extension hints: the hinted type when
/// both are text formats, the sniffed type when the hints agree or name nothing known, else
/// nothing (an image served as `.js`).
pub(super) fn from_content(
    types: &MediaTypes,
    hints: &[String],
    content: &[u8],
) -> Option<MediaType> {
    let sniffed = sniff(content);
    if sniffed == "application/octet-stream" {
        return None;
    }
    let by_type = |t: &str| types.by_type(t).map(|id| types.get(id).clone());
    let m = by_type(sniffed).or_else(|| {
        (sniffed == "text/xml")
            .then(|| by_type("application/xml"))
            .flatten()
    })?;
    if hints.is_empty() {
        return None;
    }
    let hinted = hints
        .iter()
        .find_map(|h| types.by_suffix(h).map(|id| types.get(id).clone()));
    match hinted {
        None => Some(m),
        Some(mm) if mm == m => Some(m),
        Some(mm) if m.is_text() && mm.is_text() => Some(mm),
        Some(_) => None,
    }
}

/// The essence of the content type the bytes look like: Go's `http.DetectContentType` (the
/// WHATWG sniffing rules), its signature table in order, on the first 512 bytes.
pub(super) fn sniff(b: &[u8]) -> &'static str {
    /// `pat` where the bytes masked with `mask` equal it (`None`: every bit counts).
    enum Sig {
        Html(&'static [u8]),
        Exact(&'static [u8], &'static str),
        Masked(&'static [u8], Option<&'static [u8]>, bool, &'static str),
        Mp4,
    }
    use Sig::{Exact, Html, Masked, Mp4};
    const SIGS: &[Sig] = &[
        Html(b"<!DOCTYPE HTML"),
        Html(b"<HTML"),
        Html(b"<HEAD"),
        Html(b"<SCRIPT"),
        Html(b"<IFRAME"),
        Html(b"<H1"),
        Html(b"<DIV"),
        Html(b"<FONT"),
        Html(b"<TABLE"),
        Html(b"<A"),
        Html(b"<STYLE"),
        Html(b"<TITLE"),
        Html(b"<B"),
        Html(b"<BODY"),
        Html(b"<BR"),
        Html(b"<P"),
        Html(b"<!--"),
        Masked(b"<?xml", None, true, "text/xml"),
        Exact(b"%PDF-", "application/pdf"),
        Exact(b"%!PS-Adobe-", "application/postscript"),
        Masked(b"\xfe\xff\x00\x00", Some(b"\xff\xff\x00\x00"), false, "text/plain"),
        Masked(b"\xff\xfe\x00\x00", Some(b"\xff\xff\x00\x00"), false, "text/plain"),
        Masked(b"\xef\xbb\xbf\x00", Some(b"\xff\xff\xff\x00"), false, "text/plain"),
        Exact(b"\x00\x00\x01\x00", "image/x-icon"),
        Exact(b"\x00\x00\x02\x00", "image/x-icon"),
        Exact(b"BM", "image/bmp"),
        Exact(b"GIF87a", "image/gif"),
        Exact(b"GIF89a", "image/gif"),
        Masked(
            b"RIFF\x00\x00\x00\x00WEBPVP",
            Some(b"\xff\xff\xff\xff\x00\x00\x00\x00\xff\xff\xff\xff\xff\xff"),
            false,
            "image/webp",
        ),
        Exact(b"\x89PNG\r\n\x1a\n", "image/png"),
        Exact(b"\xff\xd8\xff", "image/jpeg"),
        Masked(
            b"FORM\x00\x00\x00\x00AIFF",
            Some(b"\xff\xff\xff\xff\x00\x00\x00\x00\xff\xff\xff\xff"),
            false,
            "audio/aiff",
        ),
        Masked(b"ID3", None, false, "audio/mpeg"),
        Masked(b"OggS\x00", None, false, "application/ogg"),
        Masked(b"MThd\x00\x00\x00\x06", None, false, "audio/midi"),
        Masked(
            b"RIFF\x00\x00\x00\x00AVI ",
            Some(b"\xff\xff\xff\xff\x00\x00\x00\x00\xff\xff\xff\xff"),
            false,
            "video/avi",
        ),
        Masked(
            b"RIFF\x00\x00\x00\x00WAVE",
            Some(b"\xff\xff\xff\xff\x00\x00\x00\x00\xff\xff\xff\xff"),
            false,
            "audio/wave",
        ),
        Mp4,
        Exact(b"\x1a\x45\xdf\xa3", "video/webm"),
        // 34 bytes of anything, then "LP".
        Masked(
            b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00LP",
            Some(b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\xff\xff"),
            false,
            "application/vnd.ms-fontobject",
        ),
        Exact(b"\x00\x01\x00\x00", "font/ttf"),
        Exact(b"OTTO", "font/otf"),
        Exact(b"ttcf", "font/collection"),
        Exact(b"wOFF", "font/woff"),
        Exact(b"wOF2", "font/woff2"),
        Exact(b"\x1f\x8b\x08", "application/x-gzip"),
        Exact(b"PK\x03\x04", "application/zip"),
        Exact(b"Rar!\x1a\x07\x00", "application/x-rar-compressed"),
        Exact(b"Rar!\x1a\x07\x01\x00", "application/x-rar-compressed"),
        Exact(b"\x00asm", "application/wasm"),
    ];
    let b = &b[..b.len().min(512)];
    let ws = b
        .iter()
        .position(|c| !matches!(c, b'\t' | b'\n' | b'\x0c' | b'\r' | b' '))
        .unwrap_or(b.len());
    let masked = |data: &[u8], pat: &[u8], mask: Option<&[u8]>| {
        data.len() >= pat.len()
            && pat
                .iter()
                .enumerate()
                .all(|(i, &p)| data[i] & mask.map_or(0xff, |m| m[i]) == p)
    };
    for sig in SIGS {
        let hit = match *sig {
            Html(s) => {
                let d = &b[ws..];
                d.len() > s.len()
                    && d[..s.len()].eq_ignore_ascii_case(s)
                    && matches!(d[s.len()], b' ' | b'>')
            }
            Exact(s, _) => b.starts_with(s),
            Masked(pat, mask, skip_ws, _) => masked(if skip_ws { &b[ws..] } else { b }, pat, mask),
            Mp4 => is_mp4(b),
        };
        if hit {
            return match *sig {
                Html(_) => "text/html",
                Mp4 => "video/mp4",
                Exact(_, t) | Masked(_, _, _, t) => t,
            };
        }
    }
    if b[ws..]
        .iter()
        .any(|&c| matches!(c, 0x00..=0x08 | 0x0b | 0x0e..=0x1a | 0x1c..=0x1f))
    {
        return "application/octet-stream";
    }
    "text/plain"
}

/// Go's MP4 signature: an `ftyp` box whose brands include `mp4`.
pub(super) fn is_mp4(b: &[u8]) -> bool {
    if b.len() < 12 {
        return false;
    }
    let size = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize;
    if b.len() < size || !size.is_multiple_of(4) || &b[4..8] != b"ftyp" {
        return false;
    }
    (8..size)
        .step_by(4)
        .any(|st| st != 12 && b.get(st..st + 3) == Some(b"mp4".as_slice()))
}

/// The `filename` of a `Content-Disposition` header (`filename*` in RFC 2231/5987 form
/// preferred).
pub(super) fn disposition_filename(h: &str) -> Option<String> {
    let mut plain = None;
    let mut extended = None;
    for part in h.split(';').skip(1) {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let v = v.trim();
        match text::to_lower(k.trim()).as_str() {
            "filename" => plain = Some(v.trim_matches('"').to_owned()),
            "filename*" => {
                let encoded = v.splitn(3, '\'').nth(2).unwrap_or(v).trim_matches('"');
                extended = unescape(encoded, Component::PathSegment)
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok());
            }
            _ => {}
        }
    }
    extended.or(plain).filter(|f| !f.is_empty())
}

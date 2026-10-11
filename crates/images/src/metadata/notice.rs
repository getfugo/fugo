//! What is written into an image instead of its metadata: an EXIF block (orientation, author,
//! copyright), which photo applications show, and an XMP packet (Dublin Core title, creator
//! and rights), which Google Images shows as the creator and the copyright notice.

use std::fmt::Write as _;

/// The title, author and copyright notice written into an image; an empty field is not
/// written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Notice<'a> {
    pub title: &'a str,
    pub author: &'a str,
    pub copyright: &'a str,
}

impl Notice<'_> {
    /// Whether it has nothing to write.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.title.is_empty() && self.author.is_empty() && self.copyright.is_empty()
    }
}

/// The metadata blocks written into an image.
pub(super) struct Blocks {
    /// TIFF data.
    pub(super) exif: Option<Vec<u8>>,
    /// An XMP packet (UTF-8).
    pub(super) xmp: Option<Vec<u8>>,
}

impl Blocks {
    pub(super) fn new(orientation: Option<u8>, notice: &Notice) -> Self {
        Self {
            exif: exif_block(orientation, notice),
            xmp: xmp_packet(notice),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.exif.is_none() && self.xmp.is_none()
    }
}

const SHORT: u16 = 3;
const ASCII: u16 = 2;

/// An EXIF block (TIFF data, big-endian, one IFD) with the orientation (when not the normal
/// one), the author (`Artist`) and the copyright notice (`Copyright`, UTF-8 as exiftool writes
/// it); `None` when there is none of them.
fn exif_block(orientation: Option<u8>, n: &Notice) -> Option<Vec<u8>> {
    // Tag, type, count, value (in tag order).
    let mut entries: Vec<(u16, u16, u32, Vec<u8>)> = Vec::new();
    if let Some(o) = orientation {
        entries.push((0x0112, SHORT, 1, vec![0, o]));
    }
    for (tag, text) in [(0x013B, n.author), (0x8298, n.copyright)] {
        if !text.is_empty() {
            let value = [text.as_bytes(), &[0]].concat();
            entries.push((tag, ASCII, u32::try_from(value.len()).ok()?, value));
        }
    }
    if entries.is_empty() {
        return None;
    }
    let count = u16::try_from(entries.len()).ok()?;
    // Values longer than 4 bytes follow the IFD, at even offsets.
    let mut data_at = 8 + 2 + 12 * entries.len() + 4;
    let mut out = b"MM\0\x2a\0\0\0\x08".to_vec();
    let mut data = Vec::new();
    out.extend_from_slice(&count.to_be_bytes());
    for (tag, kind, n, value) in &entries {
        out.extend_from_slice(&tag.to_be_bytes());
        out.extend_from_slice(&kind.to_be_bytes());
        out.extend_from_slice(&n.to_be_bytes());
        if value.len() <= 4 {
            out.extend_from_slice(value);
            out.resize(out.len() + 4 - value.len(), 0);
        } else {
            out.extend_from_slice(&u32::try_from(data_at).ok()?.to_be_bytes());
            data.extend_from_slice(value);
            if value.len() % 2 == 1 {
                data.push(0);
            }
            data_at += value.len().next_multiple_of(2);
        }
    }
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&data);
    Some(out)
}

/// An XMP packet with `dc:title`, `dc:creator` and `dc:rights`; `None` for an empty notice.
fn xmp_packet(n: &Notice) -> Option<Vec<u8>> {
    if n.is_empty() {
        return None;
    }
    let mut dc = String::new();
    if !n.title.is_empty() {
        let _ = write!(
            dc,
            r#"<dc:title><rdf:Alt><rdf:li xml:lang="x-default">{}</rdf:li></rdf:Alt></dc:title>"#,
            escape(n.title)
        );
    }
    if !n.author.is_empty() {
        let _ = write!(
            dc,
            "<dc:creator><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></dc:creator>",
            escape(n.author)
        );
    }
    if !n.copyright.is_empty() {
        let _ = write!(
            dc,
            r#"<dc:rights><rdf:Alt><rdf:li xml:lang="x-default">{}</rdf:li></rdf:Alt></dc:rights>"#,
            escape(n.copyright)
        );
    }
    Some(
        format!(
            "<?xpacket begin=\"\u{FEFF}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\
             <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
             <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
             <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">{dc}\
             </rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"r\"?>"
        )
        .into_bytes(),
    )
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_alone_is_the_short_block() {
        assert_eq!(
            exif_block(Some(6), &Notice::default()).unwrap(),
            [
                b'M', b'M', 0, 42, 0, 0, 0, 8, 0, 1, 0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0, 0,
                0, 0, 0
            ]
        );
        assert_eq!(exif_block(None, &Notice::default()), None);
    }

    #[test]
    fn text_is_escaped() {
        let n = Notice {
            copyright: "© 2026 A & B <x>",
            ..Notice::default()
        };
        let xmp = String::from_utf8(xmp_packet(&n).unwrap()).unwrap();
        assert!(
            xmp.contains(">© 2026 A &amp; B &lt;x&gt;</rdf:li>"),
            "{xmp}"
        );
        assert!(!xmp.contains("dc:title"));
    }
}

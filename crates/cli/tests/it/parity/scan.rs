//! A small HTML/XML scanner (the tags and attributes of well-formed output), with entities and
//! percent-decoding.

pub(super) struct Tag {
    pub(super) name: String,
    pub(super) attrs: Vec<(String, String)>,
    /// Byte offset just after `>`.
    pub(super) end: usize,
}

impl Tag {
    pub(super) fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// The start tags of `doc` (names and attribute names lower-cased), skipping comments.
pub(super) fn tags(doc: &str) -> Vec<Tag> {
    let b = doc.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = doc[i..].find('<') {
        let start = i + off;
        if doc[start..].starts_with("<!--") {
            i = doc[start..]
                .find("-->")
                .map_or(doc.len(), |e| start + e + 3);
            continue;
        }
        let mut j = start + 1;
        if j >= b.len() || !b[j].is_ascii_alphabetic() {
            i = j;
            continue;
        }
        while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' && b[j] != b'/' {
            j += 1;
        }
        let name = doc[start + 1..j].to_ascii_lowercase();
        let mut attrs = Vec::new();
        loop {
            while j < b.len() && (b[j].is_ascii_whitespace() || b[j] == b'/') {
                j += 1;
            }
            if j >= b.len() || b[j] == b'>' {
                break;
            }
            let k0 = j;
            while j < b.len() && !b[j].is_ascii_whitespace() && !b"=>/".contains(&b[j]) {
                j += 1;
            }
            let key = doc[k0..j].to_ascii_lowercase();
            let mut value = String::new();
            if j < b.len() && b[j] == b'=' {
                j += 1;
                if j < b.len() && (b[j] == b'"' || b[j] == b'\'') {
                    let q = b[j];
                    let v0 = j + 1;
                    j = v0;
                    while j < b.len() && b[j] != q {
                        j += 1;
                    }
                    value = decode_entities(&doc[v0..j.min(b.len())]);
                    j += 1;
                } else {
                    let v0 = j;
                    while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' {
                        j += 1;
                    }
                    value = decode_entities(&doc[v0..j]);
                }
            }
            attrs.push((key, value));
        }
        out.push(Tag {
            name,
            attrs,
            end: (j + 1).min(doc.len()),
        });
        i = j.min(doc.len());
    }
    out
}

/// The text of element `name` after each of its start tags, up to its end tag.
pub(super) fn element_texts(doc: &str, name: &str) -> Vec<String> {
    let close = format!("</{name}>");
    tags(doc)
        .into_iter()
        .filter(|t| t.name == name)
        .filter_map(|t| {
            let rest = &doc[t.end..];
            rest.find(&close).map(|e| decode_entities(&rest[..e]))
        })
        .collect()
}

/// Decodes the entities of HTML output: the named ones Go and Tera write, `&#N;`, `&#xN;`.
pub(super) fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..end];
        let c = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            "copy" => Some('©'),
            "laquo" => Some('«'),
            "raquo" => Some('»'),
            "ldquo" => Some('“'),
            "rdquo" => Some('”'),
            "lsquo" => Some('‘'),
            "rsquo" => Some('’'),
            "ndash" => Some('–'),
            "mdash" => Some('—'),
            "hellip" => Some('…'),
            _ => ent
                .strip_prefix("#x")
                .or_else(|| ent.strip_prefix("#X"))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        if let Some(c) = c {
            out.push(c);
            rest = &rest[end + 1..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

/// Percent-decodes a URL (§7.2: Go percent-encodes non-ASCII, Tera does not).
pub(super) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Some(Ok(v)) = s.get(i + 1..i + 3).map(|h| u8::from_str_radix(h, 16))
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

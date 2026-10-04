//! Reading the XML as Chroma reads it (Go's `encoding/xml`): repeated attributes, references, white
//! space kept.

use super::*;

/// An element of the document.
#[derive(Debug, Default)]
pub(super) struct Element {
    pub(super) name: String,
    pub(super) attrs: Vec<(String, String)>,
    pub(super) children: Vec<Element>,
    pub(super) text: String,
    /// The comments since the previous sibling (or the parent's start).
    pub(super) comments: Vec<String>,
    /// A comment after the element, on the line it ends on.
    pub(super) trailing: Option<String>,
    /// The comments after the last child.
    pub(super) tail: Vec<String>,
}

impl Element {
    pub(super) fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub(super) fn attrs_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.attrs
            .iter()
            .filter(move |(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub(super) fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    /// Its comments and those of its descendants, in document order (the text Chroma
    /// ignores in a rule included).
    pub(super) fn all_comments(&self, out: &mut Vec<String>) {
        out.extend(self.comments.iter().cloned());
        if self.name == "rule" && !self.text.trim().is_empty() {
            out.push(self.text.trim().to_owned());
        }
        for c in &self.children {
            c.all_comments(out);
        }
        out.extend(self.tail.iter().cloned());
        out.extend(self.trailing.iter().cloned());
    }
}

/// Resolves `&name;` and `&#n;`.
pub(super) fn resolve_ref(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let n = name.strip_prefix('#')?;
            let v = match n.strip_prefix('x') {
                Some(h) => u32::from_str_radix(h, 16).ok()?,
                None => n.parse().ok()?,
            };
            char::from_u32(v)
        }
    }
}

/// Resolves the references in an attribute value; white space stays as written (Go's
/// `encoding/xml` does not normalise it: a pattern may hold newlines).
pub(super) fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        match after
            .find(';')
            .and_then(|end| resolve_ref(&after[..end]).map(|c| (c, end)))
        {
            Some((c, end)) => {
                out.push(c);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

pub(super) fn start(e: &BytesStart<'_>) -> Result<Element, String> {
    let mut attrs = Vec::new();
    for a in e.attributes().with_checks(false) {
        let a = a.map_err(|e| e.to_string())?;
        attrs.push((a.key.local_name().as_ref().to_owned(), unescape(&a.value)));
    }
    Ok(Element {
        name: e.local_name().as_ref().to_owned(),
        attrs,
        ..Element::default()
    })
}

/// The document: a pseudo-element whose children are the root element.
pub(super) fn parse(xml: &str) -> Result<Element, String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut stack = vec![Element::default()];
    // A child of the innermost open element ended and no newline followed yet: a comment now
    // is that child's trailing comment.
    let mut same_line = false;
    loop {
        let event = reader.read_event().map_err(|e| e.to_string())?;
        let top = stack.last_mut().ok_or("unbalanced end tag")?;
        match event {
            Event::Start(e) => {
                let mut el = start(&e)?;
                el.comments = std::mem::take(&mut top.tail);
                stack.push(el);
                same_line = false;
            }
            Event::Empty(e) => {
                let mut el = start(&e)?;
                el.comments = std::mem::take(&mut top.tail);
                top.children.push(el);
                same_line = true;
            }
            Event::End(_) => {
                let el = stack.pop().ok_or("unbalanced end tag")?;
                stack
                    .last_mut()
                    .ok_or("unbalanced end tag")?
                    .children
                    .push(el);
                same_line = true;
            }
            Event::Text(t) => {
                let text = t.xml10_content();
                if text.contains('\n') {
                    same_line = false;
                }
                top.text.push_str(&text);
            }
            Event::CData(t) => top.text.push_str(&t),
            Event::GeneralRef(r) => {
                let name = r.into_inner();
                match resolve_ref(&name) {
                    Some(c) => top.text.push(c),
                    None => {
                        top.text.push('&');
                        top.text.push_str(&name);
                        top.text.push(';');
                    }
                }
            }
            Event::Comment(c) => {
                let text = c.xml10_content().into_owned();
                match top.children.last_mut() {
                    Some(prev) if same_line && prev.trailing.is_none() && top.tail.is_empty() => {
                        prev.trailing = Some(text);
                    }
                    _ => top.tail.push(text),
                }
                same_line = false;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let [doc] = <[Element; 1]>::try_from(stack).map_err(|_| "unclosed element")?;
    Ok(doc)
}

/// Go's `strconv.ParseBool`.
pub(super) fn parse_bool(s: &str) -> bool {
    matches!(s.trim(), "1" | "t" | "T" | "true" | "TRUE" | "True")
}

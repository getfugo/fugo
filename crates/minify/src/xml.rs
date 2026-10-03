//! XML and stand-alone SVG, token by token (quick-xml).
//!
//! - Comments are removed (SVG `keepComments` keeps them).
//! - Tags are re-emitted compactly: one space before each attribute, no space before `>` or
//!   `/>`, and an element with no content collapses to `<a/>`.
//! - Declarations, processing instructions, DOCTYPE and CDATA are kept (trailing blanks
//!   trimmed).
//! - Text: with [`XmlWhitespace::Collapse`], whitespace-only text is dropped, text is trimmed at
//!   tags, and each other whitespace run becomes one `\n` (if it had a line break) or one space.
//!   Entity and character references are kept as written.

use quick_xml::Reader;
use quick_xml::events::attributes::AttrError;
use quick_xml::events::{BytesStart, Event};

use crate::MinifyError;
use crate::options::{XmlComments, XmlWhitespace};

#[derive(Clone, Copy)]
pub(crate) struct Style {
    pub comments: XmlComments,
    pub whitespace: XmlWhitespace,
}

pub(crate) fn minify(style: Style, input: &str) -> Result<String, MinifyError> {
    let mut reader = Reader::from_str(input);
    let config = reader.config_mut();
    config.trim_text(false);
    config.expand_empty_elements = false;
    let mut w = Writer {
        style,
        out: String::with_capacity(input.len()),
        text: String::new(),
        open: false,
    };
    loop {
        let event = reader.read_event().map_err(|e| MinifyError::Xml {
            offset: reader.error_position(),
            message: e.to_string(),
        })?;
        match event {
            Event::Text(t) => w.text.push_str(&t),
            Event::GeneralRef(r) => {
                w.text.push('&');
                w.text.push_str(&r);
                w.text.push(';');
            }
            Event::Comment(c) => {
                if style.comments == XmlComments::Keep {
                    w.markup();
                    w.out.push_str("<!--");
                    w.out.push_str(&c);
                    w.out.push_str("-->");
                }
            }
            Event::Start(s) => {
                w.markup();
                w.start_tag(&s).map_err(|e| attr_error(&reader, &e))?;
                w.open = true;
            }
            Event::Empty(s) => {
                w.markup();
                w.start_tag(&s).map_err(|e| attr_error(&reader, &e))?;
                w.out.push_str("/>");
            }
            Event::End(e) => {
                w.flush_text();
                if w.open {
                    w.open = false;
                    w.out.push_str("/>");
                } else {
                    w.out.push_str("</");
                    w.out.push_str(e.name().as_ref());
                    w.out.push('>');
                }
            }
            Event::CData(c) => {
                w.markup();
                w.out.push_str("<![CDATA[");
                w.out.push_str(&c);
                w.out.push_str("]]>");
            }
            Event::Decl(d) => w.instruction(&d),
            Event::PI(p) => w.instruction(&p),
            Event::DocType(d) => {
                w.markup();
                w.out.push_str("<!DOCTYPE ");
                w.out.push_str(d.trim_end());
                w.out.push('>');
            }
            Event::Eof => {
                // Unclosed elements are left unclosed, as in the input.
                w.markup();
                return Ok(w.out);
            }
        }
    }
}

fn attr_error(reader: &Reader<&[u8]>, e: &AttrError) -> MinifyError {
    MinifyError::Xml {
        offset: reader.buffer_position(),
        message: e.to_string(),
    }
}

struct Writer {
    style: Style,
    out: String,
    /// Text (with references) since the last markup.
    text: String,
    /// A start tag was written without its `>`, so that an empty element can become `<a/>`.
    open: bool,
}

impl Writer {
    /// Before markup other than an end tag: pending text, then the pending `>`.
    fn markup(&mut self) {
        self.flush_text();
        if self.open {
            self.open = false;
            self.out.push('>');
        }
    }

    fn flush_text(&mut self) {
        if self.text.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.text);
        let keep = self.style.whitespace == XmlWhitespace::Keep;
        if !keep && text.trim_matches(is_xml_space).is_empty() {
            return;
        }
        if self.open {
            self.open = false;
            self.out.push('>');
        }
        if keep {
            self.out.push_str(&text);
            return;
        }
        // `Some(has_line_break)` inside a whitespace run.
        let mut run: Option<bool> = None;
        for c in text.trim_matches(is_xml_space).chars() {
            if is_xml_space(c) {
                run = Some(run.unwrap_or(false) || c == '\n');
            } else {
                if let Some(line_break) = run.take() {
                    self.out.push(if line_break { '\n' } else { ' ' });
                }
                self.out.push(c);
            }
        }
    }

    fn start_tag(&mut self, s: &BytesStart<'_>) -> Result<(), AttrError> {
        self.out.push('<');
        self.out.push_str(s.name().as_ref());
        for attr in s.attributes() {
            let attr = attr?;
            self.out.push(' ');
            self.out.push_str(attr.key.as_ref());
            let quote = if attr.value.contains('"') { '\'' } else { '"' };
            self.out.push('=');
            self.out.push(quote);
            self.out.push_str(&attr.value);
            self.out.push(quote);
        }
        Ok(())
    }

    /// `<?…?>`: an XML declaration or processing instruction.
    fn instruction(&mut self, content: &str) {
        self.markup();
        self.out.push_str("<?");
        self.out.push_str(content.trim_end_matches(is_xml_space));
        self.out.push_str("?>");
    }
}

fn is_xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

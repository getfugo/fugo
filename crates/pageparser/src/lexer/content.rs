//! The page's intro (a byte order mark, front matter in YAML, TOML, JSON or Org) and its main
//! content up to each shortcode.

use super::*;

impl<'s> Lexer<'s> {
    pub(super) fn intro(&mut self) -> Step {
        while let Some(c) = self.next() {
            match c {
                '+' => return self.delimited_front_matter(FrontMatterFormat::Toml, '+'),
                '-' => return self.delimited_front_matter(FrontMatterFormat::Yaml, '-'),
                '{' => return self.json_front_matter(),
                '#' => {
                    self.org_front_matter();
                    return Ok(());
                }
                BYTE_ORDER_MARK => self.emit(TokenKind::ByteOrderMark),
                ' ' | '\t' | '\r' | '\n' => {}
                _ => break,
            }
        }
        Ok(())
    }

    pub(super) fn delimited_front_matter(&mut self, format: FrontMatterFormat, c: char) -> Step {
        for _ in 0..2 {
            if self.next() != Some(c) {
                return Err(self.error(LexErrorKind::InvalidDelimiter(format)));
            }
        }
        let delim: &[u8] = if c == '+' { b"+++" } else { b"---" };
        let mut at_line_start = self.consume_crlf();
        self.ignore();
        loop {
            let eol = at_line_start || {
                let Some(c) = self.next() else {
                    return Err(self.error(LexErrorKind::UnterminatedFrontMatter(format)));
                };
                matches!(c, '\r' | '\n')
            };
            if eol && self.at(delim) {
                self.emit(TokenKind::FrontMatter(format));
                self.pos += delim.len();
                self.consume_crlf();
                self.ignore();
                return Ok(());
            }
            at_line_start = false;
        }
    }

    /// A JSON object: braces are balanced outside strings; the newline after it is included.
    pub(super) fn json_front_matter(&mut self) -> Step {
        self.backup();
        let mut in_string = false;
        let mut depth = 0_usize;
        loop {
            match self.next() {
                None => return Err(self.error(LexErrorKind::UnterminatedJson)),
                Some('{') if !in_string => depth += 1,
                Some('}') if !in_string => depth = depth.saturating_sub(1),
                Some('"') => in_string = !in_string,
                Some('\\') => {
                    self.next();
                }
                Some(_) => {}
            }
            if depth == 0 {
                break;
            }
        }
        self.consume_crlf();
        self.emit(TokenKind::FrontMatter(FrontMatterFormat::Json));
        Ok(())
    }

    /// `#+KEY: value` lines; a lone `#` is content.
    pub(super) fn org_front_matter(&mut self) {
        self.backup();
        if !self.at(b"#+") {
            return;
        }
        if self.divider.is_some() {
            self.divider = Some(SUMMARY_DIVIDER_ORG);
        }
        while let Some(c) = self.next() {
            if c == '\n' && !self.at(b"#+") {
                break;
            }
        }
        self.emit(TokenKind::FrontMatter(FrontMatterFormat::Org));
    }

    pub(super) fn main(&mut self) -> Step {
        while self.pos < self.src.len() {
            let rest = self.rest();
            let tag = memchr::memmem::find(rest, b"{{");
            let divider = self.divider.and_then(|d| memchr::memmem::find(rest, d));
            let Some(skip) = [tag, divider].into_iter().flatten().min() else {
                self.pos = self.src.len();
                break;
            };
            self.pos += skip;
            if self.pos > self.start {
                self.emit_text();
            }
            if self.at(Delim::Html.left()) || self.at(Delim::Markdown.left()) {
                if let Err(e) = self.shortcode() {
                    if e.kind == LexErrorKind::InlineNesting {
                        // The Go lexer reads the rest of the source as text after this error.
                        self.pos = self.src.len();
                        if self.pos > self.start {
                            self.emit_text();
                        }
                    }
                    return Err(e);
                }
            } else if let Some(d) = self.divider.filter(|d| self.at(d)) {
                self.divider = None;
                self.pos += d.len();
                self.consume_space();
                self.emit(TokenKind::SummaryDivider);
            } else {
                // `{{` that is not a tag.
                self.pos += 1;
            }
        }
        if self.pos > self.start {
            self.emit_text();
        }
        Ok(())
    }
}

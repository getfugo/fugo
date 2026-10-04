//! Shortcodes: opening and closing tags, escaped shortcodes, parameters and their quoted, raw and
//! backtick values.

use super::*;

impl<'s> Lexer<'s> {
    /// A tag, from its left delimiter to its right delimiter.
    pub(super) fn shortcode(&mut self) -> Step {
        if self.inline && !self.closes_inline() {
            return Err(self.error(LexErrorKind::InlineNesting));
        }
        self.delim = if self.at(Delim::Markdown.left()) {
            Delim::Markdown
        } else {
            Delim::Html
        };
        self.pos += 3;
        if self.at(b"/*") {
            return self.escaped_shortcode();
        }
        self.emit(TokenKind::LeftDelim(self.delim));
        self.after_name = false;
        self.args = Args::Unknown;
        self.inside_tag()
    }

    /// Whether the tag at `pos` is `{{< / name …`, the closing tag of the current inline
    /// shortcode.
    pub(super) fn closes_inline(&self) -> bool {
        let after = &self.rest()[3..];
        let lead = after.len() - trim_start_space(after).len();
        let Some(tail) = after[lead..].strip_prefix(b"/") else {
            return false;
        };
        let tail = trim_end_space(trim_start_space(tail));
        let name = self.name.unwrap_or_default();
        tail.starts_with(name) && tail.get(name.len()) == Some(&b' ')
    }

    /// `{{</* x */>}}`: three text tokens (`{{<`, ` x `, `>}}`), the comment markers dropped.
    pub(super) fn escaped_shortcode(&mut self) -> Step {
        let mut close = b"*/".to_vec();
        close.extend_from_slice(self.delim.right());
        let Some(end) = memchr::memmem::find(self.rest(), &close).filter(|&i| i > 1) else {
            return Err(self.error(LexErrorKind::UnclosedEscape));
        };
        self.emit_text();
        self.pos += 2;
        self.ignore();
        self.pos += end - 2;
        self.emit_text();
        self.pos += 2;
        self.ignore();
        self.pos += 3;
        self.emit_text();
        Ok(())
    }

    pub(super) fn inside_tag(&mut self) -> Step {
        loop {
            if self.at(self.delim.right()) {
                return self.right_delim();
            }
            match self.next() {
                None => return Err(self.error(LexErrorKind::UnclosedTag)),
                Some(' ' | '\t' | '\r' | '\n') => self.ignore(),
                Some('=') => {
                    self.consume_space();
                    self.ignore();
                    match self.peek() {
                        Some('"') => self.quoted(true, ArgRole::Value)?,
                        Some('\\') => self.quoted(false, ArgRole::Value)?,
                        Some('`') => self.backtick(ArgRole::Value)?,
                        _ => {
                            self.consume_to_space();
                            self.emit(TokenKind::Value(Quoting::Bare));
                        }
                    }
                }
                Some('/') => {
                    if self.name.is_none() {
                        return Err(self.error(LexErrorKind::CloseWithoutOpen));
                    }
                    self.closing = true;
                    self.inline = false;
                    self.after_name = false;
                    self.emit(TokenKind::Close);
                }
                Some('\\') => {
                    self.ignore();
                    if matches!(self.peek(), Some('"' | '`')) {
                        self.param(true)?;
                    }
                }
                Some(c) if self.after_name && (is_word_or_hyphen(c) || c == '"' || c == '`') => {
                    self.backup();
                    self.param(false)?;
                }
                Some(c) if is_word(c) => {
                    self.backup();
                    if self.name_token()? {
                        return self.end_of_closing_tag();
                    }
                }
                Some(c) => return Err(self.error(LexErrorKind::UnexpectedChar(c))),
            }
        }
    }

    pub(super) fn right_delim(&mut self) -> Step {
        self.closing = false;
        self.pos += 3;
        self.emit(TokenKind::RightDelim(self.delim));
        Ok(())
    }

    /// A positional argument or the name of a named one; `escaped` after a backslash.
    pub(super) fn param(&mut self, escaped: bool) -> Step {
        let first = self.next();
        if first == Some('"') || (first == Some('`') && !escaped) {
            if self.args == Args::Named {
                return Err(self.error(LexErrorKind::MixedArguments));
            }
            self.args = Args::Positional;
            self.backup();
            return if first == Some('"') {
                self.quoted(!escaped, ArgRole::Param)
            } else {
                self.backtick(ArgRole::Param)
            };
        }
        if first == Some('`') {
            return Err(self.error(LexErrorKind::InvalidEscape));
        }
        let mut named = false;
        let mut c = first;
        loop {
            if !c.is_some_and(|c| is_word_or_hyphen(c) || c == '.') {
                self.backup();
                break;
            }
            c = self.next();
            if c == Some('=') {
                self.backup();
                named = true;
                break;
            }
        }
        self.args = match (self.args, named) {
            (Args::Unknown | Args::Positional, false) => Args::Positional,
            (Args::Unknown | Args::Named, true) => Args::Named,
            _ => return Err(self.error(LexErrorKind::MixedArguments)),
        };
        self.emit(TokenKind::Param(Quoting::Bare));
        Ok(())
    }

    /// A `"…"` argument. `escapes`: whether `\"` inside it is an escaped quote (otherwise it
    /// ends the string, as in `k=\"v\"`).
    pub(super) fn quoted(&mut self, escapes: bool, role: ArgRole) -> Step {
        let mut open = false;
        let mut has_escapes = false;
        let mut after_escape = false;
        loop {
            match self.next() {
                Some('\\') => match self.peek() {
                    Some('"') if open && !escapes => {
                        self.backup();
                        break;
                    }
                    Some('"') if open => {
                        has_escapes = true;
                        after_escape = true;
                    }
                    Some('`') => return Err(self.error(LexErrorKind::InvalidEscape)),
                    _ => {}
                },
                None | Some('\n') => return Err(self.error(LexErrorKind::UnterminatedString)),
                Some('"') if after_escape => after_escape = false,
                Some('"') if open => {
                    self.backup();
                    break;
                }
                Some('"') => {
                    open = true;
                    self.ignore();
                }
                Some(_) => {}
            }
        }
        if !has_escapes {
            self.emit(role.kind(Quoting::Quoted));
        } else if self.src[self.start..self.pos].iter().any(|&b| b != b'\\') {
            self.emit(role.kind(Quoting::Escaped));
        } else {
            self.ignore();
        }
        match self.next() {
            Some('\\') => {
                if self.peek() == Some('"') {
                    self.ignore();
                    self.next();
                    self.ignore();
                }
            }
            Some('"') => self.ignore(),
            _ => self.backup(),
        }
        Ok(())
    }

    /// A `` `…` `` argument.
    pub(super) fn backtick(&mut self, role: ArgRole) -> Step {
        let mut open = false;
        loop {
            match self.next() {
                Some('`') if open => {
                    self.backup();
                    break;
                }
                Some('`') => {
                    open = true;
                    self.ignore();
                }
                None => return Err(self.error(LexErrorKind::UnterminatedRawString)),
                Some(_) => {}
            }
        }
        self.emit(role.kind(Quoting::Backtick));
        self.next();
        self.ignore();
        Ok(())
    }

    /// A shortcode name. Returns whether it is the name of a closing tag.
    pub(super) fn name_token(&mut self) -> Result<bool, LexError> {
        loop {
            match self.next() {
                Some(c) if is_word_or_hyphen(c) || c == '/' => {}
                Some('.') => {
                    self.inline = self.at(b"inline ");
                    if !self.inline {
                        return Err(self.error(LexErrorKind::PeriodInName));
                    }
                }
                _ => {
                    self.backup();
                    break;
                }
            }
        }
        let word = &self.src[self.start..self.pos];
        let closing = self.closing;
        if closing && !self.opened.contains(word) {
            let name = String::from_utf8_lossy(word).into_owned();
            return Err(self.error(LexErrorKind::UnopenedClose(name)));
        }
        self.closing = false;
        self.name = Some(word);
        self.opened.insert(word);
        self.after_name = true;
        self.emit(if self.inline {
            TokenKind::InlineName
        } else {
            TokenKind::Name
        });
        Ok(closing)
    }

    pub(super) fn end_of_closing_tag(&mut self) -> Step {
        loop {
            self.inline = false;
            if self.at(self.delim.right()) {
                return self.right_delim();
            }
            match self.next() {
                Some(' ' | '\t') => self.ignore(),
                _ => return Err(self.error(LexErrorKind::JunkAfterClose)),
            }
        }
    }
}

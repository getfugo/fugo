//! Backslash escapes: references, categories, character escapes and numbers.

use super::*;

impl Parser<'_> {
    /// Backslash specials and basics (`scanBackslash`).
    pub(super) fn scan_backslash(&mut self, scan_only: bool) -> Result<Option<Node>, Error> {
        if self.chars_right() == 0 {
            return Err(self.err("illegal \\ at end of pattern"));
        }
        let ch = self.right_char(0);
        let o = self.options;
        let kind = match ch {
            'b' => Kind::Boundary,
            'B' => Kind::Nonboundary,
            'A' => Kind::Beginning,
            'G' => Kind::Start,
            'Z' => Kind::EndZ,
            'z' => Kind::End,
            'w' | 'W' => Kind::Set(CharSet::word(ch == 'W')),
            's' | 'S' => Kind::Set(CharSet::space(ch == 'S')),
            'd' | 'D' => Kind::Set(CharSet::digit(ch == 'D')),
            'p' | 'P' => {
                self.move_right(1);
                let prop = self.parse_property()?;
                let mut cc = CharSet::default();
                cc.add_category(prop, ch != 'p', self.use_i());
                if self.use_i() {
                    cc.add_lowercase();
                }
                return Ok(Some(set_node(cc, o)));
            }
            _ => return self.scan_basic_backslash(scan_only),
        };
        self.move_right(1);
        Ok(Some(match kind {
            Kind::Set(set) => set_node(set, o),
            k => Node::new(k, o),
        }))
    }

    /// Back-references and character escapes (`scanBasicBackslash`).
    pub(super) fn scan_basic_backslash(&mut self, scan_only: bool) -> Result<Option<Node>, Error> {
        if self.chars_right() == 0 {
            return Err(self.err("illegal \\ at end of pattern"));
        }
        let mut angled = false;
        let mut k = false;
        let mut close = '\0';
        let backpos = self.pos;
        let mut ch = self.right_char(0);

        if ch == 'k' {
            if self.chars_right() >= 2 {
                self.move_right(1);
                ch = self.move_right_get_char();
                if ch == '<' || ch == '\'' {
                    angled = true;
                    close = if ch == '\'' { '\'' } else { '>' };
                }
            }
            if !angled || self.chars_right() == 0 {
                return Err(self.err("malformed \\k<...> named back reference"));
            }
            ch = self.right_char(0);
            k = true;
        } else if (ch == '<' || ch == '\'') && self.chars_right() > 1 {
            angled = true;
            close = if ch == '\'' { '\'' } else { '>' };
            self.move_right(1);
            ch = self.right_char(0);
        }

        if angled && ch.is_ascii_digit() {
            let n = self.scan_decimal()?;
            if self.chars_right() > 0 && self.move_right_get_char() == close {
                if self.is_capture_slot(n) {
                    return Ok(Some(Node::new(Kind::Ref(n), self.options)));
                }
                return Err(self.err(format!("reference to undefined group number {n}")));
            }
        } else if !angled && ('1'..='9').contains(&ch) {
            let n = self.scan_decimal()?;
            if scan_only {
                return Ok(None);
            }
            if self.is_capture_slot(n) {
                return Ok(Some(Node::new(Kind::Ref(n), self.options)));
            }
            if n <= 9 {
                return Err(self.err(format!("reference to undefined group number {n}")));
            }
        } else if angled {
            let name = self.scan_capname();
            if !name.is_empty() && self.chars_right() > 0 && self.move_right_get_char() == close {
                if scan_only {
                    return Ok(None);
                }
                if self.is_capture_name(&name) {
                    let n = self.capture_slot_from_name(&name);
                    return Ok(Some(Node::new(Kind::Ref(n), self.options)));
                }
                return Err(self.err(format!("reference to undefined group name {name}")));
            } else if k {
                return Err(self.err("malformed \\k<...> named back reference"));
            }
        }

        // Not a back-reference: a character escape.
        self.pos = backpos;
        let mut ch = self.scan_char_escape()?;
        if scan_only {
            return Ok(None);
        }
        if self.use_i() {
            ch = to_lower(ch);
        }
        Ok(Some(Node::new(Kind::One(ch), self.options)))
    }

    /// `X` of `\p{X}` / `\pX` (`parseProperty`).
    pub(super) fn parse_property(&mut self) -> Result<&'static str, Error> {
        if self.chars_right() >= 1 && self.right_char(0) != '{' {
            let ch = self.move_right_get_char().to_string();
            return category_name(&ch).ok_or_else(|| {
                self.err(format!(
                    "unknown unicode category, script, or property '{ch}'"
                ))
            });
        }
        if self.chars_right() < 3 {
            return Err(self.err("incomplete \\p{X} character escape"));
        }
        if self.move_right_get_char() != '{' {
            return Err(self.err("malformed \\p{X} character escape"));
        }
        let start = self.pos;
        while self.chars_right() > 0 {
            let ch = self.move_right_get_char();
            if !(is_word_char(ch) || ch == '-') {
                self.move_left();
                break;
            }
        }
        let name: String = self.pattern[start..self.pos].iter().collect();
        if self.chars_right() == 0 || self.move_right_get_char() != '}' {
            return Err(self.err("incomplete \\p{X} character escape"));
        }
        category_name(&name).ok_or_else(|| {
            self.err(format!(
                "unknown unicode category, script, or property '{name}'"
            ))
        })
    }

    /// Decimal digits, pegged at `i32::MAX` (`scanDecimal`).
    pub(super) fn scan_decimal(&mut self) -> Result<usize, Error> {
        let mut i: usize = 0;
        while self.chars_right() > 0 {
            let Some(d) = self.right_char(0).to_digit(10) else {
                break;
            };
            self.move_right(1);
            let max = i32::MAX as usize;
            if i > max / 10 || (i == max / 10 && d as usize > max % 10) {
                return Err(self.err("capture group number out of range"));
            }
            i = i * 10 + d as usize;
        }
        Ok(i)
    }

    /// A `\` escape for one character (`scanCharEscape`).
    pub(super) fn scan_char_escape(&mut self) -> Result<char, Error> {
        let ch = self.move_right_get_char();
        if ('0'..='7').contains(&ch) {
            self.move_left();
            return Ok(self.scan_octal());
        }
        let c = match ch {
            'x' => {
                if self.chars_right() > 0 && self.right_char(0) == '{' {
                    self.move_right(1);
                    return self.scan_hex_until_brace();
                }
                self.scan_hex(2)?
            }
            'u' => self.scan_hex(4)?,
            'a' => '\u{7}',
            'b' => '\u{8}',
            'e' => '\u{1B}',
            'f' => '\u{C}',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'v' => '\u{B}',
            'c' => self.scan_control()?,
            _ => {
                if is_word_char(ch) {
                    return Err(self.err(format!("unrecognized escape sequence \\{ch}")));
                }
                ch
            }
        };
        Ok(c)
    }

    pub(super) fn scan_control(&mut self) -> Result<char, Error> {
        if self.chars_right() == 0 {
            return Err(self.err("missing control character"));
        }
        let mut ch = self.move_right_get_char() as u32;
        if (u32::from('a')..=u32::from('z')).contains(&ch) {
            ch -= u32::from('a') - u32::from('A');
        }
        match ch.checked_sub(u32::from('@')) {
            Some(c) if c < u32::from(' ') => Ok(char::from_u32(c).unwrap_or('\0')),
            _ => Err(self.err("unrecognized control character")),
        }
    }

    pub(super) fn scan_hex_until_brace(&mut self) -> Result<char, Error> {
        let mut i: u32 = 0;
        let mut has_content = false;
        while self.chars_right() > 0 {
            let ch = self.move_right_get_char();
            if ch == '}' {
                if !has_content {
                    return Err(self.err("insufficient hexadecimal digits"));
                }
                return char::from_u32(i)
                    .ok_or_else(|| self.err("hex values may not be larger than 0x10FFFF"));
            }
            has_content = true;
            let Some(d) = hex_digit(ch) else {
                return Err(self.err("missing closing }"));
            };
            i = i * 16 + d;
            if i > 0x10_FFFF {
                return Err(self.err("hex values may not be larger than 0x10FFFF"));
            }
        }
        Err(self.err("missing closing }"))
    }

    pub(super) fn scan_hex(&mut self, mut c: usize) -> Result<char, Error> {
        let mut i: u32 = 0;
        if self.chars_right() >= c {
            while c > 0 {
                let Some(d) = hex_digit(self.move_right_get_char()) else {
                    break;
                };
                i = i * 16 + d;
                c -= 1;
            }
        }
        if c > 0 {
            return Err(self.err("insufficient hexadecimal digits"));
        }
        Ok(char::from_u32(i).unwrap_or('\u{FFFD}'))
    }

    /// Up to three octal digits, truncated to 8 bits (`scanOctal`).
    pub(super) fn scan_octal(&mut self) -> char {
        let mut c = 3.min(self.chars_right());
        let mut i: u32 = 0;
        let mut d = self.right_char(0) as u32;
        while c > 0 && (u32::from('0')..=u32::from('7')).contains(&d) {
            i = i * 8 + (d - u32::from('0'));
            c -= 1;
            self.move_right(1);
            if self.chars_right() > 0 {
                d = self.right_char(0) as u32;
            } else {
                break;
            }
        }
        char::from_u32(i & 0xFF).unwrap_or('\0')
    }
}

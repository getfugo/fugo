//! Character classes (`scanCharSet`), `(?x)` blanks and comments, capture names and inline options.

use super::*;

impl Parser<'_> {
    /// Blanks and comments: `(?#…)` always, whitespace and `#…` lines with `x` (`scanBlank`).
    pub(super) fn scan_blank(&mut self) -> Result<(), Error> {
        if self.use_x() {
            loop {
                while self.chars_right() > 0 && is_space(self.right_char(0)) {
                    self.move_right(1);
                }
                if self.chars_right() == 0 {
                    break;
                }
                if self.right_char(0) == '#' {
                    while self.chars_right() > 0 && self.right_char(0) != '\n' {
                        self.move_right(1);
                    }
                } else if self.chars_right() >= 3
                    && self.right_char(2) == '#'
                    && self.right_char(1) == '?'
                    && self.right_char(0) == '('
                {
                    self.skip_comment()?;
                } else {
                    break;
                }
            }
        } else {
            while self.chars_right() >= 3
                && self.right_char(2) == '#'
                && self.right_char(1) == '?'
                && self.right_char(0) == '('
            {
                self.skip_comment()?;
            }
        }
        Ok(())
    }

    pub(super) fn skip_comment(&mut self) -> Result<(), Error> {
        while self.chars_right() > 0 && self.right_char(0) != ')' {
            self.move_right(1);
        }
        if self.chars_right() == 0 {
            return Err(self.err("unterminated comment"));
        }
        self.move_right(1);
        Ok(())
    }

    pub(super) fn scan_capname(&mut self) -> String {
        let start = self.pos;
        while self.chars_right() > 0 {
            if !is_word_char(self.move_right_get_char()) {
                self.move_left();
                break;
            }
        }
        self.pattern[start..self.pos].iter().collect()
    }

    /// The contents of `[…]` (`scanCharSet`); `None` when only scanning.
    #[expect(clippy::too_many_lines, reason = "a port of regexp2's scanCharSet")]
    pub(super) fn scan_char_set(
        &mut self,
        ignore_case: bool,
        scan_only: bool,
    ) -> Result<Option<CharSet>, Error> {
        let mut ch_prev = '\0';
        let mut in_range = false;
        let mut first_char = true;
        let mut closed = false;
        let mut cc = CharSet::default();

        if self.chars_right() > 0 && self.right_char(0) == '^' {
            self.move_right(1);
            cc.negate = true;
        }

        while self.chars_right() > 0 {
            let mut translated = false;
            let mut ch = self.move_right_get_char();
            let mut skip = false;
            if ch == ']' {
                if !first_char {
                    closed = true;
                    break;
                }
            } else if ch == '\\' && self.chars_right() > 0 {
                ch = self.move_right_get_char();
                match ch {
                    'D' | 'd' => {
                        if !scan_only {
                            if in_range {
                                return Err(self.err(format!(
                                    "cannot include class \\{ch} in character range"
                                )));
                            }
                            cc.add_digit(ch == 'D');
                        }
                        skip = true;
                    }
                    'S' | 's' => {
                        if !scan_only {
                            if in_range {
                                return Err(self.err(format!(
                                    "cannot include class \\{ch} in character range"
                                )));
                            }
                            cc.add_space(ch == 'S');
                        }
                        skip = true;
                    }
                    'W' | 'w' => {
                        if !scan_only {
                            if in_range {
                                return Err(self.err(format!(
                                    "cannot include class \\{ch} in character range"
                                )));
                            }
                            cc.add_word(ch == 'W');
                        }
                        skip = true;
                    }
                    'p' | 'P' => {
                        if !scan_only {
                            if in_range {
                                return Err(self.err(format!(
                                    "cannot include class \\{ch} in character range"
                                )));
                            }
                            let prop = self.parse_property()?;
                            cc.add_category(prop, ch != 'p', ignore_case);
                        } else {
                            let _ = self.parse_property();
                        }
                        skip = true;
                    }
                    '-' => {
                        if !scan_only {
                            cc.add_char(ch);
                        }
                        skip = true;
                    }
                    _ => {
                        self.move_left();
                        ch = self.scan_char_escape()?;
                        translated = true;
                    }
                }
            } else if ch == '[' {
                // POSIX-style `[:name:]`: skipped (regexp2 only acts on it in RE2 mode).
                if self.chars_right() > 0 && self.right_char(0) == ':' && !in_range {
                    let save = self.pos;
                    self.move_right(1);
                    if self.chars_right() > 1 && self.right_char(0) == '^' {
                        self.move_right(1);
                    }
                    self.scan_capname();
                    if self.chars_right() < 2
                        || self.move_right_get_char() != ':'
                        || self.move_right_get_char() != ']'
                    {
                        self.pos = save;
                    }
                }
            }
            if skip {
                first_char = false;
                continue;
            }

            if in_range {
                in_range = false;
                if !scan_only {
                    if ch == '[' && !translated && !first_char {
                        // A subtraction after a character: `[a-[b]]`.
                        cc.add_char(ch_prev);
                        let sub = self.scan_char_set(ignore_case, false)?.expect("set");
                        cc.add_subtraction(sub);
                        if self.chars_right() > 0 && self.right_char(0) != ']' {
                            return Err(self.err(
                                "a subtraction must be the last element in a character class",
                            ));
                        }
                    } else {
                        if ch_prev > ch {
                            return Err(
                                self.err(format!("[{ch_prev}-{ch}] range in reverse order"))
                            );
                        }
                        cc.add_range(ch_prev as u32, ch as u32);
                    }
                }
            } else if self.chars_right() >= 2
                && self.right_char(0) == '-'
                && self.right_char(1) != ']'
            {
                ch_prev = ch;
                in_range = true;
                self.move_right(1);
            } else if self.chars_right() >= 1
                && ch == '-'
                && !translated
                && self.right_char(0) == '['
                && !first_char
            {
                // A subtraction after a range: `[a-z-[b]]`.
                self.move_right(1);
                if scan_only {
                    self.scan_char_set(ignore_case, true)?;
                } else {
                    let sub = self.scan_char_set(ignore_case, false)?.expect("set");
                    cc.add_subtraction(sub);
                    if self.chars_right() > 0 && self.right_char(0) != ']' {
                        return Err(
                            self.err("a subtraction must be the last element in a character class")
                        );
                    }
                }
            } else if !scan_only {
                cc.add_range(ch as u32, ch as u32);
            }
            first_char = false;
        }

        if !closed {
            return Err(self.err("unterminated [] set"));
        }
        if scan_only {
            return Ok(None);
        }
        if ignore_case {
            cc.add_lowercase();
        }
        Ok(Some(cc))
    }

    /// `cimsx-cimsx` up to the first unrecognised character (`scanOptions`).
    pub(super) fn scan_options(&mut self) {
        let mut off = false;
        while self.chars_right() > 0 {
            let ch = self.right_char(0);
            if ch == '-' {
                off = true;
            } else if ch == '+' {
                off = false;
            } else {
                let o = option_from_code(ch);
                if o.is_empty() || o == Options::RIGHT_TO_LEFT || o == Options::ECMASCRIPT {
                    return;
                }
                if off {
                    self.options.remove(o);
                } else {
                    self.options.insert(o);
                }
            }
            self.move_right(1);
        }
    }
}

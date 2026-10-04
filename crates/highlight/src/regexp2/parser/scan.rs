//! The main scan (`scanRegex`) and quantifier recognition.

use super::*;

impl Parser<'_> {
    #[expect(
        clippy::too_many_lines,
        reason = "a port of regexp2's scanRegex, kept in one piece"
    )]
    pub(super) fn scan_regex(&mut self) -> Result<Node, Error> {
        let mut is_quant = false;
        self.start_group(Group {
            kind: GroupKind::Capture(0),
            opts: self.options,
            children: Vec::new(),
        });

        'outer: while self.chars_right() > 0 {
            let was_prev_quantifier = is_quant;
            is_quant = false;
            self.scan_blank()?;
            let startpos = self.pos;

            // Move past all of the normal characters.
            if self.use_x() {
                while self.chars_right() > 0 {
                    let ch = self.right_char(0);
                    if is_stopper_x(ch) && (ch != '{' || self.is_true_quantifier()) {
                        break;
                    }
                    self.move_right(1);
                }
            } else {
                while self.chars_right() > 0 {
                    let ch = self.right_char(0);
                    if is_special(ch) && (ch != '{' || self.is_true_quantifier()) {
                        break;
                    }
                    self.move_right(1);
                }
            }
            let endpos = self.pos;
            self.scan_blank()?;

            let ch = if self.chars_right() == 0 {
                '!' // at end
            } else {
                let c = self.right_char(0);
                if is_special(c) {
                    is_quant = is_quantifier(c);
                    self.move_right(1);
                    c
                } else {
                    ' ' // at an ordinary character
                }
            };

            let mut was_prev_quantifier = was_prev_quantifier;
            if startpos < endpos {
                let mut unquantified = endpos - startpos;
                if is_quant {
                    unquantified -= 1;
                }
                was_prev_quantifier = false;
                if unquantified > 0 {
                    self.add_to_concatenate(startpos, unquantified);
                }
                if is_quant {
                    self.add_unit_one(self.pattern[endpos - 1]);
                }
            }

            match ch {
                '!' => break 'outer,
                ' ' => continue 'outer,
                '[' => {
                    let set = self.scan_char_set(self.use_i(), false)?;
                    self.add_unit_set(set.expect("set"));
                }
                '(' => {
                    self.push_options();
                    match self.scan_group_open()? {
                        None => self.pop_keep_options(),
                        Some(group) => {
                            self.push_group();
                            self.start_group(group);
                        }
                    }
                    continue 'outer;
                }
                '|' => {
                    self.add_alternate();
                    continue 'outer;
                }
                ')' => {
                    if self.stack.is_empty() {
                        return Err(self.err("unexpected )"));
                    }
                    self.add_group()?;
                    self.pop_group()?;
                    self.pop_options();
                    if self.unit.is_none() {
                        continue 'outer;
                    }
                }
                '\\' => {
                    let node = self.scan_backslash(false)?;
                    self.unit = node;
                }
                '^' => {
                    if self.use_m() {
                        self.add_unit_type(Kind::Bol);
                    } else {
                        self.add_unit_type(Kind::Beginning);
                    }
                }
                '$' => {
                    if self.use_m() {
                        self.add_unit_type(Kind::Eol);
                    } else {
                        self.add_unit_type(Kind::EndZ);
                    }
                }
                '.' => {
                    if self.use_s() {
                        self.add_unit_set(CharSet::any());
                    } else {
                        self.add_unit_notone('\n');
                    }
                }
                '{' | '*' | '+' | '?' => {
                    if self.unit.is_none() {
                        return Err(self.err(if was_prev_quantifier {
                            "invalid nested repetition operator"
                        } else {
                            "missing argument to repetition operator"
                        }));
                    }
                    self.move_left();
                }
                _ => return Err(self.err("internal error")),
            }

            self.scan_blank()?;
            if self.chars_right() > 0 {
                is_quant = self.is_true_quantifier();
            }
            if self.chars_right() == 0 || !is_quant {
                self.add_concatenate();
                continue 'outer;
            }

            let mut ch = self.move_right_get_char();
            // Handle quantifiers.
            while self.unit.is_some() {
                let (min, max) = match ch {
                    '*' => (0, INFINITE),
                    '?' => (0, 1),
                    '+' => (1, INFINITE),
                    '{' => {
                        let startpos = self.pos;
                        let min = self.scan_decimal()?;
                        let mut max = min;
                        if startpos < self.pos
                            && self.chars_right() > 0
                            && self.right_char(0) == ','
                        {
                            self.move_right(1);
                            if self.chars_right() == 0 || self.right_char(0) == '}' {
                                max = INFINITE as usize;
                            } else {
                                max = self.scan_decimal()?;
                            }
                        }
                        if startpos == self.pos
                            || self.chars_right() == 0
                            || self.move_right_get_char() != '}'
                        {
                            self.add_concatenate();
                            self.pos = startpos - 1;
                            continue 'outer;
                        }
                        (clamp(min), clamp(max))
                    }
                    _ => return Err(self.err("internal error")),
                };
                self.scan_blank()?;
                let lazy = if self.chars_right() == 0 || self.right_char(0) != '?' {
                    false
                } else {
                    self.move_right(1);
                    true
                };
                if min > max {
                    return Err(self.err("invalid repeat count"));
                }
                self.add_concatenate_quantified(lazy, min, max);
                ch = '\0';
            }
        }

        if !self.stack.is_empty() {
            return Err(self.err("missing closing )"));
        }
        self.add_group()?;
        Ok(self.unit.take().expect("root"))
    }

    /// Whether a quantifier starts here (`{n}`, `{n,}`, `{n,m}` or `*+?`).
    pub(super) fn is_true_quantifier(&self) -> bool {
        let mut n = self.chars_right();
        if n == 0 {
            return false;
        }
        let start = self.pos;
        let mut ch = self.pattern[start];
        if ch != '{' {
            return is_quantifier(ch);
        }
        let mut pos = start;
        loop {
            n -= 1;
            if n == 0 {
                break;
            }
            pos += 1;
            ch = self.pattern[pos];
            if !ch.is_ascii_digit() {
                break;
            }
        }
        if n == 0 || pos - start == 1 {
            return false;
        }
        if ch == '}' {
            return true;
        }
        if ch != ',' {
            return false;
        }
        loop {
            n -= 1;
            if n == 0 {
                break;
            }
            pos += 1;
            ch = self.pattern[pos];
            if !ch.is_ascii_digit() {
                break;
            }
        }
        n > 0 && ch == '}'
    }
}

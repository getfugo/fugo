//! Group openings: `(?…)` constructs (`scanGroupOpen`).

use super::*;

impl Parser<'_> {
    /// The group after `(` (`scanGroupOpen`); `None` for an option setting or comment.
    #[expect(clippy::too_many_lines, reason = "a port of regexp2's scanGroupOpen")]
    pub(super) fn scan_group_open(&mut self) -> Result<Option<Group>, Error> {
        let start = self.pos;
        let mut close = '>';

        if self.chars_right() == 0
            || self.right_char(0) != '?'
            || (self.chars_right() > 1 && self.right_char(1) == ')')
        {
            if self.use_n() || self.ignore_next_paren {
                self.ignore_next_paren = false;
                return Ok(Some(self.group(GroupKind::Group)));
            }
            let n = self.consume_autocap();
            return Ok(Some(self.group(GroupKind::Capture(n))));
        }
        self.move_right(1);

        let unrecognized = |p: &Self| {
            let text: String = p.pattern[start..p.pos].iter().collect();
            p.err(format!("unrecognized grouping construct: ({text}"))
        };

        if self.chars_right() == 0 {
            return Err(unrecognized(self));
        }
        let kind = match self.move_right_get_char() {
            ':' => GroupKind::Group,
            '=' => {
                self.options.remove(Options::RIGHT_TO_LEFT);
                GroupKind::Require
            }
            '!' => {
                self.options.remove(Options::RIGHT_TO_LEFT);
                GroupKind::Prevent
            }
            '>' => GroupKind::Greedy,
            c @ ('\'' | '<') => {
                if c == '\'' {
                    close = '\'';
                }
                if self.chars_right() == 0 {
                    return Err(unrecognized(self));
                }
                match self.move_right_get_char() {
                    '=' if close != '\'' => {
                        self.options.insert(Options::RIGHT_TO_LEFT);
                        GroupKind::Require
                    }
                    '!' if close != '\'' => {
                        self.options.insert(Options::RIGHT_TO_LEFT);
                        GroupKind::Prevent
                    }
                    '=' | '!' => return Err(unrecognized(self)),
                    ch => {
                        self.move_left();
                        let mut capnum: Option<usize> = None;
                        let mut proceed = false;
                        if ch.is_ascii_digit() {
                            let n = self.scan_decimal()?;
                            capnum = self.is_capture_slot(n).then_some(n);
                            if self.chars_right() > 0
                                && !(self.right_char(0) == close || self.right_char(0) == '-')
                            {
                                return Err(self.err("invalid group name"));
                            }
                            if n == 0 {
                                return Err(self.err("capture number cannot be zero"));
                            }
                        } else if is_word_char(ch) {
                            let name = self.scan_capname();
                            if self.is_capture_name(&name) {
                                capnum = Some(self.capture_slot_from_name(&name));
                            }
                            if self.chars_right() > 0
                                && !(self.right_char(0) == close || self.right_char(0) == '-')
                            {
                                return Err(self.err("invalid group name"));
                            }
                        } else if ch == '-' {
                            proceed = true;
                        } else {
                            return Err(self.err("invalid group name"));
                        }
                        if (capnum.is_some() || proceed)
                            && self.chars_right() > 0
                            && self.right_char(0) == '-'
                        {
                            // Balancing groups (`(?<a-b>…)`): not used by Chroma's lexers.
                            return Err(self.err("balancing groups are not supported"));
                        }
                        if let Some(n) = capnum
                            && self.chars_right() > 0
                            && self.move_right_get_char() == close
                        {
                            return Ok(Some(self.group(GroupKind::Capture(n))));
                        }
                        return Err(unrecognized(self));
                    }
                }
            }
            '(' => {
                let paren_pos = self.pos;
                if self.chars_right() > 0 {
                    let ch = self.right_char(0);
                    if ch.is_ascii_digit() {
                        let n = self.scan_decimal()?;
                        if self.chars_right() > 0 && self.move_right_get_char() == ')' {
                            if self.is_capture_slot(n) {
                                return Ok(Some(self.group(GroupKind::Testref(n))));
                            }
                            return Err(self.err(format!("(?({n}) ) reference to undefined group")));
                        }
                        return Err(self.err(format!("(?({n}) ) malformed")));
                    } else if is_word_char(ch) {
                        let name = self.scan_capname();
                        if self.is_capture_name(&name)
                            && self.chars_right() > 0
                            && self.move_right_get_char() == ')'
                        {
                            let n = self.capture_slot_from_name(&name);
                            return Ok(Some(self.group(GroupKind::Testref(n))));
                        }
                    }
                }
                self.pos = paren_pos - 1;
                self.ignore_next_paren = true;
                let n = self.chars_right();
                if n >= 3 && self.right_char(1) == '?' {
                    let c2 = self.right_char(2);
                    if c2 == '#' {
                        return Err(self.err("alternation conditions cannot be comments"));
                    }
                    if c2 == '\''
                        || (n >= 4
                            && c2 == '<'
                            && self.right_char(3) != '!'
                            && self.right_char(3) != '=')
                    {
                        return Err(
                            self.err("alternation conditions do not capture and cannot be named")
                        );
                    }
                }
                GroupKind::Testgroup
            }
            _ => {
                self.move_left();
                if !matches!(
                    self.group.as_ref().map(|g| &g.kind),
                    Some(GroupKind::Testgroup)
                ) {
                    self.scan_options();
                }
                if self.chars_right() == 0 {
                    return Err(unrecognized(self));
                }
                match self.move_right_get_char() {
                    ')' => return Ok(None),
                    ':' => GroupKind::Group,
                    _ => return Err(unrecognized(self)),
                }
            }
        };
        Ok(Some(self.group(kind)))
    }

    pub(super) fn group(&self, kind: GroupKind) -> Group {
        Group {
            kind,
            opts: self.options,
            children: Vec::new(),
        }
    }
}

//! Capture numbering: slots, names and the pre-scan that counts them (`countCaptures`).

use super::*;

impl Parser<'_> {
    pub(super) fn note_capture_slot(&mut self, i: usize, pos: usize) {
        if let std::collections::btree_map::Entry::Vacant(e) = self.caps.entry(i) {
            e.insert(pos);
            self.capcount += 1;
            if self.captop <= i {
                self.captop = i + 1;
            }
        }
    }

    pub(super) fn note_capture_name(&mut self, name: String, pos: usize) {
        if !self.capnames.contains_key(&name) {
            self.capnames.insert(name.clone(), pos);
            self.capnamelist.push(name);
        }
    }

    pub(super) fn is_capture_slot(&self, i: usize) -> bool {
        self.caps.contains_key(&i)
    }

    pub(super) fn is_capture_name(&self, name: &str) -> bool {
        self.capnames.contains_key(name)
    }

    pub(super) fn capture_slot_from_name(&self, name: &str) -> usize {
        self.capnames.get(name).copied().unwrap_or(0)
    }

    pub(super) fn consume_autocap(&mut self) -> usize {
        let r = self.autocap;
        self.autocap += 1;
        r
    }

    /// Named groups take the slots after the numbered ones (`assignNameSlots`).
    pub(super) fn assign_name_slots(&mut self) {
        let names = self.capnamelist.clone();
        for name in names {
            while self.is_capture_slot(self.autocap) {
                self.autocap += 1;
            }
            let pos = self.capnames[&name];
            self.capnames.insert(name, self.autocap);
            self.note_capture_slot(self.autocap, pos);
            self.autocap += 1;
        }
    }

    /// The pre-scan that numbers the capture groups (`countCaptures`).
    pub(super) fn count_captures(&mut self) -> Result<(), Error> {
        self.note_capture_slot(0, 0);
        self.autocap = 1;
        while self.chars_right() > 0 {
            let pos = self.pos;
            let ch = self.move_right_get_char();
            match ch {
                '\\' => {
                    if self.chars_right() > 0 {
                        self.scan_backslash(true)?;
                    }
                }
                '#' => {
                    if self.use_x() {
                        self.move_left();
                        self.scan_blank()?;
                    }
                }
                '[' => {
                    self.scan_char_set(false, true)?;
                }
                ')' => {
                    if !self.options_stack.is_empty() {
                        self.pop_options();
                    }
                }
                '(' => {
                    if self.chars_right() >= 2
                        && self.right_char(1) == '#'
                        && self.right_char(0) == '?'
                    {
                        self.move_left();
                        self.scan_blank()?;
                    } else {
                        self.push_options();
                        if self.chars_right() > 0 && self.right_char(0) == '?' {
                            self.move_right(1);
                            if self.chars_right() > 1
                                && (self.right_char(0) == '<' || self.right_char(0) == '\'')
                            {
                                self.move_right(1);
                                let ch = self.right_char(0);
                                if ch != '0' && is_word_char(ch) {
                                    if ch.is_ascii_digit() {
                                        let dec = self.scan_decimal()?;
                                        self.note_capture_slot(dec, pos);
                                    } else {
                                        let name = self.scan_capname();
                                        self.note_capture_name(name, pos);
                                    }
                                }
                            } else {
                                self.scan_options();
                                if self.chars_right() > 0 {
                                    if self.right_char(0) == ')' {
                                        self.move_right(1);
                                        self.pop_keep_options();
                                    } else if self.right_char(0) == '(' {
                                        self.ignore_next_paren = true;
                                        continue;
                                    }
                                }
                            }
                        } else if !self.use_n() && !self.ignore_next_paren {
                            let n = self.consume_autocap();
                            self.note_capture_slot(n, pos);
                        }
                    }
                    self.ignore_next_paren = false;
                }
                _ => {}
            }
        }
        self.assign_name_slots();
        Ok(())
    }

    pub(super) fn reset(&mut self, options: Options) {
        self.pos = 0;
        self.autocap = 1;
        self.ignore_next_paren = false;
        self.options_stack.clear();
        self.options = options;
        self.stack.clear();
    }
}

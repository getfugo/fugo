//! What a cell is part of: text, a vertical or diagonal line, a rounded corner or a half step.

use super::*;

impl Canvas {
    /// Whether the character at `i` is text (Go's `isText`): any character GoAT does not
    /// reserve, and reserved characters that have no line above or below and either follow
    /// text, precede an unreserved character, or stand between spaces next to text.
    pub(super) fn is_text(&self, i: Index) -> bool {
        if self.text_at(i).is_some() {
            return true;
        }
        let r = self.rune_at(i);
        if r == ' ' {
            return false;
        }
        if !self.is_reserved(i) {
            return true;
        }
        // A reserved character with an incoming line (e.g. "|") above it.
        if self.has_line_above_or_below(i) {
            return false;
        }
        // Reserved but part of a word; looking at the text on the left keeps chains of
        // reserved-but-text characters like "foo----bar".
        if self.text_at(i.west()).is_some() || !self.is_reserved(i.east()) {
            return true;
        }
        let (w, e) = (i.west(), i.east());
        if !(self.rune_at(w) == ' ' && self.rune_at(e) == ' ') {
            return false;
        }
        // Circles surrounded by whitespace are not text.
        if is_dot(r) {
            return false;
        }
        // Surrounded by whitespace, with text on either side.
        !self.is_reserved(w.west()) || !self.is_reserved(e.east())
    }

    /// Whether the character at `i` may belong to the drawing (Go's `isReserved`).
    pub(super) fn is_reserved(&self, i: Index) -> bool {
        RESERVED.contains(&self.rune_at(i))
    }

    /// Whether the character at `i` belongs to anything but a horizontal line (Go's
    /// `hasLineAboveOrBelow`).
    pub(super) fn has_line_above_or_below(&self, i: Index) -> bool {
        match self.rune_at(i) {
            '*' | 'o' | '+' | 'v' | '^' => {
                self.part_of_diagonal_line(i) || self.part_of_vertical_line(i)
            }
            '|' => self.part_of_vertical_line(i) || self.part_of_rounded_corner(i),
            '/' | '\\' => self.part_of_diagonal_line(i),
            '-' => self.part_of_rounded_corner(i),
            '(' | ')' => self.part_of_vertical_line(i),
            _ => false,
        }
    }

    /// Whether a `|` segment passes through `i` (Go's `partOfVerticalLine`).
    pub(super) fn part_of_vertical_line(&self, i: Index) -> bool {
        let this = self.rune_at(i);
        let north = self.rune_at(i.north());
        let south = self.rune_at(i.south());
        north == '|'
            || (this == '|' && is_joint(north))
            || south == '|'
            || (this == '|' && is_joint(south))
    }

    /// Whether a diagonal segment passes through `i` (Go's `partOfDiagonalLine`).
    pub(super) fn part_of_diagonal_line(&self, i: Index) -> bool {
        let r = self.rune_at(i);
        let n = self.rune_at(i.north());
        let s = self.rune_at(i.south());
        let nw = self.rune_at(i.n_west());
        let se = self.rune_at(i.s_east());
        let ne = self.rune_at(i.n_east());
        let sw = self.rune_at(i.s_west());
        match r {
            // Diagonal segments can be connected to joints or other segments.
            '/' => ne == r || sw == r || is_joint(ne) || is_joint(sw) || n == '\\' || s == '\\',
            '\\' => nw == r || se == r || is_joint(nw) || is_joint(se) || n == '/' || s == '/',
            // Anything else: segments next to it.
            _ => nw == '\\' || ne == '/' || sw == '/' || se == '\\',
        }
    }

    /// For `-` and `|`: whether it could be part of a rounded corner (Go's
    /// `partOfRoundedCorner`).
    pub(super) fn part_of_rounded_corner(&self, i: Index) -> bool {
        match self.rune_at(i) {
            '-' => {
                let (w, e) = (self.rune_at(i.west()), self.rune_at(i.east()));
                w == '.' || e == '.' || w == '\'' || e == '\''
            }
            '|' => {
                self.rune_at(i.n_west()) == '.'
                    || self.rune_at(i.n_east()) == '.'
                    || self.rune_at(i.s_west()) == '\''
                    || self.rune_at(i.s_east()) == '\''
            }
            _ => false,
        }
    }

    /// The half of the cell a short vertical stroke at `i` keeps, for `'`, `.` and `|` next
    /// to baselines and midlines (Go's `partOfHalfStep`).
    pub(super) fn part_of_half_step(&self, i: Index) -> Option<Orientation> {
        let r = self.rune_at(i);
        if !matches!(r, '\'' | '.' | '|') || self.rounded_corner(i).is_some() {
            return None;
        }
        let w = self.rune_at(i.west());
        let e = self.rune_at(i.east());
        let n = self.rune_at(i.north());
        let s = self.rune_at(i.south());
        let nw = self.rune_at(i.n_west());
        let ne = self.rune_at(i.n_east());
        match r {
            //  _      _
            //   '-  -'
            '\'' if (nw == '_' && e == '-') || (w == '-' && ne == '_') => Some(North),
            // _.-  -._
            '.' if (w == '-' && e == '_') || (w == '_' && e == '-') => Some(South),
            '|' => {
                //  _   _
                //   | |
                if (n != '|' && (ne == '_' || nw == '_')) || n == '-' {
                    Some(North)
                // _| |_
                } else if (s != '|' && (w == '_' || e == '_')) || s == '-' {
                    Some(South)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

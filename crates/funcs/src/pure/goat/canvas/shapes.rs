//! The other shapes: triangles, circles, rounded corners, text and bridges.

use super::*;

impl Canvas {
    /// The arrow heads, with the tails that connect some of them to a line (Go's
    /// `Triangles`), column by column.
    pub(in super::super) fn triangles(&self) -> Vec<Shape> {
        let mut shapes = Vec::new();
        for start in up_down(self.width, self.height) {
            let r = self.rune_at(start);
            // The direction, turned to lie along an adjacent diagonal.
            let o = match r {
                //  ^  and ^
                // /        \
                '^' if self.rune_at(start.s_west()) == '/' => NorthEast,
                '^' if self.rune_at(start.s_east()) == '\\' => NorthWest,
                '^' => North,
                //  /  and \
                // v        v
                'v' if self.rune_at(start.n_east()) == '/' => SouthWest,
                'v' if self.rune_at(start.n_west()) == '\\' => SouthEast,
                'v' => South,
                '<' => West,
                '>' => East,
                _ => continue,
            };
            // Snap the head to the line it points away from and draw a tail where needed.
            let tail_from = |r: char| r == '-' || (is_joint(r) && !is_dot(r));
            let mut nudge = false;
            match o {
                North if tail_from(self.rune_at(start.north())) => {
                    nudge = true;
                    shapes.push(Shape::Line(Line::half_step(start, North)));
                }
                NorthWest if tail_from(self.rune_at(start.n_west())) => {
                    nudge = true;
                    shapes.push(Shape::Line(Line::new(start.n_west(), start, SouthEast)));
                }
                NorthEast if tail_from(self.rune_at(start.n_east())) => {
                    nudge = true;
                    shapes.push(Shape::Line(Line::new(start, start.n_east(), NorthEast)));
                }
                South if tail_from(self.rune_at(start.south())) => {
                    nudge = true;
                    shapes.push(Shape::Line(Line::half_step(start, South)));
                }
                SouthEast if tail_from(self.rune_at(start.s_east())) => {
                    nudge = true;
                    shapes.push(Shape::Line(Line::new(start, start.s_east(), SouthEast)));
                }
                SouthWest if tail_from(self.rune_at(start.s_west())) => {
                    nudge = true;
                    shapes.push(Shape::Line(Line::new(start.s_west(), start, NorthEast)));
                }
                West => nudge = is_dot(self.rune_at(start.west())),
                East => nudge = is_dot(self.rune_at(start.east())),
                _ => {}
            }
            shapes.push(Shape::Triangle(Triangle {
                start,
                orientation: o,
                nudge,
            }));
        }
        shapes
    }

    /// Every `o` and `*` of the drawing (Go's `Circles`), column by column.
    pub(in super::super) fn circles(&self) -> Vec<Circle> {
        up_down(self.width, self.height)
            .filter_map(|start| match self.rune_at(start) {
                'o' => Some(Circle { start, bold: false }),
                '*' => Some(Circle { start, bold: true }),
                _ => None,
            })
            .collect()
    }

    /// Every rounded corner (Go's `RoundedCorners`), row by row.
    pub(in super::super) fn rounded_corners(&self) -> Vec<RoundedCorner> {
        left_right(self.width, self.height)
            .filter_map(|start| {
                self.rounded_corner(start)
                    .map(|orientation| RoundedCorner { start, orientation })
            })
            .collect()
    }

    /// The orientation of the rounded corner a joint at `i` makes (Go's `isRoundedCorner`).
    pub(super) fn rounded_corner(&self, i: Index) -> Option<Orientation> {
        let r = self.rune_at(i);
        if !is_joint(r) {
            return None;
        }
        let opens_up = r == '\'' || r == '+';
        let opens_down = r == '.' || r == '+';
        let dash = |side: Index, above: Index| {
            matches!(self.rune_at(side), '-' | '+' | '_') || self.rune_at(above) == '_'
        };
        let dash_right = dash(i.east(), i.n_east());
        let dash_left = dash(i.west(), i.n_west());
        let vertical_segment = |i: Index| {
            let r = self.rune_at(i);
            matches!(r, '|' | '+' | ')' | '(') || is_dot(r)
        };

        //  .- or  .-
        // |      +
        if opens_down && dash_right && vertical_segment(i.s_west()) {
            return Some(NorthWest);
        }
        // -. or -.  or -.  or _.  or -.
        //   |     +      )      )      o
        if opens_down && dash_left && vertical_segment(i.s_east()) {
            return Some(NorthEast);
        }
        //   | or   + or   | or   + or   + or_ )
        // -'     -'     +'     +'     ++     '
        if opens_up && dash_left && vertical_segment(i.n_east()) {
            return Some(SouthEast);
        }
        // |  or +
        //  '-    '-
        if opens_up && dash_right && vertical_segment(i.n_west()) {
            return Some(SouthWest);
        }
        None
    }

    /// The text characters (Go's `Text`), column by column (Go sorts them by column, then row).
    /// Markdeep's diagonal box-drawing characters become lonely lines.
    pub(in super::super) fn text(&self) -> Vec<Shape> {
        up_down(self.width, self.height)
            .filter_map(|i| {
                let ch = self.text_at(i)?;
                let diagonal = |stop: Index, orientation| {
                    let mut line = Line::new(i, stop, orientation);
                    line.lonely = true;
                    Shape::Line(line)
                };
                Some(match ch {
                    '╱' | '╳' => diagonal(i.n_east(), NorthEast),
                    '╲' => diagonal(i.s_east(), SouthEast),
                    _ => Shape::Text(Text { start: i, ch }),
                })
            })
            .collect()
    }

    /// Every bridge, with the half steps of its vertical line (Go's `Bridges`), row by row.
    pub(in super::super) fn bridges(&self) -> Vec<Shape> {
        left_right(self.width, self.height)
            .filter_map(|start| {
                self.bridge(start).map(|orientation| {
                    [
                        Shape::Line(Line::half_step(start.north(), South)),
                        Shape::Line(Line::half_step(start.south(), North)),
                        Shape::Bridge(Bridge { start, orientation }),
                    ]
                })
            })
            .flatten()
            .collect()
    }

    /// `-)-` (east) or `-(-` (west) at `i` (Go's `isBridge`).
    pub(super) fn bridge(&self, i: Index) -> Option<Orientation> {
        if self.rune_at(i.west()) != '-' || self.rune_at(i.east()) != '-' {
            return None;
        }
        match self.rune_at(i) {
            '(' => Some(West),
            ')' => Some(East),
            _ => None,
        }
    }
}

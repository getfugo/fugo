//! The lines of the grid: segments in each orientation, half steps, and where they start and stop.

use super::*;

impl Canvas {
    /// Every line (Go's `Lines`): `-` midlines, `_` baselines, `|` verticals, `/` and `\`
    /// diagonals, then the half steps, each nudged to meet its neighbours.
    pub(in super::super) fn lines(&self) -> Vec<Line> {
        let horizontal_midlines = self.lines_for_segment('-');

        let mut diag_up_lines = self.lines_for_segment('/');
        for l in &mut diag_up_lines {
            // /_
            if self.rune_at(l.start.east()) == '_' {
                l.start_nudge.tiny = true;
            }
            //  _
            //  /
            if self.rune_at(l.stop.north()) == '_' {
                l.stop_nudge.tiny = true;
            }
            //   _
            //  /
            if !l.lonely && self.rune_at(l.stop.n_east()) == '_' {
                l.stop_nudge.tiny = true;
            }
            // _/
            if !l.lonely && self.rune_at(l.start.west()) == '_' {
                l.start_nudge.tiny = true;
            }
            // \
            // /
            if !l.lonely && self.rune_at(l.stop.north()) == '\\' {
                l.stop_nudge.tiny = true;
            }
            // /
            // \
            if !l.lonely && self.rune_at(l.start.south()) == '\\' {
                l.start_nudge.tiny = true;
            }
        }

        let mut diag_down_lines = self.lines_for_segment('\\');
        for l in &mut diag_down_lines {
            // _\
            if self.rune_at(l.stop.west()) == '_' {
                l.stop_nudge.tiny = true;
            }
            // _
            // \
            if self.rune_at(l.start.north()) == '_' {
                l.start_nudge.tiny = true;
            }
            //  _
            //   \
            if !l.lonely && self.rune_at(l.start.n_west()) == '_' {
                l.start_nudge.tiny = true;
            }
            // \_
            if !l.lonely && self.rune_at(l.stop.east()) == '_' {
                l.stop_nudge.tiny = true;
            }
            // \
            // /
            if !l.lonely && self.rune_at(l.stop.south()) == '/' {
                l.stop_nudge.tiny = true;
            }
            // /
            // \
            if !l.lonely && self.rune_at(l.start.north()) == '/' {
                l.start_nudge.tiny = true;
            }
        }

        let mut horizontal_baselines = self.lines_for_segment('_');
        for l in &mut horizontal_baselines {
            l.nudge_down = true;
            //     _
            // _| |
            if self.rune_at(l.stop.s_east()) == '|' || self.rune_at(l.stop.n_east()) == '|' {
                l.stop_nudge.full = true;
            }
            // _
            //  |  _|
            if self.rune_at(l.start.s_west()) == '|' || self.rune_at(l.start.n_west()) == '|' {
                l.start_nudge.full = true;
            }
            //     _
            // _/   \
            if self.rune_at(l.stop.east()) == '/' || self.rune_at(l.stop.s_east()) == '\\' {
                l.stop_nudge.tiny = true;
            }
            //       _
            // \_   /
            if self.rune_at(l.start.west()) == '\\' || self.rune_at(l.start.s_west()) == '/' {
                l.start_nudge.tiny = true;
            }
            // _\
            if self.rune_at(l.stop.east()) == '\\' {
                l.stop_nudge.full = true;
                l.stop_nudge.tiny = true;
            }
            // /_
            if self.rune_at(l.start.west()) == '/' {
                l.start_nudge.full = true;
                l.start_nudge.tiny = true;
            }
            //  _
            //  /
            if self.rune_at(l.stop.south()) == '/' {
                l.stop_nudge.tiny = true;
            }
            //  _
            //  \
            if self.rune_at(l.start.south()) == '\\' {
                l.start_nudge.tiny = true;
            }
            //  _
            // '
            if self.rune_at(l.start.s_west()) == '\'' {
                l.start_nudge.full = true;
            }
            // _
            //  '
            if self.rune_at(l.stop.s_east()) == '\'' {
                l.stop_nudge.full = true;
            }
        }

        let vertical_lines = self.lines_for_segment('|');

        let mut lines = horizontal_midlines;
        lines.extend(horizontal_baselines);
        lines.extend(vertical_lines);
        lines.extend(diag_up_lines);
        lines.extend(diag_down_lines);
        lines.extend(self.half_steps());
        lines
    }

    /// The half steps of vertical lines meeting `'`, `.` and `|` corners (Go's `HalfSteps`).
    pub(super) fn half_steps(&self) -> Vec<Line> {
        up_down(self.width, self.height)
            .filter_map(|idx| {
                self.part_of_half_step(idx)
                    .map(|chop| Line::half_step(idx, chop))
            })
            .collect()
    }

    /// The lines drawn by `segment` (Go's `getLinesForSegment`): the traversal, orientation
    /// and pass-through characters of each segment kind. The traversal covers one column and
    /// one row more than the grid, so every line ends on the grid.
    pub(super) fn lines_for_segment(&self, segment: char) -> Vec<Line> {
        let (w, h) = (self.width + 1, self.height + 1);
        match segment {
            '-' => self.collect_lines(
                left_right(w, h),
                segment,
                &[&JOINTS, &['<', '>', '(', ')']],
                East,
            ),
            '_' => self.collect_lines(left_right(w, h), segment, &[&JOINTS, &['|']], East),
            '|' => self.collect_lines(up_down(w, h), segment, &[&JOINTS, &['^', 'v']], South),
            '/' => self.collect_lines(
                diag_up(w, h),
                segment,
                &[&JOINTS, &['o', '*', '<', '>', '^', 'v', '|']],
                NorthEast,
            ),
            '\\' => self.collect_lines(
                diag_down(w, h),
                segment,
                &[&JOINTS, &['o', '*', '<', '>', '^', 'v', '|']],
                SouthEast,
            ),
            _ => Vec::new(),
        }
    }

    /// Follows `segment` through `cells` (Go's `getLines`). A pass-through character (a joint,
    /// an arrow head, …) ends the current line and starts the next one, so the line is drawn
    /// underneath it; two pass-throughs in a row are not connected (but for vertical lines),
    /// nor is a dot or arrow head after a pass-through. A single segment character that goes
    /// nowhere becomes a lonely line of one cell, unless it belongs to a rounded corner.
    pub(super) fn collect_lines(
        &self,
        cells: impl Iterator<Item = Index>,
        segment: char,
        pass_throughs: &[&[char]],
        o: Orientation,
    ) -> Vec<Line> {
        let passes = |r: char| pass_throughs.iter().any(|set| set.contains(&r));
        let mut lines = Vec::new();
        // Keeps a line that goes somewhere; the current line is then unstarted again.
        let snip = |current: &mut Option<Line>, lines: &mut Vec<Line>| {
            if let Some(line) = current.take().filter(Line::goes_somewhere) {
                lines.push(line);
            }
        };
        let mut current: Option<Line> = None;
        let mut last_seen = ' ';

        for idx in cells {
            let r = self.rune_at(idx);
            let is_pass_through = passes(r);
            let rounded_corner = self.rounded_corner(idx);
            let just_passed_through = passes(last_seen);

            let mut should_keep = (r == segment || is_pass_through) && rounded_corner.is_none();
            // A rounded corner that is also a joint attached to a vertical or diagonal line:
            //  '+--
            //   |
            if rounded_corner.is_some()
                && o != East
                && (self.part_of_vertical_line(idx) || self.part_of_diagonal_line(idx))
            {
                should_keep = true;
            }
            // Don't connect | to > for diagonal lines or )) for horizontal lines.
            if is_pass_through && just_passed_through && o != South {
                snip(&mut current, &mut lines);
            }
            // Don't connect o to o, + to o, etc.
            if just_passed_through && (is_dot(r) || is_triangle(r)) {
                snip(&mut current, &mut lines);
            }

            match &mut current {
                None => {
                    if should_keep {
                        current = Some(Line::new(idx, idx, o));
                    }
                }
                Some(line) => {
                    if !should_keep {
                        if !line.goes_somewhere()
                            && last_seen == segment
                            && !self.part_of_rounded_corner(line.start)
                        {
                            line.stop = idx;
                            line.lonely = true;
                        }
                        snip(&mut current, &mut lines);
                    } else if is_pass_through {
                        // Include the pass-through, and continue from it.
                        line.stop = idx;
                        snip(&mut current, &mut lines);
                        current = Some(Line::new(idx, idx, o));
                    } else {
                        line.stop = idx;
                    }
                }
            }
            last_seen = r;
        }
        lines
    }
}

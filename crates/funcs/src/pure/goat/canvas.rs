//! The character grid and the shapes GoAT finds in it: a port of bep/goat v0.5.0 `canvas.go`,
//! `iter.go` and `index.go` (MIT, `THIRD_PARTY/goat/LICENSE`).
//!
//! Every rule (which characters are text, where a line starts and stops, which joints round
//! a corner) and the order in which the cells are visited are GoAT's, because both decide the
//! SVG bytes: shapes are drawn in the order they are found. Go's channel iterators are plain
//! iterators here, and its `map[Index]rune` grids are dense vectors (a missing cell reads as a
//! space in both).

/// A cell of the grid (Go's `Index`): column `x`, row `y`. Neighbours of edge cells lie
/// outside the grid and read as spaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Index {
    pub(super) x: i32,
    pub(super) y: i32,
}

impl Index {
    const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// The pixel position of the cell (Go's `asPixel`): 8 pixels per column, 16 per row.
    pub(super) const fn pixel(self) -> (i32, i32) {
        (self.x * 8, self.y * 16)
    }

    pub(super) const fn east(self) -> Self {
        Self::new(self.x + 1, self.y)
    }

    pub(super) const fn west(self) -> Self {
        Self::new(self.x - 1, self.y)
    }

    pub(super) const fn north(self) -> Self {
        Self::new(self.x, self.y - 1)
    }

    pub(super) const fn south(self) -> Self {
        Self::new(self.x, self.y + 1)
    }

    pub(super) const fn n_west(self) -> Self {
        Self::new(self.x - 1, self.y - 1)
    }

    pub(super) const fn n_east(self) -> Self {
        Self::new(self.x + 1, self.y - 1)
    }

    pub(super) const fn s_west(self) -> Self {
        Self::new(self.x - 1, self.y + 1)
    }

    pub(super) const fn s_east(self) -> Self {
        Self::new(self.x + 1, self.y + 1)
    }
}

/// The direction a shape faces (Go's `Orientation`; its `NONE` is `Option::None`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Orientation {
    North,
    NorthEast,
    NorthWest,
    South,
    SouthEast,
    SouthWest,
    East,
    West,
}

use Orientation::{East, North, NorthEast, NorthWest, South, SouthEast, SouthWest, West};

/// The cells column by column, each top to bottom (Go's `upDown`).
fn up_down(width: i32, height: i32) -> impl Iterator<Item = Index> {
    (0..width).flat_map(move |x| (0..height).map(move |y| Index::new(x, y)))
}

/// The cells row by row, each left to right (Go's `leftRight`).
fn left_right(width: i32, height: i32) -> impl Iterator<Item = Index> {
    (0..height).flat_map(move |y| (0..width).map(move |x| Index::new(x, y)))
}

/// The cells by anti-diagonal (`x + y` ascending), each left to right (Go's `diagUp`).
fn diag_up(width: i32, height: i32) -> impl Iterator<Item = Index> {
    (0..=width + height - 2).flat_map(move |sum| {
        (0..width).filter_map(move |x| {
            let y = sum - x;
            (0..height).contains(&y).then_some(Index::new(x, y))
        })
    })
}

/// The cells by diagonal (`x - y` ascending), each left to right (Go's `diagDown`).
fn diag_down(width: i32, height: i32) -> impl Iterator<Item = Index> {
    (1 - height..=width).flat_map(move |diff| {
        (0..width).filter_map(move |x| {
            let y = x - diff;
            (0..height).contains(&y).then_some(Index::new(x, y))
        })
    })
}

/// Characters where more than one line segment can come together.
const JOINTS: [char; 5] = ['.', '\'', '+', '*', 'o'];

/// Characters that may belong to the drawing; any other character is text.
const RESERVED: [char; 17] = [
    '-', '_', '|', 'v', '^', '>', '<', 'o', '*', '+', '.', '\'', '/', '\\', ')', '(', ' ',
];

fn is_joint(r: char) -> bool {
    JOINTS.contains(&r)
}

fn is_dot(r: char) -> bool {
    r == 'o' || r == '*'
}

fn is_triangle(r: char) -> bool {
    matches!(r, '^' | 'v' | '<' | '>')
}

/// How far one end of a [`Line`] is pushed outwards, to meet a neighbouring shape (Go's
/// `needsNudgingLeft`/`needsTinyNudgingLeft` for the start, `…Right` for the stop).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Nudge {
    /// A whole cell width.
    pub(super) full: bool,
    /// Half a cell width (and, for a diagonal, half a row along it).
    pub(super) tiny: bool,
}

/// A straight segment between the centres of two cells (Go's `Line`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Line {
    pub(super) start: Index,
    pub(super) stop: Index,
    pub(super) orientation: Orientation,
    /// A segment all by itself (one `/`, `\` or half step): drawn shifted to the cell's
    /// baseline or midline.
    pub(super) lonely: bool,
    /// For a half step: which half of the cell it keeps (`North` keeps the upper half).
    pub(super) chop: Option<Orientation>,
    /// An underscore line, drawn on the cell's baseline (Go's `needsNudgingDown`).
    pub(super) nudge_down: bool,
    pub(super) start_nudge: Nudge,
    pub(super) stop_nudge: Nudge,
}

impl Line {
    fn new(start: Index, stop: Index, orientation: Orientation) -> Self {
        Self {
            start,
            stop,
            orientation,
            lonely: false,
            chop: None,
            nudge_down: false,
            start_nudge: Nudge::default(),
            stop_nudge: Nudge::default(),
        }
    }

    /// A lonely line of one cell, the half step of a vertical line (Go's `newHalfStep`).
    fn half_step(i: Index, chop: Orientation) -> Self {
        let mut line = Self::new(i, i.south(), South);
        line.lonely = true;
        line.chop = Some(chop);
        line
    }

    fn goes_somewhere(&self) -> bool {
        self.start != self.stop
    }

    pub(super) fn horizontal(&self) -> bool {
        matches!(self.orientation, East | West)
    }
}

/// A solid arrow head for `^`, `v`, `<` and `>` (Go's `Triangle`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Triangle {
    pub(super) start: Index,
    pub(super) orientation: Orientation,
    /// Pushed against the shape it points at.
    pub(super) nudge: bool,
}

/// An `o` (open) or `*` (bold) circle (Go's `Circle`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Circle {
    pub(super) start: Index,
    pub(super) bold: bool,
}

/// A rounded corner such as `.-` over `|` (Go's `RoundedCorner`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RoundedCorner {
    pub(super) start: Index,
    pub(super) orientation: Orientation,
}

/// `-)-` or `-(-`: a vertical line hopping over a horizontal one (Go's `Bridge`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Bridge {
    pub(super) start: Index,
    pub(super) orientation: Orientation,
}

/// One character of text (Go's `Text`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Text {
    pub(super) start: Index,
    pub(super) ch: char,
}

/// The shapes of the lists that mix kinds (Go's `Drawable`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shape {
    Line(Line),
    Triangle(Triangle),
    Bridge(Bridge),
    Text(Text),
}

/// The diagram as a grid of characters (Go's `Canvas`), with the text already set apart.
#[derive(Debug)]
pub(super) struct Canvas {
    /// The widest line, in characters.
    pub(super) width: i32,
    /// The number of lines.
    pub(super) height: i32,
    /// The drawing (Go's `data`), row by row; text cells hold a space.
    cells: Vec<char>,
    /// The characters read as text, row by row.
    text: Vec<Option<char>>,
}

impl Canvas {
    /// Reads `input` (Go's `NewCanvas`): lines as `bufio.ScanLines` splits them (at `\n`, one
    /// trailing `\r` dropped, no empty line after a final `\n`), one cell per `char` (a tab is
    /// one cell, as in GoAT). Then sets apart every character [`Canvas::is_text`] calls text,
    /// visiting the cells row by row (the order matters: a reserved character right of text
    /// is text too). (Go's scanner also stops reading at a line longer than 64 KiB; that limit
    /// is not reproduced.)
    ///
    /// `None` when the grid is too large for `i32` pixel coordinates.
    pub(super) fn new(input: &str) -> Option<Self> {
        let mut lines: Vec<&str> = input.split('\n').collect();
        if input.is_empty() || input.ends_with('\n') {
            lines.pop();
        }
        let rows: Vec<Vec<char>> = lines
            .iter()
            .map(|l| l.strip_suffix('\r').unwrap_or(l).chars().collect())
            .collect();
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        let height = rows.len();
        // A pixel coordinate is at most 16 × (size + 1).
        let limit = usize::try_from(i32::MAX / 16 - 1).unwrap_or(usize::MAX);
        if width > limit || height > limit {
            return None;
        }
        let mut cells = vec![' '; width * height];
        for (y, row) in rows.iter().enumerate() {
            cells[y * width..y * width + row.len()].copy_from_slice(row);
        }
        let mut canvas = Self {
            width: i32::try_from(width).ok()?,
            height: i32::try_from(height).ok()?,
            text: vec![None; cells.len()],
            cells,
        };
        for idx in left_right(canvas.width, canvas.height) {
            if canvas.is_text(idx) {
                let r = canvas.rune_at(idx);
                if let Some(pos) = canvas.pos(idx) {
                    canvas.text[pos] = Some(r);
                }
            }
        }
        for (cell, text) in canvas.cells.iter_mut().zip(&canvas.text) {
            if text.is_some() {
                *cell = ' ';
            }
        }
        Some(canvas)
    }

    /// The position of `i` in the row-by-row vectors, if it lies on the grid.
    fn pos(&self, i: Index) -> Option<usize> {
        let on_grid = (0..self.width).contains(&i.x) && (0..self.height).contains(&i.y);
        // Non-negative on the grid, so the casts are lossless.
        on_grid.then(|| i.y as usize * self.width as usize + i.x as usize)
    }

    /// The drawing character at `i` (Go's `runeAt`): a space for text and off the grid.
    fn rune_at(&self, i: Index) -> char {
        self.pos(i).map_or(' ', |p| self.cells[p])
    }

    /// The text character at `i`, if `i` holds text.
    fn text_at(&self, i: Index) -> Option<char> {
        self.pos(i).and_then(|p| self.text[p])
    }

    /// The grid as text again, text and drawing merged and short lines padded with spaces
    /// (Go's `String`).
    #[cfg(test)]
    pub(super) fn to_text(&self) -> String {
        let mut out = String::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let i = Index::new(x, y);
                out.push(self.text_at(i).unwrap_or_else(|| self.rune_at(i)));
            }
            out.push('\n');
        }
        out
    }

    /// Every line (Go's `Lines`): `-` midlines, `_` baselines, `|` verticals, `/` and `\`
    /// diagonals, then the half steps, each nudged to meet its neighbours.
    pub(super) fn lines(&self) -> Vec<Line> {
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
    fn half_steps(&self) -> Vec<Line> {
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
    fn lines_for_segment(&self, segment: char) -> Vec<Line> {
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
    fn collect_lines(
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

    /// The arrow heads, with the tails that connect some of them to a line (Go's
    /// `Triangles`), column by column.
    pub(super) fn triangles(&self) -> Vec<Shape> {
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
    pub(super) fn circles(&self) -> Vec<Circle> {
        up_down(self.width, self.height)
            .filter_map(|start| match self.rune_at(start) {
                'o' => Some(Circle { start, bold: false }),
                '*' => Some(Circle { start, bold: true }),
                _ => None,
            })
            .collect()
    }

    /// Every rounded corner (Go's `RoundedCorners`), row by row.
    pub(super) fn rounded_corners(&self) -> Vec<RoundedCorner> {
        left_right(self.width, self.height)
            .filter_map(|start| {
                self.rounded_corner(start)
                    .map(|orientation| RoundedCorner { start, orientation })
            })
            .collect()
    }

    /// The orientation of the rounded corner a joint at `i` makes (Go's `isRoundedCorner`).
    fn rounded_corner(&self, i: Index) -> Option<Orientation> {
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
    pub(super) fn text(&self) -> Vec<Shape> {
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
    pub(super) fn bridges(&self) -> Vec<Shape> {
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
    fn bridge(&self, i: Index) -> Option<Orientation> {
        if self.rune_at(i.west()) != '-' || self.rune_at(i.east()) != '-' {
            return None;
        }
        match self.rune_at(i) {
            '(' => Some(West),
            ')' => Some(East),
            _ => None,
        }
    }

    /// Whether the character at `i` is text (Go's `isText`): any character GoAT does not
    /// reserve, and reserved characters that have no line above or below and either follow
    /// text, precede an unreserved character, or stand between spaces next to text.
    fn is_text(&self, i: Index) -> bool {
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
    fn is_reserved(&self, i: Index) -> bool {
        RESERVED.contains(&self.rune_at(i))
    }

    /// Whether the character at `i` belongs to anything but a horizontal line (Go's
    /// `hasLineAboveOrBelow`).
    fn has_line_above_or_below(&self, i: Index) -> bool {
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
    fn part_of_vertical_line(&self, i: Index) -> bool {
        let this = self.rune_at(i);
        let north = self.rune_at(i.north());
        let south = self.rune_at(i.south());
        north == '|'
            || (this == '|' && is_joint(north))
            || south == '|'
            || (this == '|' && is_joint(south))
    }

    /// Whether a diagonal segment passes through `i` (Go's `partOfDiagonalLine`).
    fn part_of_diagonal_line(&self, i: Index) -> bool {
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
    fn part_of_rounded_corner(&self, i: Index) -> bool {
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
    fn part_of_half_step(&self, i: Index) -> Option<Orientation> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Go's `TestIterators`.
    #[test]
    fn iterators() {
        let pairs = |it: &mut dyn Iterator<Item = Index>| -> Vec<(i32, i32)> {
            it.map(|i| (i.x, i.y)).collect()
        };
        // 1 3
        // 2 4
        assert_eq!(pairs(&mut up_down(2, 2)), [(0, 0), (0, 1), (1, 0), (1, 1)]);
        // 1 2
        // 3 4
        assert_eq!(
            pairs(&mut left_right(2, 2)),
            [(0, 0), (1, 0), (0, 1), (1, 1)]
        );
        // 1 3
        // 2 5
        // 4 6
        assert_eq!(
            pairs(&mut diag_up(2, 3)),
            [(0, 0), (0, 1), (1, 0), (0, 2), (1, 1), (1, 2)]
        );
        // 2 4 6
        // 1 3 5
        assert_eq!(
            pairs(&mut diag_down(3, 2)),
            [(0, 1), (0, 0), (1, 1), (1, 0), (2, 1), (2, 0)]
        );
    }

    /// Go's `TestReadASCII`.
    #[test]
    fn read_ascii() {
        let canvas = Canvas::new(" +-->\n | å\n +----->").expect("small");
        assert_eq!((canvas.width, canvas.height), (8, 3));
        assert_eq!(canvas.to_text(), " +-->   \n | å    \n +----->\n");
    }

    /// Lines as `bufio.ScanLines` splits them.
    #[test]
    fn scan_lines() {
        let size = |s: &str| {
            let c = Canvas::new(s).expect("small");
            (c.width, c.height)
        };
        assert_eq!(size(""), (0, 0));
        assert_eq!(size("\n"), (0, 1));
        assert_eq!(size("ab\r\nc\r"), (2, 2));
        assert_eq!(size("a\n\n"), (1, 2));
        assert_eq!(size("\ta\r\r\n"), (3, 1));
    }

    #[test]
    fn text_is_set_apart() {
        let canvas = Canvas::new("+--+ foo\n|  |-->").expect("small");
        assert_eq!(canvas.to_text(), "+--+ foo\n|  |--> \n");
        let text: String = canvas
            .text()
            .iter()
            .filter_map(|s| match s {
                Shape::Text(t) => Some(t.ch),
                _ => None,
            })
            .collect();
        assert_eq!(text, "foo");
    }
}

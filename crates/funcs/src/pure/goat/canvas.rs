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

mod cells;
mod lines;
mod shapes;

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
}

#[cfg(test)]
mod tests;

//! The SVG elements of the shapes: a port of bep/goat v0.5.0 `svg.go` (MIT,
//! `THIRD_PARTY/goat/LICENSE`), byte for byte: element and attribute order, single quotes,
//! one element per line, integers for paths and circles, and Go's `%f` (six decimals) for the
//! `float32` coordinates of arrow heads (Rust's `{:.6}` of an `f32` rounds the same way: exact
//! decimal value, ties to even).

use std::fmt::Write as _;

use super::canvas::{
    Bridge, Canvas, Circle, Line, Orientation, RoundedCorner, Shape, Text, Triangle,
};

use Orientation::{East, North, NorthEast, NorthWest, South, SouthEast, SouthWest, West};

/// The `<g>` element with every shape of `canvas` (Go's `WriteSVGBody`): lines, arrow heads,
/// rounded corners, circles, bridges, then text.
pub(super) fn body(canvas: &Canvas) -> String {
    let mut out = String::from("<g transform='translate(8,16)'>\n");
    for line in canvas.lines() {
        line.draw(&mut out);
    }
    for shape in canvas.triangles() {
        shape.draw(&mut out);
    }
    for corner in canvas.rounded_corners() {
        corner.draw(&mut out);
    }
    for circle in canvas.circles() {
        circle.draw(&mut out);
    }
    for shape in canvas.bridges() {
        shape.draw(&mut out);
    }
    for shape in canvas.text() {
        shape.draw(&mut out);
    }
    out.push_str("</g>\n");
    out
}

impl Shape {
    fn draw(&self, out: &mut String) {
        match self {
            Self::Line(line) => line.draw(out),
            Self::Triangle(triangle) => triangle.draw(out),
            Self::Bridge(bridge) => bridge.draw(out),
            Self::Text(text) => text.draw(out),
        }
    }
}

impl Line {
    /// A `<path>` from cell centre to cell centre, shifted for lonely segments and half steps
    /// and stretched by the nudges, so lines meet their neighbours: a vertical line that hits a
    /// horizontal one stops at its midline, a lone diagonal lines up with `_` baselines.
    fn draw(&self, out: &mut String) {
        let (mut x0, mut y0) = self.start.pixel();
        let (mut x1, mut y1) = self.stop.pixel();
        if self.lonely {
            match self.orientation {
                NorthEast => {
                    x0 -= 4;
                    x1 -= 4;
                    y0 += 8;
                    y1 += 8;
                }
                SouthEast => {
                    x0 -= 4;
                    x1 -= 4;
                    y0 -= 8;
                    y1 -= 8;
                }
                South => {
                    y0 -= 8;
                    y1 -= 8;
                }
                _ => {}
            }
            // Half steps.
            match self.chop {
                Some(North) => y1 -= 8,
                Some(South) => y0 += 8,
                _ => {}
            }
        }
        if self.nudge_down {
            y1 += 8;
            if self.horizontal() {
                y0 += 8;
            }
        }
        if self.start_nudge.full {
            x0 -= 8;
        }
        if self.stop_nudge.full {
            x1 += 8;
        }
        if self.start_nudge.tiny {
            x0 -= 4;
            match self.orientation {
                NorthEast => y0 += 8,
                SouthEast => y0 -= 8,
                _ => {}
            }
        }
        if self.stop_nudge.tiny {
            x1 += 4;
            match self.orientation {
                NorthEast => y1 -= 8,
                SouthEast => y1 += 8,
                _ => {}
            }
        }
        let _ = writeln!(
            out,
            "<path d='M {x0},{y0} L {x1},{y1}' fill='none' stroke='currentColor'></path>"
        );
    }
}

impl Triangle {
    /// A `<polygon>` pointing east, rotated about the cell's position, in `f32` arithmetic as
    /// Go's `float32`.
    fn draw(&self, out: &mut String) {
        // Go's `0.35*16`, a constant rounded once to `float32`.
        const HALF_BASE: f32 = 5.6;
        let (px, py) = self.start.pixel();
        // Go's `float32(int)`, rounding to nearest as `as` does.
        let (x, y) = (px as f32, py as f32);
        let (mut x0, y0) = (x + 8.0, y);
        let (mut x1, y1) = (x - 4.0, y - HALF_BASE);
        let (mut x2, y2) = (x - 4.0, y + HALF_BASE);
        let mut shift = |dx: f32| {
            x0 += dx;
            x1 += dx;
            x2 += dx;
        };
        let rotation: f64 = match self.orientation {
            North => {
                if self.nudge {
                    shift(8.0);
                }
                270.0
            }
            NorthEast | NorthWest | SouthWest | SouthEast => {
                shift(4.0);
                if self.nudge {
                    shift(6.0);
                }
                match self.orientation {
                    NorthEast => 300.0,
                    NorthWest => 240.0,
                    SouthWest => 120.0,
                    _ => 60.0,
                }
            }
            West => {
                if self.nudge {
                    shift(-8.0);
                }
                180.0
            }
            East => {
                if self.nudge {
                    shift(-8.0);
                }
                0.0
            }
            South => {
                if self.nudge {
                    shift(8.0);
                }
                90.0
            }
        };
        let _ = writeln!(
            out,
            "<polygon points='{x0:.6},{y0:.6} {x1:.6},{y1:.6} {x2:.6},{y2:.6}' \
             fill='currentColor' transform='rotate({rotation:.6}, {x:.6}, {y:.6})'></polygon>"
        );
    }
}

impl Circle {
    /// A `<circle>` of radius 6, white (`o`) or filled (`*`).
    fn draw(&self, out: &mut String) {
        let fill = if self.bold { "currentColor" } else { "#fff" };
        let (cx, cy) = self.start.pixel();
        let _ = writeln!(
            out,
            "<circle cx='{cx}' cy='{cy}' r='6' stroke='currentColor' fill='{fill}'></circle>"
        );
    }
}

impl Text {
    /// A `<text>` of one character, or for Markdeep's shade characters a grey `<rect>` (written
    /// without a line break, as Go does).
    fn draw(&self, out: &mut String) {
        let (x, y) = self.start.pixel();
        // Markdeep's checkerboard shades.
        let shade = match self.ch {
            '▉' => Some("currentColor"),
            '▓' => Some("rgb(64,64,64)"),
            '▒' => Some("rgb(128,128,128)"),
            '░' => Some("rgb(191,191,191)"),
            _ => None,
        };
        if let Some(fill) = shade {
            let _ = write!(
                out,
                "<rect x='{}' y='{}' width='8' height='16' fill='{fill}'></rect>",
                x - 4,
                y - 8
            );
            return;
        }
        let mut buf = [0; 4];
        let ch = match self.ch {
            '&' => "&amp;",
            '>' => "&gt;",
            '<' => "&lt;",
            c => c.encode_utf8(&mut buf),
        };
        let _ = writeln!(
            out,
            "<text text-anchor='middle' x='{x}' y='{}' fill='currentColor' \
             style='font-size:1em'>{ch}</text>",
            y + 4
        );
    }
}

impl RoundedCorner {
    /// A quarter circle `<path>` (an elliptical arc of radius 16) from the end of the
    /// horizontal line to the end of the vertical one.
    fn draw(&self, out: &mut String) {
        let (x, y) = self.start.pixel();
        let (start_x, start_y, sweep, end_x, end_y) = match self.orientation {
            NorthWest => (x + 8, y, 0, x - 8, y + 16),
            NorthEast => (x - 8, y, 1, x + 8, y + 16),
            SouthEast => (x + 8, y - 16, 1, x - 8, y),
            SouthWest => (x - 8, y - 16, 0, x + 8, y),
            // Not a corner orientation; Go draws its zero values.
            _ => (0, 0, 0, 0, 0),
        };
        let _ = writeln!(
            out,
            "<path d='M {start_x},{start_y} A 16,16 0 0,{sweep} {end_x},{end_y}' fill='none' \
             stroke='currentColor'></path>"
        );
    }
}

impl Bridge {
    /// A half circle `<path>` (an arc of radius 9) over the cell, bulging east for `)` and
    /// west for `(`.
    fn draw(&self, out: &mut String) {
        let (x, y) = self.start.pixel();
        let sweep = u8::from(self.orientation != West);
        let _ = writeln!(
            out,
            "<path d='M {x},{} A 9,9 0 0,{sweep} {x},{}' fill='none' stroke='currentColor'></path>",
            y - 8,
            y + 8
        );
    }
}

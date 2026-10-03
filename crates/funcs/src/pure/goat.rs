//! `diagrams_goat` (feature `goat`): ASCII diagrams to SVG, as Go's `diagrams.Goat`
//! (`tpl/diagrams/goat.go`) with GoAT, the Go port of Markdeep's diagrams.
//!
//! [`canvas`] and [`svg`] port bep/goat v0.5.0 (the version Go uses; MIT,
//! `THIRD_PARTY/goat/LICENSE`), so the SVG is byte-identical to Go's: one `<path>` per line
//! segment and corner, a `<polygon>` per arrow head, a `<circle>` per dot and one `<text>` per
//! character of text, in GoAT's order. A character cell is 8×16 pixels; the size is one column
//! and half a row of margin more than the diagram.

mod canvas;
mod svg;

use super::Registrar;
use super::value::text;
use tera::{Map, Value};

use canvas::Canvas;

pub(super) fn register(r: &mut Registrar<'_>) {
    r.function("diagrams_goat", |kw, _| {
        let input = kw.must_get::<Value>("text")?;
        let svg = goat(&text(&input, "diagrams_goat")?).map_err(tera::Error::message)?;
        let mut map = Map::new();
        map.insert("inner".into(), Value::safe_string(&svg.inner));
        map.insert("wrapped".into(), Value::safe_string(&svg.wrapped()));
        map.insert("width".into(), Value::from(svg.width));
        map.insert("height".into(), Value::from(svg.height));
        Ok(Value::from(map))
    });
}

/// A rendered diagram (Go's `goat.SVG`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoatSvg {
    /// The SVG content (without the `<svg>` element): one `<g>` element and a line break.
    pub inner: String,
    /// Width and height in pixels.
    pub width: usize,
    pub height: usize,
}

impl GoatSvg {
    /// A complete `<svg>` element, as the Go templates' `.Wrapped` (GoAT's `SVG.String`).
    #[must_use]
    pub fn wrapped(&self) -> String {
        format!(
            "<svg class='diagram' xmlns='http://www.w3.org/2000/svg' version='1.1' height='{}' \
             width='{}' font-family='Menlo,Lucida Console,monospace'>\n{}</svg>\n",
            self.height, self.width, self.inner
        )
    }
}

/// The SVG of the ASCII diagram `text` (Go's `goat.BuildSVG`): `(columns + 1) × 8` pixels
/// wide and `rows × 16 + 9` high.
///
/// # Errors
/// When the diagram has more than about 134 million rows or columns (its pixel coordinates
/// would not fit an `i32`).
pub fn goat(text: &str) -> Result<GoatSvg, String> {
    let canvas =
        Canvas::new(text).ok_or_else(|| "diagrams_goat: the diagram is too large".to_owned())?;
    let size = |n: i32| usize::try_from(n).unwrap_or_default();
    Ok(GoatSvg {
        inner: svg::body(&canvas),
        width: (size(canvas.width) + 1) * 8,
        height: size(canvas.height) * 16 + 8 + 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_and_structure() {
        let svg = goat("+--+\n|  |-->\n+--+").unwrap();
        assert_eq!((svg.width, svg.height), (8 * 8, 3 * 16 + 9));
        assert!(
            svg.inner.starts_with("<g transform='translate(8,16)'>\n"),
            "{}",
            svg.inner
        );
        assert!(svg.inner.ends_with("</g>\n"));
        assert!(!svg.inner.contains("<svg"));
        assert!(svg.wrapped().starts_with(
            "<svg class='diagram' xmlns='http://www.w3.org/2000/svg' version='1.1' height='57' \
             width='64' font-family='Menlo,Lucida Console,monospace'>\n<g "
        ));
    }

    #[test]
    fn empty() {
        let svg = goat("").unwrap();
        assert_eq!((svg.width, svg.height), (8, 9));
        assert_eq!(svg.inner, "<g transform='translate(8,16)'>\n</g>\n");
    }

    /// One `<text>` per character, escaped as GoAT escapes (`&`, `<`, `>` only).
    #[test]
    fn text_characters() {
        let svg = goat("a&b'c").unwrap();
        let texts: Vec<&str> = svg
            .inner
            .lines()
            .filter_map(|l| l.strip_suffix("</text>"))
            .filter_map(|l| l.rsplit_once('>').map(|(_, t)| t))
            .collect();
        assert_eq!(texts, ["a", "&amp;", "b", "'", "c"]);
    }
}

//! Headings, fragments and the table of contents.

use serde::Serialize;

use crate::TocOptions;

/// A heading of the table of contents. Levels a document skips are filled with empty
/// headings (no id, no title), as Go does.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Heading {
    pub id: String,
    /// 1–6; 0 for a filler, and (as in Go) for a heading without an id.
    pub level: u8,
    /// The title as HTML (rendered without hooks).
    pub html: String,
    /// The title as plain text.
    pub plain: String,
    pub children: Vec<Heading>,
}

impl Heading {
    fn is_empty(&self) -> bool {
        self.id.is_empty() && self.html.is_empty()
    }
}

/// The headings of a page (Go's `.Fragments`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Fragments {
    /// The heading tree.
    pub headings: Vec<Heading>,
    /// Every heading id, sorted (an id given twice appears twice).
    pub identifiers: Vec<String>,
}

impl Fragments {
    /// Whether some heading has id `id`.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.identifiers
            .binary_search_by(|x| x.as_str().cmp(id))
            .is_ok()
    }

    /// How many headings have id `id`.
    #[must_use]
    pub fn count(&self, id: &str) -> usize {
        self.identifiers.iter().filter(|x| *x == id).count()
    }
}

/// The table of contents (Go's `.TableOfContents`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Toc {
    pub headings: Vec<Heading>,
}

impl Toc {
    /// `<nav id="TableOfContents">` with the levels `o` selects.
    #[must_use]
    pub fn to_html(&self, o: &TocOptions) -> String {
        let mut b = TocWriter {
            out: String::from("<nav id=\"TableOfContents\">"),
            o,
        };
        b.headings(1, 0, &self.headings);
        b.out.push_str("</nav>");
        b.out
    }
}

struct TocWriter<'o> {
    out: String,
    o: &'o TocOptions,
}

impl TocWriter<'_> {
    fn indent(&mut self, n: usize) {
        for _ in 0..n {
            self.out.push_str("  ");
        }
    }

    fn list_tag(&self, close: bool) -> &'static str {
        match (self.o.ordered, close) {
            (true, false) => "<ol>\n",
            (true, true) => "</ol>",
            (false, false) => "<ul>\n",
            (false, true) => "</ul>",
        }
    }

    fn headings(&mut self, level: u8, indent: usize, hs: &[Heading]) {
        if level < self.o.start {
            for h in hs {
                self.headings(level + 1, indent, &h.children);
            }
            return;
        }
        if self.o.end.is_some_and(|end| level > end) {
            return;
        }
        if !hs.is_empty() {
            self.out.push('\n');
            self.indent(indent + 1);
            self.out.push_str(self.list_tag(false));
        }
        for h in hs {
            self.heading(level + 1, indent + 2, h);
        }
        if !hs.is_empty() {
            self.indent(indent + 1);
            self.out.push_str(self.list_tag(true));
            self.out.push('\n');
            self.indent(indent);
        }
    }

    fn heading(&mut self, level: u8, indent: usize, h: &Heading) {
        self.indent(indent);
        self.out.push_str("<li>");
        if !h.is_empty() {
            self.out.push_str("<a href=\"#");
            self.out.push_str(&h.id);
            self.out.push_str("\">");
            self.out.push_str(&h.html);
            self.out.push_str("</a>");
        }
        self.headings(level, indent, &h.children);
        self.out.push_str("</li>\n");
    }
}

/// Builds the heading tree from `(level, heading)` in document order: a level-1 heading
/// starts a new top entry, deeper ones hang under the last entry of the level above,
/// with empty fillers for skipped levels.
pub(crate) fn tree(entries: Vec<(u8, Heading)>) -> Vec<Heading> {
    let mut roots: Vec<Heading> = Vec::new();
    let mut row: Option<usize> = None;
    for (level, h) in entries {
        let r = match row {
            Some(r) if level != 1 => r,
            Some(r) => r + 1,
            None => 0,
        };
        row = Some(r);
        while roots.len() <= r {
            roots.push(Heading::default());
        }
        let depth = usize::from(level.saturating_sub(1));
        if depth == 0 {
            roots[r] = h;
            continue;
        }
        let mut cur = &mut roots[r];
        for _ in 1..depth {
            if cur.children.is_empty() {
                cur.children.push(Heading::default());
            }
            cur = cur.children.last_mut().expect("a child was just ensured");
        }
        cur.children.push(h);
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(id: &str, level: u8) -> Heading {
        Heading {
            id: id.into(),
            level,
            html: id.into(),
            plain: id.into(),
            children: Vec::new(),
        }
    }

    #[test]
    fn toc_like_go() {
        let t = Toc {
            headings: tree(vec![(3, h("homebrew", 3))]),
        };
        assert_eq!(
            t.to_html(&TocOptions {
                start: 2,
                end: Some(3),
                ordered: false
            }),
            "<nav id=\"TableOfContents\">\n  <ul>\n    <li>\n      <ul>\n        <li><a href=\"#homebrew\">homebrew</a></li>\n      </ul>\n    </li>\n  </ul>\n</nav>"
        );
        let deep = tree(vec![
            (4, h("starts-deep", 4)),
            (2, h("then-two", 2)),
            (1, h("then-one", 1)),
            (6, h("six", 6)),
        ]);
        assert_eq!(deep.len(), 2);
        assert_eq!(deep[0].children[1].id, "then-two");
        assert_eq!(
            deep[1].children[0].children[0].children[0].children[0].children[0].id,
            "six"
        );
    }
}

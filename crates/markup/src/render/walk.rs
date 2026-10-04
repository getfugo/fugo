//! The walk over the tree: what each node writes when it is entered and when it is left.

use super::*;

impl<'r, 'a> Renderer<'r, 'a> {
    #[expect(clippy::too_many_lines, reason = "one arm per node kind")]
    pub(super) fn enter(&mut self, n: Node<'a>) -> Result<Walk, MarkupError> {
        let value = n.data().value.clone();
        match value {
            NodeValue::Document
            | NodeValue::BlockQuote
            | NodeValue::Heading(_)
            | NodeValue::Link(_)
            | NodeValue::Image(_)
            | NodeValue::TableCell
            | NodeValue::TableRow(_)
            | NodeValue::DescriptionItem(_)
            | NodeValue::Escaped => {
                if let NodeValue::TableRow(header) = value
                    && let Some(t) = self.tables.last_mut()
                {
                    if header { &mut t.head } else { &mut t.body }.push(Vec::new());
                }
                Ok(Walk::Children)
            }
            NodeValue::FrontMatter(_) | NodeValue::FootnoteDefinition(_) => Ok(Walk::Done),
            NodeValue::List(l) => {
                self.out.push_str(match l.list_type {
                    ListType::Bullet => "<ul",
                    ListType::Ordered => "<ol",
                });
                if l.list_type == ListType::Ordered && l.start != 1 {
                    self.out.push_str(&format!(" start=\"{}\"", l.start));
                }
                let a = self.attrs_of(n);
                escape::attrs(&mut self.out, a, &["start", "reversed", "type"]);
                self.out.push_str(">\n");
                Ok(Walk::Children)
            }
            NodeValue::Item(_) | NodeValue::TaskItem(_) => {
                self.out.push_str("<li");
                let a = self.attrs_of(n);
                escape::attrs(&mut self.out, a, &["value"]);
                self.out.push('>');
                match n.first_child() {
                    Some(fc) if matches!(fc.data().value, NodeValue::Paragraph) => {
                        if !self.text_block(fc) {
                            self.out.push('\n');
                        }
                    }
                    Some(_) => {
                        self.out.push('\n');
                        self.checkbox(n);
                    }
                    None => self.checkbox(n),
                }
                Ok(Walk::Children)
            }
            NodeValue::DescriptionList => {
                self.out.push_str("<dl");
                let a = self.attrs_of(n);
                escape::attrs(&mut self.out, a, &[]);
                self.out.push_str(">\n");
                Ok(Walk::Children)
            }
            NodeValue::DescriptionTerm => {
                self.out.push_str("<dt");
                if let Some(id) = self.doc.extra(n).and_then(|e| e.id.as_deref()) {
                    self.out.push_str(" id=\"");
                    escape::html(&mut self.out, id);
                    self.out.push('"');
                }
                self.out.push('>');
                Ok(Walk::Children)
            }
            NodeValue::DescriptionDetails => {
                self.out.push_str("<dd>");
                if !matches!(self.doc.role(n), Some(Role::TightDetails)) {
                    self.out.push('\n');
                }
                Ok(Walk::Children)
            }
            NodeValue::Paragraph => {
                if !self.text_block(n) {
                    self.out.push_str("<p");
                    let a = self.attrs_of(n);
                    escape::attrs(&mut self.out, a, &[]);
                    self.out.push('>');
                }
                if let Some(item) = n.parent()
                    && item.first_child().is_some_and(|f| f.same_node(n))
                {
                    self.checkbox(item);
                }
                Ok(Walk::Children)
            }
            NodeValue::ThematicBreak => {
                self.out.push_str("<hr");
                let a = self.attrs_of(n);
                escape::attrs(&mut self.out, a, &[]);
                self.out.push_str(self.void());
                self.out.push('\n');
                Ok(Walk::Done)
            }
            NodeValue::CodeBlock(cb) => {
                self.code_block(n, &cb)?;
                Ok(Walk::Done)
            }
            NodeValue::HtmlBlock(b) => {
                match self.o.raw_html {
                    RawHtml::Pass => self.out.push_str(&b.literal),
                    RawHtml::Omit => self.out.push_str("<!-- raw HTML omitted -->\n"),
                }
                Ok(Walk::Done)
            }
            NodeValue::Table(t) => {
                self.tables.push(TableBuild {
                    head: Vec::new(),
                    body: Vec::new(),
                    alignments: t.alignments.clone(),
                });
                Ok(Walk::Children)
            }
            NodeValue::Text(t) => {
                escape::html(&mut self.out, &t);
                Ok(Walk::Done)
            }
            NodeValue::SoftBreak => {
                if self.o.line_breaks == LineBreaks::Hard {
                    self.out.push_str("<br");
                    self.out.push_str(self.void());
                }
                self.out.push('\n');
                Ok(Walk::Done)
            }
            NodeValue::LineBreak => {
                self.out.push_str("<br");
                self.out.push_str(self.void());
                self.out.push('\n');
                Ok(Walk::Done)
            }
            NodeValue::Code(c) => {
                self.out.push_str("<code>");
                escape::html(&mut self.out, &c.literal);
                self.out.push_str("</code>");
                Ok(Walk::Done)
            }
            NodeValue::HtmlInline(h) => {
                match self.o.raw_html {
                    RawHtml::Pass => self.out.push_str(&h),
                    RawHtml::Omit => self.out.push_str("<!-- raw HTML omitted -->"),
                }
                Ok(Walk::Done)
            }
            NodeValue::Raw(r) => {
                self.raw(n, &r)?;
                Ok(Walk::Done)
            }
            NodeValue::Emph => {
                self.out.push_str("<em>");
                Ok(Walk::Children)
            }
            NodeValue::Strong => {
                self.out.push_str("<strong>");
                Ok(Walk::Children)
            }
            NodeValue::Strikethrough => {
                self.out.push_str("<del>");
                Ok(Walk::Children)
            }
            NodeValue::FootnoteReference(f) => {
                let back = if f.ref_num > 1 {
                    (f.ref_num - 1).to_string()
                } else {
                    String::new()
                };
                self.out.push_str(&format!(
                    "<sup id=\"fnref{back}:{ix}\"><a href=\"#fn:{ix}\" class=\"footnote-ref\" role=\"doc-noteref\">{ix}</a></sup>",
                    ix = f.ix
                ));
                Ok(Walk::Done)
            }
            NodeValue::ShortCode(sc) => {
                // goldmark-emoji's `Entity` rendering (v1.0.6 `renderEmoji`): the zero-width
                // joiner by name.
                for c in sc.emoji.chars() {
                    if c == '\u{200d}' {
                        self.out.push_str("&zwj;");
                    } else {
                        self.out.push_str(&format!("&#x{:x};", u32::from(c)));
                    }
                }
                Ok(Walk::Done)
            }
            _ => Ok(Walk::Children),
        }
    }

    pub(super) fn exit(&mut self, n: Node<'a>, mark: usize) -> Result<(), MarkupError> {
        let value = n.data().value.clone();
        match value {
            NodeValue::Document => self.footnotes()?,
            NodeValue::BlockQuote => self.blockquote(n, mark)?,
            NodeValue::List(l) => self.out.push_str(match l.list_type {
                ListType::Bullet => "</ul>\n",
                ListType::Ordered => "</ol>\n",
            }),
            NodeValue::Item(_) | NodeValue::TaskItem(_) => self.out.push_str("</li>\n"),
            NodeValue::DescriptionList => self.out.push_str("</dl>\n"),
            NodeValue::DescriptionTerm => self.out.push_str("</dt>\n"),
            NodeValue::DescriptionDetails => self.out.push_str("</dd>\n"),
            NodeValue::Paragraph => {
                if !self.text_block(n) {
                    self.out.push_str("</p>\n");
                } else if n.next_sibling().is_some()
                    && n.first_child().is_some()
                    && !n
                        .parent()
                        .is_some_and(|p| matches!(p.data().value, NodeValue::DescriptionTerm))
                {
                    self.out.push('\n');
                }
            }
            NodeValue::Heading(h) => self.heading(n, h.level, mark)?,
            NodeValue::Emph => self.out.push_str("</em>"),
            NodeValue::Strong => self.out.push_str("</strong>"),
            NodeValue::Strikethrough => self.out.push_str("</del>"),
            NodeValue::Link(l) => self.link(n, &l.url, &l.title, mark, false)?,
            NodeValue::Image(l) => self.link(n, &l.url, &l.title, mark, true)?,
            NodeValue::TableCell => {
                let text = self.out.split_off(mark);
                let col = n.preceding_siblings().count() - 1;
                let padded = matches!(self.doc.role(n), Some(Role::PaddedCell));
                if let Some(t) = self.tables.last_mut() {
                    let alignment = if padded {
                        Alignment::None
                    } else {
                        match t.alignments.get(col) {
                            Some(TableAlignment::Left) => Alignment::Left,
                            Some(TableAlignment::Center) => Alignment::Center,
                            Some(TableAlignment::Right) => Alignment::Right,
                            _ => Alignment::None,
                        }
                    };
                    let header = n
                        .parent()
                        .is_some_and(|r| matches!(r.data().value, NodeValue::TableRow(true)));
                    let rows = if header { &mut t.head } else { &mut t.body };
                    if let Some(row) = rows.last_mut() {
                        row.push(Cell { text, alignment });
                    }
                }
            }
            NodeValue::Table(_) => self.table(n)?,
            _ => {}
        }
        Ok(())
    }
}

//! Tables, code blocks, raw HTML and footnotes.

use super::*;

pub(super) fn write_rows(out: &mut String, rows: &[Vec<Cell>], tag: &str) {
    for row in rows {
        out.push_str("\n      <tr>");
        for c in row {
            out.push_str(&format!("\n          <{tag}"));
            if c.alignment != Alignment::None {
                out.push_str(&format!(" style=\"text-align: {}\"", c.alignment.as_str()));
            }
            out.push('>');
            out.push_str(&c.text);
            out.push_str(&format!("</{tag}>"));
        }
        out.push_str("\n      </tr>");
    }
}

/// The `{…}` of a fence info string: `(attributes, options)`. A `{…}` that does not parse
/// is an error (a `{` without `}` is not an attribute block).
pub(super) fn fence_attributes(info: &str) -> Result<attributes::Converted, String> {
    let Some(close) = info.find('}') else {
        return Ok(Default::default());
    };
    let Some(open) = info[..close].rfind('{') else {
        return Ok(Default::default());
    };
    let attrs = attributes::parse(&info[open..=close])
        .filter(|(_, n)| *n == close + 1 - open)
        .map(|(a, _)| a)
        .ok_or_else(|| {
            "failed to parse Markdown attributes; you may need to quote the values".to_owned()
        })?;
    attributes::convert(&attrs, Owner::CodeBlock).map_err(|e| e.to_string())
}

impl<'r, 'a> Renderer<'r, 'a> {
    pub(super) fn table(&mut self, n: Node<'a>) -> Result<(), MarkupError> {
        let Some(t) = self.tables.pop() else {
            return Ok(());
        };
        let attrs = self.attrs_of(n).to_vec();
        let ctx = TableCtx {
            thead: t.head,
            tbody: t.body,
            attributes: attributes::to_map(&attrs),
        };
        if let Some(html) = self.call(HookKind::Table, n, |h, env| h.table(env, &ctx))? {
            self.out.push_str(&html);
            return Ok(());
        }
        // Go's embedded table template: `range $k, $v := .Attributes` (key order), falsy
        // values skipped, `printf " %s=%q" $k ($v | transform.HTMLEscape)`.
        self.out.push_str("<table");
        for (k, v) in ctx.attributes.iter() {
            let falsy = match v {
                ssg_base::Value::Null => true,
                ssg_base::Value::Bool(b) => !b,
                ssg_base::Value::Int(i) => *i == 0,
                ssg_base::Value::Float(f) => *f == 0.0,
                ssg_base::Value::String(s) => s.is_empty(),
                _ => false,
            };
            if falsy {
                continue;
            }
            self.out.push_str(&format!(" {k}=\""));
            let mut text = String::new();
            escape::html(&mut text, &escape::value_text(v));
            // Go's html.EscapeString also escapes `'`, and writes `"` as `&#34;`.
            self.out
                .push_str(&text.replace('\'', "&#39;").replace("&quot;", "&#34;"));
            self.out.push('"');
        }
        self.out.push_str(">\n  <thead>");
        write_rows(&mut self.out, &ctx.thead, "th");
        self.out.push_str("\n  </thead>\n  <tbody>");
        write_rows(&mut self.out, &ctx.tbody, "td");
        self.out.push_str("\n  </tbody>\n</table>\n");
        Ok(())
    }

    pub(super) fn code_block(
        &mut self,
        n: Node<'a>,
        cb: &comrak::nodes::NodeCodeBlock,
    ) -> Result<(), MarkupError> {
        if !cb.fenced {
            self.out.push_str("<pre><code>");
            escape::html(&mut self.out, &cb.literal);
            self.out.push_str("</code></pre>\n");
            return Ok(());
        }
        let info = cb.info.trim();
        let word = info.split(' ').next().unwrap_or("");
        if self.o.code_fences == CodeFences::Plain || self.hooks.is_none() {
            self.plain_code(word, &cb.literal);
            return Ok(());
        }
        let lang = word.split('{').next().unwrap_or("").to_owned();
        let (attrs, options) =
            fence_attributes(info).map_err(|message| MarkupError::Attributes {
                position: self.doc.position(n),
                message,
            })?;
        let inner = cb.literal.trim_end_matches(['\n', '\r']).to_owned();
        let ctx = CodeBlockCtx {
            lang: lang.clone(),
            inner: inner.clone(),
            options: attributes::to_map(&options),
            attributes: attributes::to_map(&attrs),
        };
        // `call` numbers the code block (hooks are set here).
        let ordinal = self.ordinals[HookKind::CodeBlock as usize];
        if let Some(html) = self.call(HookKind::CodeBlock, n, |h, env| h.code_block(env, &ctx))? {
            self.out.push_str(&html);
            return Ok(());
        }
        if let Some(hl) = self.hl {
            let o = HighlightOptions {
                options: ctx.options,
                attributes: ctx.attributes,
                ordinal,
            };
            let html = hl
                .highlight(&inner, &lang, &o)
                .map_err(|e| self.hook_error(HookKind::CodeBlock, n, e))?;
            self.out.push_str(&html);
            return Ok(());
        }
        self.plain_code(&lang, &cb.literal);
        Ok(())
    }

    /// goldmark's fenced code: `<pre><code class="language-x">`.
    pub(super) fn plain_code(&mut self, lang: &str, code: &str) {
        self.out.push_str("<pre><code");
        if !lang.is_empty() {
            self.out.push_str(" class=\"language-");
            escape::html(&mut self.out, lang);
            self.out.push('"');
        }
        self.out.push('>');
        escape::html(&mut self.out, code);
        self.out.push_str("</code></pre>\n");
    }

    pub(super) fn raw(&mut self, n: Node<'a>, literal: &str) -> Result<(), MarkupError> {
        match self.doc.role(n).cloned() {
            Some(Role::Passthrough { kind, inner, raw }) => {
                let ctx = PassthroughCtx {
                    kind,
                    inner,
                    attributes: Map::new(),
                };
                let hooked =
                    self.call(HookKind::Passthrough, n, |h, env| h.passthrough(env, &ctx))?;
                let block_level = n
                    .parent()
                    .is_some_and(|p| !p.data().value.contains_inlines());
                // Go (`markup/goldmark/passthrough`, `renderPassthroughBlock`) writes a hook's
                // output as it is: the next block follows without a newline. Without a hook
                // the source of a block ends its line.
                let newline = kind == PassthroughKind::Block && block_level && hooked.is_none();
                self.out.push_str(&hooked.unwrap_or(raw));
                if newline {
                    self.out.push('\n');
                }
            }
            _ => self.out.push_str(literal),
        }
        Ok(())
    }

    pub(super) fn footnotes(&mut self) -> Result<(), MarkupError> {
        let mut index: HashMap<String, u32> = HashMap::new();
        for d in self.doc.root.descendants() {
            if let NodeValue::FootnoteReference(f) = &d.data().value {
                index.entry(f.name.clone()).or_insert(f.ix);
            }
        }
        let mut defs: Vec<(u32, u32, Node<'a>)> = self
            .doc
            .root
            .descendants()
            .filter_map(|d| match &d.data().value {
                NodeValue::FootnoteDefinition(f) => {
                    index.get(&f.name).map(|&ix| (ix, f.total_references, d))
                }
                _ => None,
            })
            .collect();
        if defs.is_empty() {
            return Ok(());
        }
        defs.sort_by_key(|(ix, ..)| *ix);
        self.out
            .push_str("<div class=\"footnotes\" role=\"doc-endnotes\">\n<hr");
        self.out.push_str(self.void());
        self.out.push_str("\n<ol>\n");
        let backlink = self
            .o
            .footnote_backlink
            .clone()
            .unwrap_or_else(|| "&#x21a9;&#xfe0e;".to_owned());
        for (ix, refs, def) in defs {
            self.out.push_str(&format!("<li id=\"fn:{ix}\">\n"));
            let start = self.out.len();
            for c in def.children() {
                self.walk(c)?;
            }
            let mut links = String::new();
            for k in 0..refs.max(1) {
                let k = if k == 0 { String::new() } else { k.to_string() };
                links.push_str(&format!(
                    "&#160;<a href=\"#fnref{k}:{ix}\" class=\"footnote-backref\" role=\"doc-backlink\">{backlink}</a>"
                ));
            }
            if self.out[start..].ends_with("</p>\n") {
                let at = self.out.len() - "</p>\n".len();
                self.out.insert_str(at, &links);
            } else {
                self.out.push_str(&links);
            }
            self.out.push_str("</li>\n");
        }
        self.out.push_str("</ol>\n</div>\n");
        Ok(())
    }
}

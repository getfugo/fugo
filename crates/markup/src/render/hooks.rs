//! The nodes render hooks may replace: headings, links and images, blockquotes (and alerts).

use super::*;

impl<'r, 'a> Renderer<'r, 'a> {
    pub(super) fn call(
        &mut self,
        kind: HookKind,
        n: Node<'_>,
        f: impl FnOnce(&dyn Hooks, &HookEnv) -> Result<HookOut, HookError>,
    ) -> Result<Option<String>, MarkupError> {
        let Some(h) = self.hooks else {
            return Ok(None);
        };
        let env = self.env(kind, n);
        match f(h, &env) {
            Ok(HookOut::Html(s)) => Ok(Some(s)),
            Ok(HookOut::Default) => Ok(None),
            Err(e) => Err(self.hook_error(kind, n, e)),
        }
    }

    pub(super) fn heading(
        &mut self,
        n: Node<'a>,
        level: u8,
        mark: usize,
    ) -> Result<(), MarkupError> {
        let text = self.out.split_off(mark);
        let extra = self.doc.extra(n);
        let id = extra.and_then(|e| e.id.clone());
        let attrs = extra.map(|e| e.attrs.clone()).unwrap_or_default();
        if self.hooks.is_some() {
            let mut map = attributes::to_map(&attrs);
            if let Some(id) = &id {
                map.insert("id", ssg_base::Value::string(id));
            }
            let ctx = HeadingCtx {
                level,
                anchor: id.clone().unwrap_or_default(),
                text: text.clone(),
                plain_text: text_plain(self.doc, n),
                attributes: map,
            };
            if let Some(html) = self.call(HookKind::Heading, n, |h, env| h.heading(env, &ctx))? {
                self.out.push_str(&html);
                return Ok(());
            }
        }
        self.out.push_str(&format!("<h{level}"));
        if let Some(id) = &id {
            self.out.push_str(" id=\"");
            escape::html(&mut self.out, id);
            self.out.push('"');
        }
        escape::all_attrs(&mut self.out, &attrs);
        self.out.push('>');
        self.out.push_str(&text);
        self.out.push_str(&format!("</h{level}>\n"));
        Ok(())
    }

    pub(super) fn link(
        &mut self,
        n: Node<'a>,
        url: &str,
        title: &str,
        mark: usize,
        image: bool,
    ) -> Result<(), MarkupError> {
        let inner = self.out.split_off(mark);
        let auto = match self.doc.role(n) {
            Some(Role::AutoLink { www }) => Some(*www),
            _ => None,
        };
        let url = match auto {
            Some(true) => format!("{}://{url}", self.o.linkify_protocol.as_str()),
            _ => url.to_owned(),
        };
        if self.hooks.is_some() {
            let (text, plain_text) = if auto.is_some() {
                let label: String = n.descendants().filter_map(crate::doc::text_of).collect();
                (label.clone(), label)
            } else {
                (inner.clone(), text_plain(self.doc, n))
            };
            // Hooks see the destination and title as written.
            let (destination, raw_title) = match auto {
                Some(_) => (url.clone(), title.to_owned()),
                None => self
                    .doc
                    .raw_link(n, image)
                    .unwrap_or_else(|| (url.clone(), title.to_owned())),
            };
            let ctx = LinkCtx {
                destination,
                title: raw_title,
                text,
                plain_text,
                is_block: matches!(self.doc.role(n), Some(Role::BlockImage)),
                attributes: attributes::to_map(self.attrs_of(n)),
            };
            let out = if image {
                self.call(HookKind::Image, n, |h, env| h.image(env, &ctx))?
            } else {
                self.call(HookKind::Link, n, |h, env| h.link(env, &ctx))?
            };
            if let Some(html) = out {
                self.out.push_str(&html);
                return Ok(());
            }
        }
        if image {
            self.out.push_str("<img src=\"");
            self.href(&url);
            self.out.push_str("\" alt=\"");
            self.out.push_str(&crate::text::strip_html(&inner));
            self.out.push('"');
            if !title.is_empty() {
                self.out.push_str(" title=\"");
                escape::html(&mut self.out, title);
                self.out.push('"');
            }
            let a = self.attrs_of(n);
            escape::attrs(
                &mut self.out,
                a,
                &["align", "height", "width", "loading", "decoding"],
            );
            self.out.push_str(self.void());
        } else {
            self.out.push_str("<a href=\"");
            self.href(&url);
            self.out.push('"');
            if !title.is_empty() {
                self.out.push_str(" title=\"");
                escape::html(&mut self.out, title);
                self.out.push('"');
            }
            self.out.push('>');
            self.out.push_str(&inner);
            self.out.push_str("</a>");
        }
        Ok(())
    }

    pub(super) fn blockquote(&mut self, n: Node<'a>, mark: usize) -> Result<(), MarkupError> {
        let captured = self.out.split_off(mark);
        let text = captured.trim().to_owned();
        let attrs = self.attrs_of(n).to_vec();
        if self.hooks.is_some() {
            let alert = self
                .o
                .extensions
                .contains(Extensions::ALERTS)
                .then(|| ALERT.captures(&text))
                .flatten()
                .map(|m| {
                    let title = m.get(3).map_or("", |t| t.as_str()).trim();
                    let sign = match m.get(2).map(|s| s.as_str()) {
                        Some("+") => AlertSign::Plus,
                        Some("-") => AlertSign::Minus,
                        _ => AlertSign::None,
                    };
                    // The content without the `[!TYPE]` line; when that line closed its
                    // paragraph the rest starts with the next block.
                    let rest = &text[m.get(0).map_or(0, |m| m.end())..];
                    let (title, rest) = match title.strip_suffix("</p>") {
                        Some(t) => (t, rest.to_owned()),
                        None => (title, format!("<p>{rest}")),
                    };
                    (m[1].to_lowercase(), title.trim().to_owned(), sign, rest)
                });
            let ctx = match alert {
                Some((alert_type, alert_title, alert_sign, rest)) => BlockquoteCtx {
                    kind: BlockquoteKind::Alert,
                    alert_type,
                    alert_title,
                    alert_sign,
                    text: rest,
                    attributes: attributes::to_map(&attrs),
                },
                None => BlockquoteCtx {
                    kind: BlockquoteKind::Regular,
                    alert_type: String::new(),
                    alert_title: String::new(),
                    alert_sign: AlertSign::None,
                    text: text.clone(),
                    attributes: attributes::to_map(&attrs),
                },
            };
            if let Some(html) =
                self.call(HookKind::Blockquote, n, |h, env| h.blockquote(env, &ctx))?
            {
                self.out.push_str(&html);
                return Ok(());
            }
        }
        if attrs.is_empty() {
            self.out.push_str("<blockquote>\n");
        } else {
            self.out.push_str("<blockquote");
            escape::attrs(&mut self.out, &attrs, &["cite"]);
            self.out.push('>');
        }
        self.out.push_str(&text);
        self.out.push_str("</blockquote>\n");
        Ok(())
    }
}

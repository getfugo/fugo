//! Where a page's text is in its published HTML. A build with the editor marks, in each page,
//! the element that holds the page's text (`page.content`) with `data-cms-body`, so that the
//! editor's preview puts the text being edited where the layout put it, whatever the layout
//! and its render hooks add around it:
//!
//! - `data-cms-body` (no value): the element holds the text alone (whitespace aside);
//! - `data-cms-body="1..3"`: the text is the element's children 1 and 2 (counted from 0,
//!   elements only), among others the layout puts there (a title before it, a footer after).
//!
//! The text is found as the layout printed it, byte for byte, where an element can hold it
//! (not in an attribute, a comment, a script or a `<template>`). A page gets no mark when the
//! layout changed the text before printing it (a filter), when the text's HTML does not close
//! what it opens, when the text is not elements alone and shares its element with others, or
//! when the page marks an element itself; the editor then finds the text by what it says.

mod tree;

use tree::{Tree, ends_name};

/// The attribute that marks the element of a page's text.
pub const BODY_ATTR: &str = "data-cms-body";

/// `html`, a page as its layout printed it, with the element that holds `content` (the page's
/// text, `page.content`) marked [`BODY_ATTR`]; `None` when there is nothing to mark (see the
/// module's documentation).
#[must_use]
pub fn mark_body(html: &str, content: &str) -> Option<String> {
    let text = content.trim();
    if text.is_empty() || (html.contains(BODY_ATTR) && Tree::new(html).marked()) {
        return None;
    }
    let mut tree = Tree::new(html);
    for (at, _) in html.match_indices(text) {
        if !tree.advance_to(at) || tree.in_template() {
            continue;
        }
        let Some(parent) = tree.parent() else {
            continue;
        };
        let inner = Inner::of(text)?;
        let alone = html[parent.tag_end..at].trim().is_empty()
            && closes(&html[at + text.len()..], parent.name);
        let mark = if alone {
            BODY_ATTR.to_owned()
        } else if inner.elements > 0 && !inner.text {
            let from = parent.children;
            format!("{BODY_ATTR}=\"{from}..{}\"", from + inner.elements)
        } else {
            return None;
        };
        let mut out = String::with_capacity(html.len() + mark.len() + 1);
        out.push_str(&html[..parent.attr_at]);
        out.push(' ');
        out.push_str(&mark);
        out.push_str(&html[parent.attr_at..]);
        return Some(out);
    }
    None
}

/// Whether `rest` starts, after whitespace, with the end tag of `name`.
fn closes(rest: &str, name: &str) -> bool {
    let r = rest.trim_start().as_bytes();
    let n = name.len();
    r.starts_with(b"</")
        && r.get(2..2 + n)
            .is_some_and(|b| b.eq_ignore_ascii_case(name.as_bytes()))
        && r.get(2 + n).is_none_or(|&c| ends_name(c))
}

/// What the page's text is at its top level.
struct Inner {
    /// Its elements.
    elements: usize,
    /// Whether it has text outside them (more than whitespace).
    text: bool,
}

impl Inner {
    /// `None` when the text does not close what it opens, or closes what it did not open.
    fn of(text: &str) -> Option<Self> {
        let mut tree = Tree::new(text);
        tree.advance_to(text.len());
        let root = tree.stack.first()?;
        (tree.stack.len() == 1 && !tree.stray_end).then_some(Self {
            elements: root.children,
            text: root.text,
        })
    }
}

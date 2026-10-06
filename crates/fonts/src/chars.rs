//! The characters of a site's pages: their text (character references decoded) outside
//! `<script>` and `<style>`, and their `alt`, `placeholder`, `title` and `value` attributes,
//! which are drawn with the page's fonts too. The characters a site's CSS draws are in
//! [`crate::css`].

use std::collections::BTreeSet;

use html5gum::{State, Token, Tokenizer};

/// Adds the characters of the text of the HTML page `html`: its text outside `<script>` and
/// `<style>`, and its `alt`, `placeholder`, `title` and `value` attributes.
pub fn add_html_text(html: &str, out: &mut BTreeSet<char>) {
    let mut tokenizer = Tokenizer::new(html);
    let mut skipping = false;
    while let Some(Ok(token)) = tokenizer.next() {
        match token {
            Token::StartTag(tag) => {
                for (key, value) in &tag.attributes {
                    if matches!(&key[..], b"alt" | b"placeholder" | b"title" | b"value") {
                        add_text(&value.value, out);
                    }
                }
                // The tokenizer reads a script's or style's content as text up to its end tag.
                let state = match &tag.name[..] {
                    b"script" => Some(State::ScriptData),
                    b"style" => Some(State::RawText),
                    _ => None,
                };
                if let Some(state) = state.filter(|_| !tag.self_closing) {
                    tokenizer.set_state(state);
                    skipping = true;
                }
            }
            Token::EndTag(tag) if matches!(&tag.name[..], b"script" | b"style") => {
                skipping = false;
            }
            Token::String(s) if !skipping => add_text(&s.value, out),
            _ => {}
        }
    }
}

/// Adds the characters of `text` but control characters (line breaks, tabs).
fn add_text(text: &[u8], out: &mut BTreeSet<char>) {
    out.extend(
        String::from_utf8_lossy(text)
            .chars()
            .filter(|c| !c.is_control()),
    );
}

//! The `es5` target of `js.Build`, which Rolldown cannot build (it starts at ES2015).
//!
//! This port bundles with Rolldown at `es2015` and adds two steps around it:
//!
//! - [`check_es5`], for every module as loaded (before Rolldown's own transform): the errors
//!   esbuild 0.25 reports for `--target=es5`, with its texts and positions. esbuild lowers some
//!   ES2015+ syntax to ES5 (arrow functions, template literals, shorthand properties, object
//!   spread, `**`, `?.`, `??`, logical assignment, optional catch bindings, regular expression
//!   flags, ...) and rejects the rest (`let`/`const`, classes, destructuring, default and rest
//!   parameters, spread arguments, generators, async functions, for-of, computed keys and
//!   methods, ...); this check rejects exactly what esbuild rejects.
//! - [`lower_to_es5`], on the bundle: lowers what is left of ES2015 in Rolldown's output (its
//!   runtime helpers and wrappers, the syntax that passed the check and that oxc keeps at
//!   `es2015`, and what its minifier and code generator emit) to ES5, then verifies that no
//!   ES2015+ syntax remains.
//!
//! The ESM output format keeps its `import`/`export` statements, as esbuild does for `es5`.

mod check;
mod lower;
mod print;
mod verify;

use oxc::span::SourceType;

use super::{LowerError, Lowered};

/// esbuild's `es5` errors for one module (JS, JSX, TS or TSX source as loaded, before TS
/// stripping), sorted by position as esbuild sorts them, with esbuild's texts and positions.
/// Empty when esbuild can build the module for `es5`.
///
/// A module that does not parse gets no errors: the bundler reports its syntax errors.
/// esbuild's es5 warnings (BigInt literals, `import.meta`) are not errors and are not reported.
#[must_use]
pub fn check_es5(source: &str, source_type: SourceType, filename: &str) -> Vec<LowerError> {
    let _ = filename; // positions are relative to `source`; the caller names the file
    let mut errors: Vec<LowerError> = check::check(source, source_type)
        .into_iter()
        .map(|(offset, message)| at(source, offset, message))
        .collect();
    // esbuild sorts its messages by position, then by text (stable).
    errors.sort_by(|a, b| (a.line, a.column, &a.message).cmp(&(b.line, b.column, &b.message)));
    errors
}

/// Turns a bundle that Rolldown produced at target `es2015` (possibly minified) into ES5.
/// `map` is from `code` to the result.
///
/// # Errors
///
/// When `code` does not parse, or uses ES2015+ syntax that has no ES5 lowering here (the error
/// names it, at its position in `code`): esbuild would have rejected it in [`check_es5`], so it
/// only reaches this function from code neither esbuild nor this check vetted.
pub fn lower_to_es5(code: &str, minify: bool, sourcemap: bool) -> Result<Lowered, LowerError> {
    lower::lower(code, minify, sourcemap).map_err(|(offset, message)| at(code, offset, message))
}

/// `message` at byte `offset` of `source`, with esbuild's line counting: `\n`, `\r\n`, `\r`,
/// U+2028 and U+2029 end lines (also inside strings and comments); columns count bytes.
fn at(source: &str, offset: u32, message: String) -> LowerError {
    let offset = (offset as usize).min(source.len());
    let bytes = source.as_bytes();
    let mut line = 1u32;
    let mut line_start = 0usize;
    let mut i = 0usize;
    while i < offset {
        match bytes[i] {
            b'\n' => {
                line += 1;
                line_start = i + 1;
            }
            b'\r' => {
                if bytes.get(i + 1) == Some(&b'\n') && i + 1 < offset {
                    i += 1;
                }
                line += 1;
                line_start = i + 1;
            }
            // U+2028 / U+2029 are E2 80 A8 / E2 80 A9.
            0xE2 if bytes.get(i + 1) == Some(&0x80)
                && matches!(bytes.get(i + 2), Some(0xA8 | 0xA9))
                && i + 2 < offset =>
            {
                i += 2;
                line += 1;
                line_start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    LowerError {
        message,
        line,
        column: u32::try_from(offset - line_start).unwrap_or(u32::MAX),
    }
}

/// Scanning of source text between AST nodes (keywords and punctuators have no node).
pub(crate) struct Scan<'s> {
    pub(crate) src: &'s str,
}

impl<'s> Scan<'s> {
    pub(crate) fn new(src: &'s str) -> Self {
        Self { src }
    }

    fn bytes(&self) -> &'s [u8] {
        self.src.as_bytes()
    }

    /// The position after whitespace, line terminators and comments from `pos`.
    pub(crate) fn skip_trivia(&self, mut pos: u32) -> u32 {
        let b = self.bytes();
        loop {
            let i = pos as usize;
            let Some(&c) = b.get(i) else { return pos };
            match c {
                b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C => pos += 1,
                b'/' if b.get(i + 1) == Some(&b'/') => {
                    let rest = &self.src[i..];
                    let end = rest
                        .find(['\n', '\r', '\u{2028}', '\u{2029}'])
                        .unwrap_or(rest.len());
                    pos += u32::try_from(end).unwrap_or(0);
                }
                b'/' if b.get(i + 1) == Some(&b'*') => {
                    let end = self.src[i + 2..]
                        .find("*/")
                        .map_or(self.src.len(), |e| i + 2 + e + 2);
                    pos = u32::try_from(end).unwrap_or(pos + 2);
                }
                c if c >= 0x80 => {
                    let ch = self.src[i..].chars().next().unwrap_or('\0');
                    if ch.is_whitespace() || ch == '\u{FEFF}' {
                        pos += u32::try_from(ch.len_utf8()).unwrap_or(1);
                    } else {
                        return pos;
                    }
                }
                _ => return pos,
            }
        }
    }

    /// The identifier-like word at `pos` (ASCII letters, digits, `_`, `$`).
    pub(crate) fn word_at(&self, pos: u32) -> &'s str {
        let rest = self.src.get(pos as usize..).unwrap_or("");
        let end = rest
            .bytes()
            .position(|c| !(c.is_ascii_alphanumeric() || c == b'_' || c == b'$'))
            .unwrap_or(rest.len());
        &rest[..end]
    }

    /// Whether `tok` starts at `pos`.
    pub(crate) fn starts(&self, pos: u32, tok: &str) -> bool {
        self.src
            .get(pos as usize..)
            .is_some_and(|r| r.starts_with(tok))
    }

    /// The position of the next token after `pos` (trivia skipped) when it is `tok`.
    pub(crate) fn token(&self, pos: u32, tok: &str) -> Option<u32> {
        let p = self.skip_trivia(pos);
        self.starts(p, tok).then_some(p)
    }

    /// From `pos`, skips trivia and any of `words`; the position of the next token.
    pub(crate) fn skip_words(&self, mut pos: u32, words: &[&str]) -> u32 {
        loop {
            pos = self.skip_trivia(pos);
            let w = self.word_at(pos);
            if !w.is_empty() && words.contains(&w) {
                pos += u32::try_from(w.len()).unwrap_or(0);
            } else {
                return pos;
            }
        }
    }

    /// The first token from `pos` that is `tok`, skipping trivia, identifier-like words and
    /// `*` (modifiers such as `static`, `async`, `get`).
    pub(crate) fn find_after_modifiers(&self, mut pos: u32, tok: &str) -> Option<u32> {
        for _ in 0..16 {
            pos = self.skip_trivia(pos);
            if self.starts(pos, tok) {
                return Some(pos);
            }
            let w = self.word_at(pos);
            if !w.is_empty() {
                pos += u32::try_from(w.len()).unwrap_or(0);
            } else if self.starts(pos, "*") {
                pos += 1;
            } else {
                return None;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests;

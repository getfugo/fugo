//! Content files: front matter split and decode, the summary divider, and the shortcode lexer
//! (`lex` + `assemble`). See `README.md` for the oracle results and accepted deviations.
//!
//! A content file is first split ([`split_front_matter`]) and its front matter decoded
//! ([`decode_front_matter`]); the body is lexed ([`lex`]) into typed [`Token`]s with byte
//! ranges and assembled ([`assemble`]) into text and [`ShortcodeCall`]s. Assembling needs to
//! know which shortcodes use their inner content ([`InnerOracle`]), because the Go
//! implementation's syntax depends on it.

#![forbid(unsafe_code)]

mod assemble;
mod front_matter;
mod lexer;
mod token;

pub use assemble::{
    Body, Closing, InnerOracle, InnerUse, ParseError, Segment, ShortcodeArgs, ShortcodeCall,
    assemble, parse_body,
};
pub use front_matter::{
    DecodeError, Split, decode_front_matter, decode_front_matter_map, split_front_matter,
};
pub use lexer::{LexError, LexErrorKind, LexOptions, Lexed, Start, SummaryDivider, lex, lex_with};
pub use token::{Delim, FrontMatterFormat, Quoting, Scalar, Token, TokenKind};

/// The 1-based line and column (in characters) of byte `offset` in `src`, for diagnostics.
#[must_use]
pub fn line_col(src: &str, offset: usize) -> (u32, u32) {
    let before = &src[..offset.min(src.len())];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line = before.matches('\n').count() + 1;
    let col = before[line_start..].chars().count() + 1;
    (
        u32::try_from(line).unwrap_or(u32::MAX),
        u32::try_from(col).unwrap_or(u32::MAX),
    )
}

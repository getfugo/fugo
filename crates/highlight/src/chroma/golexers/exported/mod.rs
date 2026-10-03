//! Chroma's lexers, converted from its XML (crate README, "Lexer and style files"), and
//! [`LEXERS`], in Chroma's order (its file names, bytewise). Written by
//! `tests/it/xml2rust.rs` with the files.

use crate::chroma::defs::LexerDef;

mod caddyfile;
mod caddyfile_directives;
mod genshi;
mod genshi_html;
mod genshi_text;
mod go;
mod haxe;
mod http;
mod markdown;
mod phtml;
mod raku;
mod restructuredtext;
mod svelte;
mod typoscript;

/// Every one of this directory, in Chroma's order.
#[rustfmt::skip]
pub(crate) static LEXERS: &[&LexerDef] = &[
    &caddyfile::LEXER,
    &caddyfile_directives::LEXER,
    &genshi::LEXER,
    &genshi_html::LEXER,
    &genshi_text::LEXER,
    &go::LEXER,
    &haxe::LEXER,
    &http::LEXER,
    &markdown::LEXER,
    &phtml::LEXER,
    &raku::LEXER,
    &restructuredtext::LEXER,
    &svelte::LEXER,
    &typoscript::LEXER,
];

//! Chroma's lexers written in Go (`lexers/*.go`, Chroma v2.19.0, MIT), and the registry in
//! Chroma's order.
//!
//! Their configuration and rules are in `golexers/exported/`, exported from Chroma's Go code in
//! Chroma's XML format (the exporter walks each lexer's `Rules()`) and converted to Rust (README
//! "Lexer and style files", "Fixtures"); the Go functions they call are ported here: the Go lexer's template strings, HTTP's header and body emitters and its
//! content-type sub-lexer, reStructuredText's code blocks, Haxe's pre-processor mutator; the
//! wrappers are the delegating lexers (Go HTML templates, Markdown, PHTML and Svelte in HTML)
//! and the Common Lisp and Emacs Lisp word remapping; the analysers of Go, DNS, MySQL and Zed.
//! Raku's lexer is mostly Go code (rule-rewriting mutators) and is not ported: Raku is
//! registered with plain-text rules (crate README, deviations).

pub(super) mod exported;
mod lisp;

use std::sync::Arc;

use super::regex_lexer::{EmitFn, MutateFn};
use super::{
    Config, DelegatingLexer, Lexer, LexerState, RegexLexer, Registry, Token, TokeniseOptions,
    TypeRemappingLexer, defs, until_eof,
};
use crate::token::TokenType as T;

/// The lexer of Chroma's `lexers/embedded/<file>.xml`.
fn embedded(file: &str) -> RegexLexer {
    let def = defs::lexer(file).unwrap_or_else(|| panic!("no lexer {file}"));
    RegexLexer::from_table(def)
}

/// The exported Go lexer `<file>` (`golexers/exported/`).
fn go(file: &str) -> RegexLexer {
    let def = go_lexer(file).unwrap_or_else(|| panic!("no Go lexer {file}"));
    RegexLexer::from_table(def)
}

/// The exported Go lexer `<file>`.
pub(crate) fn go_lexer(file: &str) -> Option<&'static defs::LexerDef> {
    exported::LEXERS.iter().copied().find(|l| l.file == file)
}

/// The emitter functions the exported lexers name (`emitfunc = "…"`).
pub(crate) fn emitter_func(name: &str) -> Option<(&'static str, EmitFn)> {
    let f: (&'static str, EmitFn) = match name {
        "goTextTemplateString" => ("goTextTemplateString", go_text_template_string),
        "httpContentBlock" => ("httpContentBlock", http_content_block),
        "httpHeaderBlock" => ("httpHeaderBlock", http_header_block),
        "httpContinuousHeaderBlock" => ("httpContinuousHeaderBlock", http_continuous_header_block),
        "rstCodeBlock" => ("rstCodeBlock", rst_code_block),
        _ => return None,
    };
    Some(f)
}

/// The mutator functions the exported lexers name (`mutatorfunc = "…"`).
pub(crate) fn mutator_func(name: &str) -> Option<(&'static str, MutateFn)> {
    match name {
        "haxePreProcMutator" => Some(("haxePreProcMutator", haxe_pre_proc_mutator)),
        _ => None,
    }
}

/// Every lexer, registered in Chroma's order: its XML lexers (`lexers/`, bytewise file name
/// order), then the Go lexers in Go's initialisation order (a Go lexer named like an XML one
/// replaces it in place: Common Lisp, Emacs Lisp, TypoScript).
pub(crate) fn registry() -> Registry {
    let mut reg = Registry::default();
    for def in super::lexers::LEXERS {
        let lexer = RegexLexer::from_table(def);
        let lexer = match def.file {
            "dns" => lexer.with_analyser(dns_analyser),
            "mysql" => lexer.with_analyser(mysql_analyser),
            "zed" => lexer.with_analyser(zed_analyser),
            _ => lexer,
        };
        reg.register(Arc::new(lexer));
    }
    // Chroma's `HTML` variable: its own instance of the HTML lexer, the root of the
    // delegating lexers.
    let html: Arc<dyn Lexer> = Arc::new(embedded("html"));
    let delegating = |language: RegexLexer| -> Arc<dyn Lexer> {
        Arc::new(DelegatingLexer {
            root: Arc::clone(&html),
            language: Arc::new(language),
        })
    };

    reg.register(Arc::new(go("caddyfile")));
    reg.register(Arc::new(go("caddyfile_directives")));
    reg.register(Arc::new(TypeRemappingLexer::new(
        Arc::new(embedded("common_lisp")),
        &[
            (T::NameVariable, T::NameFunction, lisp::CL_BUILTIN_FUNCTIONS),
            (T::NameVariable, T::Keyword, lisp::CL_SPECIAL_FORMS),
            (T::NameVariable, T::NameBuiltin, lisp::CL_MACROS),
            (T::NameVariable, T::Keyword, lisp::CL_LAMBDA_LIST_KEYWORDS),
            (T::NameVariable, T::Keyword, lisp::CL_DECLARATIONS),
            (T::NameVariable, T::KeywordType, lisp::CL_BUILTIN_TYPES),
            (T::NameVariable, T::NameClass, lisp::CL_BUILTIN_CLASSES),
        ],
    )));
    reg.register(Arc::new(TypeRemappingLexer::new(
        Arc::new(embedded("emacslisp")),
        &[
            (
                T::NameVariable,
                T::NameFunction,
                lisp::EMACS_BUILTIN_FUNCTION,
            ),
            (T::NameVariable, T::NameBuiltin, lisp::EMACS_SPECIAL_FORMS),
            (
                T::NameVariable,
                T::NameException,
                lisp::EMACS_ERROR_KEYWORDS,
            ),
            (
                T::NameVariable,
                T::NameBuiltin,
                lisp::EMACS_BUILTIN_FUNCTION_HIGHLIGHTED,
            ),
            (T::NameVariable, T::NameBuiltin, lisp::EMACS_MACROS),
            (
                T::NameVariable,
                T::KeywordPseudo,
                lisp::EMACS_LAMBDA_LIST_KEYWORDS,
            ),
        ],
    )));
    reg.register(Arc::new(go("genshi_text")));
    reg.register(Arc::new(go("genshi_html")));
    reg.register(Arc::new(go("genshi")));
    reg.register(delegating(embedded("go_template").with_config(Config {
        name: "Go HTML Template".to_owned(),
        aliases: vec!["go-html-template".to_owned()],
        ..Config::default()
    })));
    reg.register(Arc::new(embedded("go_template").with_config(Config {
        name: "Go Text Template".to_owned(),
        aliases: vec!["go-text-template".to_owned()],
        ..Config::default()
    })));
    reg.register(Arc::new(go("go").with_analyser(go_analyser)));
    reg.register(Arc::new(go("haxe")));
    reg.register(Arc::new(HttpBodyContentTyper(Arc::new(go("http")))));
    reg.register(delegating(go("markdown")));
    reg.register(delegating(go("phtml").with_analyser(phtml_analyser)));
    reg.register(Arc::new(go("raku")));
    reg.register(Arc::new(go("restructuredtext")));
    reg.register(delegating(go("svelte")));
    reg.register(Arc::new(go("typoscript")));
    reg
}

// ── analysers ──

/// `lexers/go.go`.
fn go_analyser(text: &str) -> f32 {
    if text.contains("fmt.") && text.contains("package ") {
        return 0.5;
    }
    if text.contains("package ") {
        return 0.1;
    }
    0.0
}

/// `lexers/php.go` (unreachable in Chroma: the delegating lexer asks HTML; kept for parity).
fn phtml_analyser(text: &str) -> f32 {
    if text.contains("<?php") { 0.5 } else { 0.0 }
}

/// `lexers/dns.go`: a zone file's `@ IN SOA` (Go's RE2 `(?m)^@\s+IN\s+SOA\s+`: `^` at the
/// start of the text or after `\n`, `\s` ASCII white space, which spans lines).
fn dns_analyser(text: &str) -> f32 {
    let b = text.as_bytes();
    let ws = |c: &u8| matches!(c, b' ' | b'\t' | b'\n' | 0x0c | b'\r');
    // `\s+` then `lit` from `i` (no backtracking needed: `lit` starts with no space).
    let spaces_then = |i: usize, lit: &[u8]| -> Option<usize> {
        let n = b[i..].iter().take_while(|&c| ws(c)).count();
        (n > 0 && b[i + n..].starts_with(lit)).then_some(i + n + lit.len())
    };
    let zone = |i: usize| -> bool {
        b[i..].starts_with(b"@")
            && spaces_then(i + 1, b"IN")
                .and_then(|j| spaces_then(j, b"SOA"))
                .is_some_and(|j| b.get(j).is_some_and(ws))
    };
    let found = (0..b.len()).any(|i| (i == 0 || b[i - 1] == b'\n') && zone(i));
    if found { 1.0 } else { 0.0 }
}

/// `lexers/mysql.go`: `` `name` `` against `[name]` (Go's RE2 `\w` is ASCII).
fn mysql_analyser(text: &str) -> f32 {
    let count = |open: char, close: char| -> usize {
        let b = text.as_bytes();
        let mut n = 0;
        let mut i = 0;
        while i < b.len() {
            if b[i] == open as u8
                && i + 1 < b.len()
                && (b[i + 1].is_ascii_alphabetic() || b[i + 1] == b'_')
            {
                let mut j = i + 2;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                if j < b.len() && b[j] == close as u8 {
                    n += 1;
                    i = j + 1;
                    continue;
                }
            }
            i += 1;
        }
        n
    };
    let backtick = count('`', '`');
    let bracket = count('[', ']');
    let mut result = 0.0f32;
    let dialect = backtick + bracket;
    if dialect >= 1 && backtick >= 2 * bracket {
        result += 0.5;
    } else if backtick > bracket {
        result += 0.2;
    } else if backtick > 0 {
        result += 0.1;
    }
    result
}

/// `lexers/zed.go`.
fn zed_analyser(text: &str) -> f32 {
    let (d, r, p) = (
        text.contains("definition "),
        text.contains("relation "),
        text.contains("permission "),
    );
    if d && r && p {
        0.9
    } else if d || r {
        0.5
    } else if p {
        0.25
    } else {
        0.0
    }
}

// ── emitters and mutators ──

/// The Go lexer's raw strings: `UsingLexer(TypeRemappingLexer(GoTextTemplate, Other →
/// LiteralString))`.
fn go_text_template_string(groups: &[String], st: &mut LexerState<'_>) -> Vec<Token> {
    let reg = st.registry;
    let Some(lexer) = reg.get("Go Text Template") else {
        return vec![Token::new(T::LiteralString, groups[0].clone())];
    };
    let tokens = lexer.tokenise(reg, Some(&TokeniseOptions::nested("root")), &groups[0]);
    until_eof(tokens)
        .map(|mut t| {
            if t.ty == T::Other {
                t.ty = T::LiteralString;
            }
            t
        })
        .collect()
}

/// `lexers/http.go`.
fn http_content_block(groups: &[String], _: &mut LexerState<'_>) -> Vec<Token> {
    vec![Token::new(T::Generic, groups[0].clone())]
}

fn http_header_block(groups: &[String], _: &mut LexerState<'_>) -> Vec<Token> {
    vec![
        Token::new(T::Name, groups[1].clone()),
        Token::new(T::Text, groups[2].clone()),
        Token::new(T::Operator, groups[3].clone()),
        Token::new(T::Text, groups[4].clone()),
        Token::new(T::Literal, groups[5].clone()),
        Token::new(T::Text, groups[6].clone()),
    ]
}

fn http_continuous_header_block(groups: &[String], _: &mut LexerState<'_>) -> Vec<Token> {
    vec![
        Token::new(T::Text, groups[1].clone()),
        Token::new(T::Literal, groups[2].clone()),
        Token::new(T::Text, groups[3].clone()),
    ]
}

/// `lexers/rst.go`: a `.. code-block:: lang` block, its code by that language's lexer.
fn rst_code_block(groups: &[String], st: &mut LexerState<'_>) -> Vec<Token> {
    let mut tokens = vec![
        Token::new(T::Punctuation, groups[1].clone()),
        Token::new(T::Text, groups[2].clone()),
        Token::new(T::OperatorWord, groups[3].clone()),
        Token::new(T::Punctuation, groups[4].clone()),
        Token::new(T::Text, groups[5].clone()),
        Token::new(T::Keyword, groups[6].clone()),
        Token::new(T::Text, groups[7].clone()),
    ];
    let code: String = groups[8..].concat();
    let reg = st.registry;
    match reg.get(&groups[6]) {
        None => tokens.push(Token::new(T::LiteralString, code)),
        Some(l) => tokens.extend(until_eof(l.tokenise(reg, None, &code))),
    }
    tokens
}

/// `lexers/haxe.go`: `#if`/`#else`/`#elseif`/`#end`/`#error` keep a stack of state stacks
/// (copies of a [`Stack`](super::stack::Stack) share its nodes).
fn haxe_pre_proc_mutator(st: &mut LexerState<'_>) {
    let proc = st.groups.get(2).cloned().unwrap_or_default();
    match proc.as_str() {
        "if" => st.preproc.push(st.stack.clone()),
        "else" | "elseif" => {
            if let Some(s) = st.preproc.last() {
                st.stack = s.clone();
            }
        }
        "end" => st.preproc.pop(),
        _ => {}
    }
    if proc == "if" || proc == "elseif" {
        st.stack.push("preproc-expr".to_owned());
    }
    if proc == "error" {
        st.stack.push("preproc-error".to_owned());
    }
}

/// `lexers/http.go`'s `httpBodyContentTyper`: the body of a message whose `Content-Type` has
/// a lexer is tokenised by it.
struct HttpBodyContentTyper(Arc<dyn Lexer>);

impl Lexer for HttpBodyContentTyper {
    fn config(&self) -> &Config {
        self.0.config()
    }

    fn analyse_text(&self, text: &str) -> f32 {
        self.0.analyse_text(text)
    }

    fn tokenise(&self, reg: &Registry, opts: Option<&TokeniseOptions>, text: &str) -> Vec<Token> {
        let tokens = self.0.tokenise(reg, opts, text);
        let mut out = Vec::with_capacity(tokens.len());
        let mut content_type = String::new();
        let mut is_content_type = false;
        let mut body: Option<Vec<Token>> = None;
        for mut token in tokens {
            if token.ty == T::Name && token.value.to_lowercase() == "content-type" {
                is_content_type = true;
            } else if token.ty == T::Literal && is_content_type {
                is_content_type = false;
                content_type = token.value.trim().to_owned();
                if let Some(pos) = content_type.find(';')
                    && pos > 0
                {
                    content_type = content_type[..pos].trim().to_owned();
                }
            } else if token.ty == T::Generic && !content_type.is_empty() {
                let mut lexer = reg.match_mime_type(&content_type);
                // application/calendar+xml can be treated as application/xml.
                if lexer.is_none() && content_type.contains('+') {
                    let slash = content_type.find('/').map_or(0, |i| i + 1);
                    let plus = content_type.rfind('+').map_or(0, |i| i + 1);
                    content_type = format!("{}{}", &content_type[..slash], &content_type[plus..]);
                    lexer = reg.match_mime_type(&content_type);
                }
                match lexer {
                    None => token.ty = T::Text,
                    Some(l) => {
                        // Chroma's iterator returns EOF here and the body's tokens after the
                        // message's: Go's `Coalesce` ends a run there and reads on; a
                        // consumer of a nested iterator stops ([`until_eof`]).
                        body = Some(l.tokenise(reg, None, &token.value));
                        out.push(Token::new(T::EOFType, ""));
                        continue;
                    }
                }
            }
            out.push(token);
        }
        out.extend(body.unwrap_or_default());
        out
    }
}

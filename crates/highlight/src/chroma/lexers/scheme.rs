//! Chroma's `scheme.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "scheme",
    config: ConfigDef {
        name: "Scheme",
        aliases: &["scheme", "scm"],
        filenames: &["*.scm", "*.ss"],
        mime_types: &["text/x-scheme", "application/x-scheme"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r";.*$").token(T::CommentSingle),
            rule(r"#\|").token(T::CommentMultiline).push(&["multiline-comment"]),
            rule(r"#;\s*\(").token(T::Comment).push(&["commented-form"]),
            rule(r"#!r6rs").token(T::Comment),
            rule(r"\s+").token(T::Text),
            rule(r"-?\d+\.\d+").token(T::LiteralNumberFloat),
            rule(r"-?\d+").token(T::LiteralNumberInteger),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            rule(r"'[\w!$%&*+,/:<=>?@^~|-]+").token(T::LiteralStringSymbol),
            rule(r"#\\(alarm|backspace|delete|esc|linefeed|newline|page|return|space|tab|vtab|x[0-9a-zA-Z]{1,5}|.)").token(T::LiteralStringChar),
            rule(r"(#t|#f)").token(T::NameConstant),
            rule(r"('|#|`|,@|,|\.)").token(T::Operator),
            rule(r"(lambda |define |if |else |cond |and |or |case |let |let\* |letrec |begin |do |delay |set\! |\=\> |quote |quasiquote |unquote |unquote\-splicing |define\-syntax |let\-syntax |letrec\-syntax |syntax\-rules )").token(T::Keyword),
            rule(r"(?<='\()[\w!$%&*+,/:<=>?@^~|-]+").token(T::NameVariable),
            rule(r"(?<=#\()[\w!$%&*+,/:<=>?@^~|-]+").token(T::NameVariable),
            rule(r"(?<=\()(\* |\+ |\- |\/ |\< |\<\= |\= |\> |\>\= |abs |acos |angle |append |apply |asin |assoc |assq |assv |atan |boolean\? |caaaar |caaadr |caaar |caadar |caaddr |caadr |caar |cadaar |cadadr |cadar |caddar |cadddr |caddr |cadr |call\-with\-current\-continuation |call\-with\-input\-file |call\-with\-output\-file |call\-with\-values |call\/cc |car |cdaaar |cdaadr |cdaar |cdadar |cdaddr |cdadr |cdar |cddaar |cddadr |cddar |cdddar |cddddr |cdddr |cddr |cdr |ceiling |char\-\>integer |char\-alphabetic\? |char\-ci\<\=\? |char\-ci\<\? |char\-ci\=\? |char\-ci\>\=\? |char\-ci\>\? |char\-downcase |char\-lower\-case\? |char\-numeric\? |char\-ready\? |char\-upcase |char\-upper\-case\? |char\-whitespace\? |char\<\=\? |char\<\? |char\=\? |char\>\=\? |char\>\? |char\? |close\-input\-port |close\-output\-port |complex\? |cons |cos |current\-input\-port |current\-output\-port |denominator |display |dynamic\-wind |eof\-object\? |eq\? |equal\? |eqv\? |eval |even\? |exact\-\>inexact |exact\? |exp |expt |floor |for\-each |force |gcd |imag\-part |inexact\-\>exact |inexact\? |input\-port\? |integer\-\>char |integer\? |interaction\-environment |lcm |length |list |list\-\>string |list\-\>vector |list\-ref |list\-tail |list\? |load |log |magnitude |make\-polar |make\-rectangular |make\-string |make\-vector |map |max |member |memq |memv |min |modulo |negative\? |newline |not |null\-environment |null\? |number\-\>string |number\? |numerator |odd\? |open\-input\-file |open\-output\-file |output\-port\? |pair\? |peek\-char |port\? |positive\? |procedure\? |quotient |rational\? |rationalize |read |read\-char |real\-part |real\? |remainder |reverse |round |scheme\-report\-environment |set\-car\! |set\-cdr\! |sin |sqrt |string |string\-\>list |string\-\>number |string\-\>symbol |string\-append |string\-ci\<\=\? |string\-ci\<\? |string\-ci\=\? |string\-ci\>\=\? |string\-ci\>\? |string\-copy |string\-fill\! |string\-length |string\-ref |string\-set\! |string\<\=\? |string\<\? |string\=\? |string\>\=\? |string\>\? |string\? |substring |symbol\-\>string |symbol\? |tan |transcript\-off |transcript\-on |truncate |values |vector |vector\-\>list |vector\-fill\! |vector\-length |vector\-ref |vector\-set\! |vector\? |with\-input\-from\-file |with\-output\-to\-file |write |write\-char |zero\? )").token(T::NameBuiltin),
            rule(r"(?<=\()[\w!$%&*+,/:<=>?@^~|-]+").token(T::NameFunction),
            rule(r"[\w!$%&*+,/:<=>?@^~|-]+").token(T::NameVariable),
            rule(r"(\(|\))").token(T::Punctuation),
            rule(r"(\[|\])").token(T::Punctuation),
        ]),
        ("multiline-comment", &[
            rule(r"#\|").token(T::CommentMultiline).push(&[]),
            rule(r"\|#").token(T::CommentMultiline).pop(1),
            rule(r"[^|#]+").token(T::CommentMultiline),
            rule(r"[|#]").token(T::CommentMultiline),
        ]),
        ("commented-form", &[
            rule(r"\(").token(T::Comment).push(&[]),
            rule(r"\)").token(T::Comment).pop(1),
            rule(r"[^()]+").token(T::Comment),
        ]),
    ],
};

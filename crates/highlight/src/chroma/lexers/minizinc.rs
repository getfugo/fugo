//! Chroma's `minizinc.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "minizinc",
    config: ConfigDef {
        name: "MiniZinc",
        aliases: &["minizinc", "MZN", "mzn"],
        filenames: &["*.mzn", "*.dzn", "*.fzn"],
        mime_types: &["text/minizinc"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\n").token(T::Text),
            rule(r"\s+").token(T::Text),
            rule(r"\\\n").token(T::Text),
            rule(r"\%(.*?)\n").token(T::CommentSingle),
            rule(r"/(\\\n)?[*](.|\n)*?[*](\\\n)?/").token(T::CommentMultiline),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            rule(r"\b(annotation|constraint|predicate|minimize|function|maximize|satisfy|include|record|output|solve|test|list|type|ann|par|any|var|op|of)\b").token(T::Keyword),
            rule(r"\b(string|tuple|float|array|bool|enum|int|set)\b").token(T::KeywordType),
            rule(r"\b(forall|where|endif|then|else|for|if)\b").token(T::Keyword),
            rule(r"\b(array_intersect|index_set_2of3|index_set_1of3|index_set_3of3|index_set_1of2|index_set_2of2|array_union|show_float|dom_array|int2float|set2array|index_set|dom_size|lb_array|is_fixed|ub_array|bool2int|show_int|array4d|array2d|array1d|array5d|array6d|array3d|product|length|assert|concat|trace|acosh|round|abort|log10|floor|sinh|tanh|atan|sqrt|asin|show|log2|card|ceil|cosh|join|pow|cos|max|log|exp|dom|sin|abs|fix|sum|tan|min|lb|ln|ub)\b").token(T::NameBuiltin),
            rule(r"(not|<->|->|<-|\\/|xor|/\\)").token(T::Operator),
            rule(r"(<|>|<=|>=|==|=|!=)").token(T::Operator),
            rule(r"(\+|-|\*|/|div|mod)").token(T::Operator),
            rule(r"\b(intersect|superset|symdiff|subset|union|diff|in)\b").token(T::Operator),
            rule(r"(\\|\.\.|\+\+)").token(T::Operator),
            rule(r"[|()\[\]{},:;]").token(T::Punctuation),
            rule(r"(true|false)\b").token(T::KeywordConstant),
            rule(r"([+-]?)\d+(\.(?!\.)\d*)?([eE][-+]?\d+)?").token(T::LiteralNumber),
            rule(r"::\s*([^\W\d]\w*)(\s*\([^\)]*\))?").token(T::NameDecorator),
            rule(r"\b([^\W\d]\w*)\b(\()").groups(&[T::NameFunction, T::Punctuation]),
            rule(r"[^\W\d]\w*").token(T::NameOther),
        ]),
    ],
};

//! Chroma's `promql.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "promql",
    config: ConfigDef {
        name: "PromQL",
        aliases: &["promql"],
        filenames: &["*.promql"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("range", &[
            rule(r"\]").token(T::Punctuation).pop(1),
            rule(r"[1-9][0-9]*[smhdwy]").token(T::LiteralString),
        ]),
        ("function", &[
            rule(r"\)").token(T::Operator).pop(1),
            rule(r"\(").token(T::Operator).push(&[]),
            rule("").pop(1),
        ]),
        ("root", &[
            rule(r"\n").token(T::TextWhitespace),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r",").token(T::Punctuation),
            rule(r"(group_right|group_left|ignoring|without|offset|bool|on|by)\b").token(T::Keyword),
            rule(r"(count_values|quantile|bottomk|stdvar|stddev|count|group|topk|sum|min|max|avg)\b").token(T::Keyword),
            rule(r"(histogram_quantile|quantile_over_time|absent_over_time|stdvar_over_time|stddev_over_time|count_over_time|predict_linear|label_replace|max_over_time|avg_over_time|sum_over_time|days_in_month|min_over_time|day_of_month|holt_winters|day_of_week|label_join|sort_desc|clamp_max|timestamp|clamp_min|increase|changes|resets|vector|absent|idelta|minute|scalar|log10|delta|month|floor|deriv|round|irate|rate|year|sort|log2|sqrt|ceil|time|hour|abs|exp|ln)\b").token(T::KeywordReserved),
            rule(r"[1-9][0-9]*[smhdwy]").token(T::LiteralString),
            rule(r"-?[0-9]+\.[0-9]+").token(T::LiteralNumberFloat),
            rule(r"-?[0-9]+").token(T::LiteralNumberInteger),
            rule(r"#.*?$").token(T::CommentSingle),
            rule(r"(\+|\-|\*|\/|\%|\^)").token(T::Operator),
            rule(r"==|!=|>=|<=|<|>").token(T::Operator),
            rule(r"and|or|unless").token(T::OperatorWord),
            rule(r"[_a-zA-Z][a-zA-Z0-9_]+").token(T::NameVariable),
            rule(r#"(["\'])(.*?)(["\'])"#).groups(&[T::Punctuation, T::LiteralString, T::Punctuation]),
            rule(r"\(").token(T::Operator).push(&["function"]),
            rule(r"\)").token(T::Operator),
            rule(r"\{").token(T::Punctuation).push(&["labels"]),
            rule(r"\[").token(T::Punctuation).push(&["range"]),
        ]),
        ("labels", &[
            rule(r"\}").token(T::Punctuation).pop(1),
            rule(r"\n").token(T::TextWhitespace),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r",").token(T::Punctuation),
            rule(r#"([_a-zA-Z][a-zA-Z0-9_]*?)(\s*?)(=~|!=|=|!~)(\s*?)("|')(.*?)("|')"#).groups(&[T::NameLabel, T::TextWhitespace, T::Operator, T::TextWhitespace, T::Punctuation, T::LiteralString, T::Punctuation]),
        ]),
    ],
};

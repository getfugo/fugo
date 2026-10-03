//! Chroma's `mysql.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "mysql",
    config: ConfigDef {
        name: "MySQL",
        aliases: &["mysql", "mariadb"],
        filenames: &["*.sql"],
        mime_types: &["text/x-mysql", "text/x-mariadb"],
        case_insensitive: true,
        not_multiline: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("string", &[
            rule(r"[^']+").token(T::LiteralStringSingle),
            rule(r"''").token(T::LiteralStringSingle),
            rule(r"'").token(T::LiteralStringSingle).pop(1),
        ]),
        ("double-string", &[
            rule(r#"[^"]+"#).token(T::LiteralStringDouble),
            rule(r#""""#).token(T::LiteralStringDouble),
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
        ]),
        ("root", &[
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"(#|--\s+).*\n?").token(T::CommentSingle),
            rule(r"/\*").token(T::CommentMultiline).push(&["multiline-comments"]),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r"[0-9]*\.[0-9]+(e[+-][0-9]+)").token(T::LiteralNumberFloat),
            rule(r"((?:_[a-z0-9]+)?)(')").groups(&[T::LiteralStringAffix, T::LiteralStringSingle]).push(&["string"]),
            rule(r#"((?:_[a-z0-9]+)?)(")"#).groups(&[T::LiteralStringAffix, T::LiteralStringDouble]).push(&["double-string"]),
            rule(r"[+*/<>=~!@#%^&|`?-]").token(T::Operator),
            rule(r"\b(tinyint|smallint|mediumint|int|integer|bigint|date|datetime|time|bit|bool|tinytext|mediumtext|longtext|text|tinyblob|mediumblob|longblob|blob|float|double|double\s+precision|real|numeric|dec|decimal|timestamp|year|char|varchar|varbinary|varcharacter|enum|set)(\b\s*)(\()?").groups(&[T::KeywordType, T::TextWhitespace, T::Punctuation]),
            rule(r"\b(add|all|alter|analyze|and|as|asc|asensitive|before|between|bigint|binary|blob|both|by|call|cascade|case|change|char|character|check|collate|column|condition|constraint|continue|convert|create|cross|current_date|current_time|current_timestamp|current_user|cursor|database|databases|day_hour|day_microsecond|day_minute|day_second|dec|decimal|declare|default|delayed|delete|desc|describe|deterministic|distinct|distinctrow|div|double|drop|dual|each|else|elseif|enclosed|escaped|exists|exit|explain|fetch|flush|float|float4|float8|for|force|foreign|from|fulltext|grant|group|having|high_priority|hour_microsecond|hour_minute|hour_second|identified|if|ignore|in|index|infile|inner|inout|insensitive|insert|int|int1|int2|int3|int4|int8|integer|interval|into|is|iterate|join|key|keys|kill|leading|leave|left|like|limit|lines|load|localtime|localtimestamp|lock|long|loop|low_priority|match|minute_microsecond|minute_second|mod|modifies|natural|no_write_to_binlog|not|numeric|on|optimize|option|optionally|or|order|out|outer|outfile|precision|primary|privileges|procedure|purge|raid0|read|reads|real|references|regexp|release|rename|repeat|replace|require|restrict|return|revoke|right|rlike|schema|schemas|second_microsecond|select|sensitive|separator|set|show|smallint|soname|spatial|specific|sql|sql_big_result|sql_calc_found_rows|sql_small_result|sqlexception|sqlstate|sqlwarning|ssl|starting|straight_join|table|terminated|then|to|trailing|trigger|undo|union|unique|unlock|unsigned|update|usage|use|user|using|utc_date|utc_time|utc_timestamp|values|varying|when|where|while|with|write|x509|xor|year_month|zerofill)\b").token(T::Keyword),
            rule(r"\b(auto_increment|engine|charset|tables)\b").token(T::KeywordPseudo),
            rule(r"(true|false|null)").token(T::NameConstant),
            rule(r"([a-z_]\w*)(\s*)(\()").groups(&[T::NameFunction, T::TextWhitespace, T::Punctuation]),
            rule(r"[a-z_]\w*").token(T::Name),
            rule(r"@[a-z0-9]*[._]*[a-z0-9]*").token(T::NameVariable),
            rule(r"[;:()\[\],.]").token(T::Punctuation),
        ]),
        ("multiline-comments", &[
            rule(r"/\*").token(T::CommentMultiline).push(&["multiline-comments"]),
            rule(r"\*/").token(T::CommentMultiline).pop(1),
            rule(r"[^/*]+").token(T::CommentMultiline),
            rule(r"[/*]").token(T::CommentMultiline),
        ]),
    ],
};

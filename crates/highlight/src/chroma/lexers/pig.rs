//! Chroma's `pig.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "pig",
    config: ConfigDef {
        name: "Pig",
        aliases: &["pig"],
        filenames: &["*.pig"],
        mime_types: &["text/x-pig"],
        case_insensitive: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::Text),
            rule(r"--.*").token(T::Comment),
            rule(r"/\*[\w\W]*?\*/").token(T::CommentMultiline),
            rule(r"\\\n").token(T::Text),
            rule(r"\\").token(T::Text),
            rule(r"\'(?:\\[ntbrf\\\']|\\u[0-9a-f]{4}|[^\'\\\n\r])*\'").token(T::LiteralString),
            include("keywords"),
            include("types"),
            include("builtins"),
            include("punct"),
            include("operators"),
            rule(r"[0-9]*\.[0-9]+(e[0-9]+)?[fd]?").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-f]+").token(T::LiteralNumberHex),
            rule(r"[0-9]+L?").token(T::LiteralNumberInteger),
            rule(r"\n").token(T::Text),
            rule(r"([a-z_]\w*)(\s*)(\()").groups(&[T::NameFunction, T::Text, T::Punctuation]),
            rule(r"[()#:]").token(T::Text),
            rule(r#"[^(:#\'")\s]+"#).token(T::Text),
            rule(r"\S+\s+").token(T::Text),
        ]),
        ("keywords", &[
            rule(r"(assert|and|any|all|arrange|as|asc|bag|by|cache|CASE|cat|cd|cp|%declare|%default|define|dense|desc|describe|distinct|du|dump|eval|exex|explain|filter|flatten|foreach|full|generate|group|help|if|illustrate|import|inner|input|into|is|join|kill|left|limit|load|ls|map|matches|mkdir|mv|not|null|onschema|or|order|outer|output|parallel|pig|pwd|quit|register|returns|right|rm|rmf|rollup|run|sample|set|ship|split|stderr|stdin|stdout|store|stream|through|union|using|void)\b").token(T::Keyword),
        ]),
        ("builtins", &[
            rule(r"(AVG|BinStorage|cogroup|CONCAT|copyFromLocal|copyToLocal|COUNT|cross|DIFF|MAX|MIN|PigDump|PigStorage|SIZE|SUM|TextLoader|TOKENIZE)\b").token(T::NameBuiltin),
        ]),
        ("types", &[
            rule(r"(bytearray|BIGINTEGER|BIGDECIMAL|chararray|datetime|double|float|int|long|tuple)\b").token(T::KeywordType),
        ]),
        ("punct", &[
            rule(r"[;(){}\[\]]").token(T::Punctuation),
        ]),
        ("operators", &[
            rule(r"[#=,./%+\-?]").token(T::Operator),
            rule(r"(eq|gt|lt|gte|lte|neq|matches)\b").token(T::Operator),
            rule(r"(==|<=|<|>=|>|!=)").token(T::Operator),
        ]),
    ],
};

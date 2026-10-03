//! Chroma's `turing.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "turing",
    config: ConfigDef {
        name: "Turing",
        aliases: &["turing"],
        filenames: &["*.turing", "*.tu"],
        mime_types: &["text/x-turing"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\n").token(T::Text),
            rule(r"\s+").token(T::Text),
            rule(r"\\\n").token(T::Text),
            rule(r"%(.*?)\n").token(T::CommentSingle),
            rule(r"/(\\\n)?[*](.|\n)*?[*](\\\n)?/").token(T::CommentMultiline),
            rule(r"(var|fcn|function|proc|procedure|process|class|end|record|type|begin|case|loop|for|const|union|monitor|module|handler)\b").token(T::KeywordDeclaration),
            rule(r"(all|asm|assert|bind|bits|body|break|by|cheat|checked|close|condition|decreasing|def|deferred|else|elsif|exit|export|external|flexible|fork|forward|free|get|if|implement|import|include|inherit|init|invariant|label|new|objectclass|of|opaque|open|packed|pause|pervasive|post|pre|priority|put|quit|read|register|result|seek|self|set|signal|skip|tag|tell|then|timeout|to|unchecked|unqualified|wait|when|write)\b").token(T::Keyword),
            rule(r"(true|false)\b").token(T::KeywordConstant),
            rule(r"(addressint|boolean|pointer|string|array|real4|real8|nat1|int8|int4|int2|nat2|nat4|nat8|int1|real|char|enum|nat|int)\b").token(T::KeywordType),
            rule(r"\d+i").token(T::LiteralNumber),
            rule(r"\d+\.\d*([Ee][-+]\d+)?i").token(T::LiteralNumber),
            rule(r"\.\d+([Ee][-+]\d+)?i").token(T::LiteralNumber),
            rule(r"\d+[Ee][-+]\d+i").token(T::LiteralNumber),
            rule(r"\d+(\.\d+[eE][+\-]?\d+|\.\d*|[eE][+\-]?\d+)").token(T::LiteralNumberFloat),
            rule(r"\.\d+([eE][+\-]?\d+)?").token(T::LiteralNumberFloat),
            rule(r"0[0-7]+").token(T::LiteralNumberOct),
            rule(r"0[xX][0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"(0|[1-9][0-9]*)").token(T::LiteralNumberInteger),
            rule(r"(div|mod|rem|\*\*|=|<|>|>=|<=|not=|not|and|or|xor|=>|in|shl|shr|->|~|~=|~in|&|:=|\.\.|[\^+\-*/&#])").token(T::Operator),
            rule(r#"'(\\['"\\abfnrtv]|\\x[0-9a-fA-F]{2}|\\[0-7]{1,3}|\\u[0-9a-fA-F]{4}|\\U[0-9a-fA-F]{8}|[^\\])'"#).token(T::LiteralStringChar),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            rule(r"[()\[\]{}.,:]").token(T::Punctuation),
            rule(r"[^\W\d]\w*").token(T::NameOther),
        ]),
    ],
};

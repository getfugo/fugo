//! Chroma's `iscdhcpd.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "iscdhcpd",
    config: ConfigDef {
        name: "ISCdhcpd",
        aliases: &["iscdhcpd"],
        filenames: &["dhcpd.conf"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("interpol", &[
            rule(r"\$[{(]").token(T::LiteralStringInterpol).push(&[]),
            rule(r"[})]").token(T::LiteralStringInterpol).pop(1),
            rule(r"[^${()}]+").token(T::LiteralStringInterpol),
        ]),
        ("root", &[
            rule(r"#.*?\n").token(T::Comment),
            rule(r"(hardware|packet|leased-address|host-decl-name|lease-time|max-lease-time|client-state|config-option|option|filename|next-server|allow|deny|match|ignore)\b").token(T::Keyword),
            rule(r"(include|group|host|subnet|subnet6|netmask|class|subclass|pool|failover|include|shared-network|range|range6|prefix6)\b").token(T::KeywordType),
            rule(r"(on|off|true|false|none)\b").token(T::KeywordConstant),
            rule(r"(if|elsif|else)\b").token(T::Keyword),
            rule(r"(exists|known|static)\b").token(T::KeywordConstant),
            rule(r"(and|or|not)\b").token(T::OperatorWord),
            rule(r"(==|!=|~=|~~|=)").token(T::Operator),
            rule(r"[{},;\)]").token(T::Punctuation),
            rule(r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\/\d{1,2}").token(T::LiteralNumberFloat),
            rule(r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}").token(T::LiteralNumberFloat),
            rule(r"[a-fA-F0-9]{1,2}:[a-fA-F0-9]{1,2}:[a-fA-F0-9]{1,2}:[a-fA-F0-9]{1,2}:[a-fA-F0-9]{1,2}:[a-fA-F0-9]{1,2}").token(T::LiteralNumberHex),
            rule(r#"""#).token(T::LiteralString).push(&["doublequotestring"]),
            rule(r"([\w\-.]+)(\s*)(\()").groups(&[T::NameFunction, T::Text, T::Punctuation]),
            rule(r"[\w\-.]+").token(T::NameVariable),
            rule(r"\s+").token(T::Text),
        ]),
        ("doublequotestring", &[
            rule(r"\$[{(]").token(T::LiteralStringInterpol).push(&["interpol"]),
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r"\n").token(T::LiteralString),
            rule(r".").token(T::LiteralString),
        ]),
    ],
};

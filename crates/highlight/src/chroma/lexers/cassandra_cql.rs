//! Chroma's `cassandra_cql.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "cassandra_cql",
    config: ConfigDef {
        name: "Cassandra CQL",
        aliases: &["cassandra", "cql"],
        filenames: &["*.cql"],
        mime_types: &["text/x-cql"],
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
        ("quoted-ident", &[
            rule(r#"[^"]+"#).token(T::LiteralStringName),
            rule(r#""""#).token(T::LiteralStringName),
            rule(r#"""#).token(T::LiteralStringName).pop(1),
        ]),
        ("dollar-string", &[
            rule(r"[^\$]+").token(T::LiteralStringHeredoc),
            rule(r"\$\$").token(T::LiteralStringHeredoc).pop(1),
        ]),
        ("root", &[
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"(--|\/\/).*\n?").token(T::CommentSingle),
            rule(r"/\*").token(T::CommentMultiline).push(&["multiline-comments"]),
            rule(r"(ascii|bigint|blob|boolean|counter|date|decimal|double|float|frozen|inet|int|list|map|set|smallint|text|time|timestamp|timeuuid|tinyint|tuple|uuid|varchar|varint)\b").token(T::NameBuiltin),
            rule(r"(DURABLE_WRITES|LOCAL_QUORUM|MATERIALIZED|COLUMNFAMILY|REPLICATION|NORECURSIVE|NOSUPERUSER|PERMISSIONS|EACH_QUORUM|CONSISTENCY|PERMISSION|CLUSTERING|WRITETIME|SUPERUSER|KEYSPACES|AUTHORIZE|LOCAL_ONE|AGGREGATE|FINALFUNC|PARTITION|FILTERING|UNLOGGED|CONTAINS|DISTINCT|FUNCTION|LANGUAGE|INFINITY|INITCOND|TRUNCATE|KEYSPACE|PASSWORD|REPLACE|OPTIONS|TRIGGER|STORAGE|ENTRIES|RETURNS|COMPACT|PRIMARY|EXISTS|STATIC|PAGING|UPDATE|CUSTOM|VALUES|INSERT|DELETE|MODIFY|CREATE|SELECT|SCHEMA|LOGGED|REVOKE|RENAME|QUORUM|CALLED|STYPE|ORDER|ALTER|BATCH|BEGIN|COUNT|ROLES|APPLY|WHERE|SFUNC|LEVEL|INPUT|LOGIN|INDEX|TABLE|THREE|ALLOW|TOKEN|LIMIT|USING|USERS|GRANT|FROM|KEYS|JSON|USER|INTO|ROLE|TYPE|VIEW|DESC|WITH|DROP|FULL|ASC|TTL|OFF|PER|KEY|USE|ADD|NAN|ONE|ALL|ANY|TWO|AND|NOT|AS|IN|IF|OF|IS|ON|TO|BY|OR)\b").token(T::Keyword),
            rule(r"[+*/<>=~!@#%^&|`?-]+").token(T::Operator),
            rule(r"(?s)(java|javascript)(\s+)(AS)(\s+)('|\$\$)(.*?)(\5)").using_by_group(1, 6, &[E::Token(T::NameBuiltin), E::Token(T::TextWhitespace), E::Token(T::Keyword), E::Token(T::TextWhitespace), E::Token(T::LiteralStringHeredoc), E::Token(T::LiteralStringHeredoc), E::Token(T::LiteralStringHeredoc)]),
            rule(r"(true|false|null)\b").token(T::KeywordConstant),
            rule(r"0x[0-9a-f]+").token(T::LiteralNumberHex),
            rule(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").token(T::LiteralNumberHex),
            rule(r"\.[0-9]+(e[+-]?[0-9]+)?").token(T::Error),
            rule(r"-?[0-9]+(\.[0-9])?(e[+-]?[0-9]+)?").token(T::LiteralNumberFloat),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r"'").token(T::LiteralStringSingle).push(&["string"]),
            rule(r#"""#).token(T::LiteralStringName).push(&["quoted-ident"]),
            rule(r"\$\$").token(T::LiteralStringHeredoc).push(&["dollar-string"]),
            rule(r"[a-z_]\w*").token(T::Name),
            rule(r#":(['"]?)[a-z]\w*\b\1"#).token(T::NameVariable),
            rule(r"[;:()\[\]\{\},.]").token(T::Punctuation),
        ]),
        ("multiline-comments", &[
            rule(r"/\*").token(T::CommentMultiline).push(&["multiline-comments"]),
            rule(r"\*/").token(T::CommentMultiline).pop(1),
            rule(r"[^/*]+").token(T::CommentMultiline),
            rule(r"[/*]").token(T::CommentMultiline),
        ]),
    ],
};

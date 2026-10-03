//! Chroma's `postgresql_sql_dialect.xml` lexer, converted to Rust
//! (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "postgresql_sql_dialect",
    config: ConfigDef {
        name: "PostgreSQL SQL dialect",
        aliases: &["postgresql", "postgres"],
        mime_types: &["text/x-postgresql"],
        case_insensitive: true,
        not_multiline: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::Text),
            rule(r"--.*\n?").token(T::CommentSingle),
            rule(r"/\*").token(T::CommentMultiline).push(&["multiline-comments"]),
            rule(r"(bigint|bigserial|bit|bit\s+varying|bool|boolean|box|bytea|char|character|character\s+varying|cidr|circle|date|decimal|double\s+precision|float4|float8|inet|int|int2|int4|int8|integer|interval|json|jsonb|line|lseg|macaddr|money|numeric|path|pg_lsn|point|polygon|real|serial|serial2|serial4|serial8|smallint|smallserial|text|time|timestamp|timestamptz|timetz|tsquery|tsvector|txid_snapshot|uuid|varbit|varchar|with\s+time\s+zone|without\s+time\s+zone|xml|anyarray|anyelement|anyenum|anynonarray|anyrange|cstring|fdw_handler|internal|language_handler|opaque|record|void)\b").token(T::NameBuiltin),
            rule(r"(?s)(DO)(\s+)(?:(LANGUAGE)?(\s+)('?)(\w+)?('?)(\s+))?(\$)([^$]*)(\$)(.*?)(\$)(\10)(\$)").using_by_group(6, 12, &[E::Token(T::Keyword), E::Token(T::Text), E::Token(T::Keyword), E::Token(T::Text), E::Token(T::LiteralStringSingle), E::Token(T::LiteralStringSingle), E::Token(T::LiteralStringSingle), E::Token(T::Text), E::Token(T::LiteralStringHeredoc), E::Token(T::LiteralStringHeredoc), E::Token(T::LiteralStringHeredoc), E::Token(T::LiteralStringHeredoc), E::Token(T::LiteralStringHeredoc), E::Token(T::LiteralStringHeredoc), E::Token(T::LiteralStringHeredoc)]),
            rule(r"(CURRENT_TIMESTAMP|CHARACTERISTICS|CURRENT_CATALOG|CURRENT_SCHEMA|LOCALTIMESTAMP|CONFIGURATION|AUTHORIZATION|XMLATTRIBUTES|MATERIALIZED|SERIALIZABLE|CURRENT_DATE|XMLSERIALIZE|SESSION_USER|CURRENT_ROLE|CURRENT_USER|CONCURRENTLY|CURRENT_TIME|UNENCRYPTED|UNCOMMITTED|TRANSACTION|INSENSITIVE|CONSTRAINTS|ASSIGNMENT|ASYMMETRIC|DEALLOCATE|ORDINALITY|PRIVILEGES|DEFERRABLE|PROCEDURAL|CONVERSION|REFERENCES|WHITESPACE|TABLESPACE|CONSTRAINT|CONNECTION|STATISTICS|DELIMITERS|STANDALONE|DICTIONARY|REPEATABLE|CHECKPOINT|XMLELEMENT|LC_COLLATE|SYMMETRIC|PARTITION|SEQUENCES|FUNCTIONS|IMMEDIATE|IMMUTABLE|SAVEPOINT|EXTENSION|INCLUDING|RETURNING|COLLATION|INCREMENT|EXCLUSIVE|EXCLUDING|INITIALLY|COMMITTED|INTERSECT|STATEMENT|SUBSTRING|FOLLOWING|ISOLATION|TEMPORARY|ENCRYPTED|TIMESTAMP|RECURSIVE|LEAKPROOF|PROCEDURE|LOCALTIME|XMLFOREST|XMLEXISTS|AGGREGATE|ATTRIBUTE|UNBOUNDED|ASSERTION|XMLCONCAT|DELIMITER|CHARACTER|VALIDATOR|PRECISION|PRECEDING|LC_CTYPE|SECURITY|PREPARED|PASSWORD|VALIDATE|OVERLAPS|VARIADIC|DEFAULTS|VOLATILE|DEFERRED|OPERATOR|NATIONAL|UNLOGGED|UNLISTEN|TRAILING|BACKWARD|MAXVALUE|PRESERVE|DISTINCT|LOCATION|DOCUMENT|TRUNCATE|MINVALUE|POSITION|ABSOLUTE|XMLPARSE|LANGUAGE|ENCODING|CONTINUE|TEMPLATE|INTERVAL|CASCADED|DATABASE|RELATIVE|INHERITS|COMMENTS|SNAPSHOT|RESTRICT|COALESCE|IMPLICIT|ROLLBACK|EXTERNAL|REASSIGN|SMALLINT|IDENTITY|GREATEST|SEQUENCE|FUNCTION|DISCARD|CASCADE|DECIMAL|XMLROOT|FOREIGN|FORWARD|PARTIAL|PLACING|WRAPPER|WITHOUT|OVERLAY|DECLARE|PREPARE|GRANTED|CURRENT|DEFAULT|HANDLER|PRIMARY|OPTIONS|VERSION|VERBOSE|DEFINER|VARYING|ANALYSE|VARCHAR|EXTRACT|EXPLAIN|PROGRAM|RECHECK|EXECUTE|ANALYZE|INDEXES|INHERIT|EXCLUDE|CONTENT|REFRESH|REINDEX|NUMERIC|UNKNOWN|RELEASE|NOTNULL|INSTEAD|TRUSTED|INTEGER|NOTHING|COMMENT|TRIGGER|INVOKER|BETWEEN|REPLACE|REPLICA|RESTART|BOOLEAN|NATURAL|COLLATE|RETURNS|CLUSTER|LATERAL|STORAGE|DISABLE|LEADING|MAPPING|PASSING|SESSION|CATALOG|SIMILAR|SIMPLE|LISTEN|STABLE|SERVER|DOUBLE|DOMAIN|SELECT|SECOND|CALLED|SEARCH|SCROLL|STDOUT|MINUTE|SCHEMA|STRICT|REVOKE|SYSTEM|ENABLE|COLUMN|DELETE|TABLES|BINARY|BIGINT|ISNULL|BEFORE|RENAME|ESCAPE|NOTIFY|INSERT|NOWAIT|UNIQUE|NULLIF|COMMIT|UPDATE|OBJECT|VACUUM|INLINE|OFFSET|EXCEPT|EXISTS|VALUES|FAMILY|OPTION|HEADER|CREATE|HAVING|ALWAYS|WINDOW|CURSOR|WITHIN|GLOBAL|FILTER|POLICY|ACTION|PARSER|FREEZE|ACCESS|LOCAL|LEAST|OWNER|PLANS|OWNED|FORCE|XMLPI|CYCLE|ADMIN|OUTER|AFTER|ORDER|PRIOR|WRITE|CROSS|FIRST|GRANT|QUOTE|RANGE|FETCH|ALTER|WHERE|GROUP|VIEWS|ILIKE|FALSE|VALUE|INDEX|NULLS|VALID|INNER|USING|INOUT|UNTIL|RESET|NCHAR|NAMES|ARRAY|INPUT|MONTH|RIGHT|EVENT|UNION|TYPES|TREAT|BEGIN|CLOSE|LABEL|ABORT|MATCH|TABLE|CLASS|LARGE|CHECK|SYSID|CHAIN|FLOAT|STRIP|LIMIT|STDIN|SETOF|SHARE|CACHE|START|LEVEL|NAME|MOVE|SOME|LEFT|CAST|LIKE|DROP|SHOW|ZONE|EACH|ELSE|LAST|LOAD|YEAR|BOTH|CHAR|DATA|LOCK|DESC|FROM|TEMP|OVER|JOIN|TEXT|THEN|TIME|ALSO|FULL|WORK|RULE|ROWS|INTO|TRIM|TRUE|ENUM|ONLY|TYPE|WITH|READ|REAL|COST|MODE|ROLE|CASE|WHEN|COPY|NEXT|VIEW|USER|NONE|HOLD|NULL|HOUR|OIDS|BIT|ADD|SET|YES|FOR|AND|NOT|DAY|ANY|KEY|DEC|END|ASC|OFF|ROW|INT|REF|OUT|ALL|CSV|XML|ON|AT|NO|TO|AS|IN|DO|IS|IF|BY|OR|OF)\b").token(T::Keyword),
            rule(r"[+*/<>=~!@#%^&|`?-]+").token(T::Operator),
            rule(r"::").token(T::Operator),
            rule(r"\$\d+").token(T::NameVariable),
            rule(r"([0-9]*\.[0-9]*|[0-9]+)(e[+-]?[0-9]+)?").token(T::LiteralNumberFloat),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r"((?:E|U&)?)(')").groups(&[T::LiteralStringAffix, T::LiteralStringSingle]).push(&["string"]),
            rule(r#"((?:U&)?)(")"#).groups(&[T::LiteralStringAffix, T::LiteralStringName]).push(&["quoted-ident"]),
            rule(r"(?s)(\$)([^$]*)(\$)(.*?)(\$)(\2)(\$)(\s+)(LANGUAGE)?(\s+)('?)(\w+)?('?)").using_by_group(12, 4, &[E::Token(T::LiteralStringHeredoc), E::Token(T::LiteralStringHeredoc), E::Token(T::LiteralStringHeredoc), E::Token(T::LiteralStringHeredoc), E::Token(T::LiteralStringHeredoc), E::Token(T::LiteralStringHeredoc), E::Token(T::LiteralStringHeredoc), E::Token(T::Text), E::Token(T::Keyword), E::Token(T::Text), E::Token(T::LiteralStringSingle), E::Token(T::LiteralStringSingle), E::Token(T::LiteralStringSingle)]),
            rule(r"(?s)(\$)([^$]*)(\$)(.*?)(\$)(\2)(\$)").token(T::LiteralStringHeredoc),
            rule(r"[a-z_]\w*").token(T::Name),
            rule(r#":(['"]?)[a-z]\w*\b\1"#).token(T::NameVariable),
            rule(r"[;:()\[\]{},.]").token(T::Punctuation),
        ]),
        ("multiline-comments", &[
            rule(r"/\*").token(T::CommentMultiline).push(&["multiline-comments"]),
            rule(r"\*/").token(T::CommentMultiline).pop(1),
            rule(r"[^/*]+").token(T::CommentMultiline),
            rule(r"[/*]").token(T::CommentMultiline),
        ]),
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
    ],
};

//! Chroma's `materialize_sql_dialect.xml` lexer, converted to Rust
//! (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "materialize_sql_dialect",
    config: ConfigDef {
        name: "Materialize SQL dialect",
        aliases: &["materialize", "mzsql"],
        mime_types: &["text/x-materializesql"],
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
            rule(r"(ACCESS|ADD|ADDRESSES|AGGREGATE|ALIGNED|ALL|ALTER|ANALYSIS|AND|ANY|ARITY|ARN|ARRANGEMENT|ARRAY|AS|ASC|ASSERT|ASSUME|AT|AUCTION|AUTHORITY|AVAILABILITY|AVRO|AWS|BATCH|BEGIN|BETWEEN|BIGINT|BILLED|BODY|BOOLEAN|BOTH|BPCHAR|BROKEN|BROKER|BROKERS|BY|BYTES|CARDINALITY|CASCADE|CASE|CAST|CERTIFICATE|CHAIN|CHAINS|CHAR|CHARACTER|CHARACTERISTICS|CHECK|CLASS|CLIENT|CLOCK|CLOSE|CLUSTER|CLUSTERS|COALESCE|COLLATE|COLUMN|COLUMNS|COMMENT|COMMIT|COMMITTED|COMPACTION|COMPATIBILITY|COMPRESSION|COMPUTE|COMPUTECTL|CONFIG|CONFLUENT|CONNECTION|CONNECTIONS|CONSTRAINT|CONTINUAL|COPY|COUNT|COUNTER|CREATE|CREATECLUSTER|CREATEDB|CREATEROLE|CREATION|CROSS|CSV|CURRENT|CURSOR|DATABASE|DATABASES|DATUMS|DAY|DAYS|DEALLOCATE|DEBEZIUM|DEBUG|DEBUGGING|DEC|DECIMAL|DECLARE|DECODING|DECORRELATED|DEFAULT|DEFAULTS|DELETE|DELIMITED|DELIMITER|DELTA|DESC|DETAILS|DISCARD|DISK|DISTINCT|DOC|DOT|DOUBLE|DROP|EAGER|ELEMENT|ELSE|ENABLE|END|ENDPOINT|ENFORCED|ENVELOPE|ERROR|ERRORS|ESCAPE|ESTIMATE|EVERY|EXCEPT|EXCLUDE|EXECUTE|EXISTS|EXPECTED|EXPLAIN|EXPOSE|EXPRESSIONS|EXTERNAL|EXTRACT|FACTOR|FALSE|FAST|FEATURES|FETCH|FIELDS|FILE|FILTER|FIRST|FIXPOINT|FLOAT|FOLLOWING|FOR|FOREIGN|FORMAT|FORWARD|FROM|FULL|FULLNAME|FUNCTION|FUSION|GENERATOR|GRANT|GREATEST|GROUP|GROUPS|HAVING|HEADER|HEADERS|HISTORY|HOLD|HOST|HOUR|HOURS|HUMANIZED|HYDRATION|ID|IDENTIFIERS|IDS|IF|IGNORE|ILIKE|IMPLEMENTATIONS|IMPORTED|IN|INCLUDE|INDEX|INDEXES|INFO|INHERIT|INLINE|INNER|INPUT|INSERT|INSIGHTS|INSPECT|INT|INTEGER|INTERNAL|INTERSECT|INTERVAL|INTO|INTROSPECTION|IS|ISNULL|ISOLATION|JOIN|JOINS|JSON|KAFKA|KEY|KEYS|LAST|LATERAL|LATEST|LEADING|LEAST|LEFT|LEGACY|LETREC|LEVEL|LIKE|LIMIT|LINEAR|LIST|LOAD|LOCAL|LOCALLY|LOG|LOGICAL|LOGIN|LOWERING|MANAGED|MANUAL|MAP|MARKETING|MATERIALIZE|MATERIALIZED|MAX|MECHANISMS|MEMBERSHIP|MESSAGE|METADATA|MINUTE|MINUTES|MODE|MONTH|MONTHS|MUTUALLY|MYSQL|NAME|NAMES|NATURAL|NEGATIVE|NEW|NEXT|NO|NOCREATECLUSTER|NOCREATEDB|NOCREATEROLE|NODE|NOINHERIT|NOLOGIN|NON|NONE|NOSUPERUSER|NOT|NOTICE|NOTICES|NULL|NULLIF|NULLS|OBJECTS|OF|OFFSET|ON|ONLY|OPERATOR|OPTIMIZED|OPTIMIZER|OPTIONS|OR|ORDER|ORDINALITY|OUTER|OVER|OWNED|OWNER|PARTITION|PARTITIONS|PASSWORD|PATH|PHYSICAL|PLAN|PLANS|PORT|POSITION|POSTGRES|PRECEDING|PRECISION|PREFIX|PREPARE|PRIMARY|PRIVATELINK|PRIVILEGES|PROGRESS|PROTOBUF|PROTOCOL|PUBLIC|PUBLICATION|PUSHDOWN|QUERY|QUOTE|RAISE|RANGE|RATE|RAW|READ|READY|REAL|REASSIGN|RECURSION|RECURSIVE|REDACTED|REDUCE|REFERENCE|REFERENCES|REFRESH|REGEX|REGION|REGISTRY|RENAME|REOPTIMIZE|REPEATABLE|REPLACE|REPLAN|REPLICA|REPLICAS|REPLICATION|RESET|RESPECT|RESTRICT|RETAIN|RETURN|RETURNING|REVOKE|RIGHT|ROLE|ROLES|ROLLBACK|ROTATE|ROUNDS|ROW|ROWS|SASL|SCALE|SCHEDULE|SCHEMA|SCHEMAS|SECOND|SECONDS|SECRET|SECRETS|SECURITY|SEED|SELECT|SEQUENCES|SERIALIZABLE|SERVICE|SESSION|SET|SHARD|SHOW|SINK|SINKS|SIZE|SMALLINT|SNAPSHOT|SOME|SOURCE|SOURCES|SSH|SSL|START|STDIN|STDOUT|STORAGE|STORAGECTL|STRATEGY|STRICT|STRING|STRONG|SUBSCRIBE|SUBSOURCE|SUBSOURCES|SUBSTRING|SUBTREE|SUPERUSER|SWAP|SYNTAX|SYSTEM|TABLE|TABLES|TAIL|TASK|TEMP|TEMPORARY|TEXT|THEN|TICK|TIES|TIME|TIMELINE|TIMEOUT|TIMESTAMP|TIMESTAMPTZ|TIMING|TO|TOKEN|TOPIC|TPCH|TRACE|TRAILING|TRANSACTION|TRANSACTIONAL|TRIM|TRUE|TUNNEL|TYPE|TYPES|UNBOUNDED|UNCOMMITTED|UNION|UNIQUE|UNKNOWN|UNNEST|UNTIL|UP|UPDATE|UPSERT|URL|USAGE|USER|USERNAME|USERS|USING|VALIDATE|VALUE|VALUES|VARCHAR|VARIADIC|VARYING|VERSION|VIEW|VIEWS|WAIT|WARNING|WEBHOOK|WHEN|WHERE|WINDOW|WIRE|WITH|WITHIN|WITHOUT|WORK|WORKERS|WORKLOAD|WRITE|YEAR|YEARS|YUGABYTE|ZONE|ZONES)\b").token(T::Keyword),
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

//! Chroma's `puppet.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "puppet",
    config: ConfigDef {
        name: "Puppet",
        aliases: &["puppet"],
        filenames: &["*.pp"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("strings", &[
            rule(r#""([^"])*""#).token(T::LiteralString),
            rule(r"'(\\'|[^'])*'").token(T::LiteralString),
        ]),
        ("root", &[
            include("comments"),
            include("keywords"),
            include("names"),
            include("numbers"),
            include("operators"),
            include("strings"),
            rule(r"[]{}:(),;[]").token(T::Punctuation),
            rule(r"[^\S\n]+").token(T::Text),
        ]),
        ("comments", &[
            rule(r"\s*#.*$").token(T::Comment),
            rule(r"/(\\\n)?[*](.|\n)*?[*](\\\n)?/").token(T::CommentMultiline),
        ]),
        ("operators", &[
            rule(r"(=>|\?|<|>|=|\+|-|/|\*|~|!|\|)").token(T::Operator),
            rule(r"(in|and|or|not)\b").token(T::OperatorWord),
        ]),
        ("names", &[
            rule(r"[a-zA-Z_]\w*").token(T::NameAttribute),
            rule(r"(\$\S+)(\[)(\S+)(\])").groups(&[T::NameVariable, T::Punctuation, T::LiteralString, T::Punctuation]),
            rule(r"\$\S+").token(T::NameVariable),
        ]),
        ("numbers", &[
            rule(r"(\d+\.\d*|\d*\.\d+)([eE][+-]?[0-9]+)?j?").token(T::LiteralNumberFloat),
            rule(r"\d+[eE][+-]?[0-9]+j?").token(T::LiteralNumberFloat),
            rule(r"0[0-7]+j?").token(T::LiteralNumberOct),
            rule(r"0[xX][a-fA-F0-9]+").token(T::LiteralNumberHex),
            rule(r"\d+L").token(T::LiteralNumberIntegerLong),
            rule(r"\d+j?").token(T::LiteralNumberInteger),
        ]),
        ("keywords", &[
            rule(r"(?i)(nagios_servicedependency|nagios_serviceescalation|nagios_hostdependency|nagios_hostescalation|nagios_serviceextinfo|nagios_contactgroup|nagios_servicegroup|ssh_authorized_key|nagios_hostextinfo|nagios_timeperiod|nagios_hostgroup|macauthorization|create_resources|inline_template|scheduled_task|nagios_contact|nagios_command|nagios_service|nagios_host|configured|versioncmp|selboolean|filebucket|shellquote|selmodule|extlookup|unmounted|interface|contained|resources|fqdn_rand|installed|mailalias|directory|subscribe|loglevel|computer|maillist|schedule|generate|template|regsubst|inherits|present|sprintf|service|stopped|running|package|realize|defined|mounted|warning|yumrepo|k5login|include|default|notice|purged|latest|router|search|sshkey|define|notify|absent|before|augeas|import|tagged|split|undef|mount|check|alert|class|audit|debug|alias|stage|elsif|false|zpool|emerg|noop|sha1|vlan|exec|fail|file|else|host|info|cron|role|link|zone|tidy|true|node|case|user|crit|err|mcx|zfs|md5|tag|if)\b").token(T::Keyword),
        ]),
    ],
};

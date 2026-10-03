//! Chroma's `bash.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "bash",
    config: ConfigDef {
        name: "Bash",
        aliases: &["bash", "sh", "ksh", "zsh", "shell"],
        filenames: &[
            "*.sh",
            "*.ksh",
            "*.bash",
            "*.ebuild",
            "*.eclass",
            ".env",
            "*.env",
            "*.exheres-0",
            "*.exlib",
            "*.zsh",
            "*.zshrc",
            ".bashrc",
            "bashrc",
            ".bash_*",
            "bash_*",
            "zshrc",
            ".zshrc",
            "PKGBUILD",
        ],
        mime_types: &["application/x-sh", "application/x-shellscript"],
        analyse: Some(AnalyseDef {
            first: true,
            regexes: &[
                (r"(?m)^#!.*/bin/(?:env |)(?:bash|zsh|sh|ksh)", 1.0),
            ],
        }),
        ..ConfigDef::EMPTY
    },
    states: &[
        ("data", &[
            rule(r#"(?s)\$?"(\\\\|\\[0-7]+|\\.|[^"\\$])*""#).token(T::LiteralStringDouble),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["string"]),
            rule(r"(?s)\$'(\\\\|\\[0-7]+|\\.|[^'\\])*'").token(T::LiteralStringSingle),
            rule(r"(?s)'.*?'").token(T::LiteralStringSingle),
            rule(r";").token(T::Punctuation),
            rule(r"&").token(T::Punctuation),
            rule(r"\|").token(T::Punctuation),
            rule(r"\s+").token(T::Text),
            rule(r"\d+(?= |$)").token(T::LiteralNumber),
            rule(r#"[^=\s\[\]{}()$"\'`\\<&|;]+"#).token(T::Text),
            rule(r"<").token(T::Text),
        ]),
        ("string", &[
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            rule(r#"(?s)(\\\\|\\[0-7]+|\\.|[^"\\$])+"#).token(T::LiteralStringDouble),
            include("interp"),
        ]),
        ("interp", &[
            rule(r"\$\(\(").token(T::Keyword).push(&["math"]),
            rule(r"\$\(").token(T::Keyword).push(&["paren"]),
            rule(r"\$\{#?").token(T::LiteralStringInterpol).push(&["curly"]),
            rule(r"\$[a-zA-Z_]\w*").token(T::NameVariable),
            rule(r"\$(?:\d+|[#$?!_*@-])").token(T::NameVariable),
            rule(r"\$").token(T::Text),
        ]),
        ("paren", &[
            rule(r"\)").token(T::Keyword).pop(1),
            include("root"),
        ]),
        ("math", &[
            rule(r"\)\)").token(T::Keyword).pop(1),
            rule(r"[-+*/%^|&]|\*\*|\|\|").token(T::Operator),
            rule(r"\d+#\d+").token(T::LiteralNumber),
            rule(r"\d+#(?! )").token(T::LiteralNumber),
            rule(r"\d+").token(T::LiteralNumber),
            include("root"),
        ]),
        ("backticks", &[
            rule(r"`").token(T::LiteralStringBacktick).pop(1),
            include("root"),
        ]),
        ("root", &[
            include("basic"),
            rule(r"`").token(T::LiteralStringBacktick).push(&["backticks"]),
            include("data"),
            include("interp"),
        ]),
        ("basic", &[
            rule(r"\b(if|fi|else|while|do|done|for|then|return|function|case|select|continue|until|esac|elif)(\s*)\b").groups(&[T::Keyword, T::Text]),
            rule(r"\b(alias|bg|bind|break|builtin|caller|cd|command|compgen|complete|declare|dirs|disown|echo|enable|eval|exec|exit|export|false|fc|fg|getopts|hash|help|history|jobs|kill|let|local|logout|popd|printf|pushd|pwd|read|readonly|set|shift|shopt|source|suspend|test|time|times|trap|true|type|typeset|ulimit|umask|unalias|unset|wait)(?=[\s)`])").token(T::NameBuiltin),
            rule(r"\A#!.+\n").token(T::CommentPreproc),
            rule(r"#.*(\S|$)").token(T::CommentSingle),
            rule(r"\\[\w\W]").token(T::LiteralStringEscape),
            rule(r"(\b\w+)(\s*)(\+?=)").groups(&[T::NameVariable, T::Text, T::Operator]),
            rule(r"[\[\]{}()=]").token(T::Operator),
            rule(r"<<<").token(T::Operator),
            rule(r"<<-?\s*(\'?)\\?(\w+)[\w\W]+?\2").token(T::LiteralString),
            rule(r"&&|\|\|").token(T::Operator),
        ]),
        ("curly", &[
            rule(r"\}").token(T::LiteralStringInterpol).pop(1),
            rule(r":-").token(T::Keyword),
            rule(r"\w+").token(T::NameVariable),
            rule(r#"[^}:"\'`$\\]+"#).token(T::Punctuation),
            rule(r":").token(T::Punctuation),
            include("root"),
        ]),
    ],
};

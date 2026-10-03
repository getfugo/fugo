//! Chroma's `tcsh.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "tcsh",
    config: ConfigDef {
        name: "Tcsh",
        aliases: &["tcsh", "csh"],
        filenames: &["*.tcsh", "*.csh"],
        mime_types: &["application/x-csh"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("basic", &[
            rule(r"\b(if|endif|else|while|then|foreach|case|default|continue|goto|breaksw|end|switch|endsw)\s*\b").token(T::Keyword),
            rule(r"\b(alias|alloc|bg|bindkey|break|builtins|bye|caller|cd|chdir|complete|dirs|echo|echotc|eval|exec|exit|fg|filetest|getxvers|glob|getspath|hashstat|history|hup|inlib|jobs|kill|limit|log|login|logout|ls-F|migrate|newgrp|nice|nohup|notify|onintr|popd|printenv|pushd|rehash|repeat|rootnode|popd|pushd|set|shift|sched|setenv|setpath|settc|setty|setxvers|shift|source|stop|suspend|source|suspend|telltc|time|umask|unalias|uncomplete|unhash|universe|unlimit|unset|unsetenv|ver|wait|warp|watchlog|where|which)\s*\b").token(T::NameBuiltin),
            rule(r"#.*").token(T::Comment),
            rule(r"\\[\w\W]").token(T::LiteralStringEscape),
            rule(r"(\b\w+)(\s*)(=)").groups(&[T::NameVariable, T::Text, T::Operator]),
            rule(r"[\[\]{}()=]+").token(T::Operator),
            rule(r"<<\s*(\'?)\\?(\w+)[\w\W]+?\2").token(T::LiteralString),
            rule(r";").token(T::Punctuation),
        ]),
        ("data", &[
            rule(r#"(?s)"(\\\\|\\[0-7]+|\\.|[^"\\])*""#).token(T::LiteralStringDouble),
            rule(r"(?s)'(\\\\|\\[0-7]+|\\.|[^'\\])*'").token(T::LiteralStringSingle),
            rule(r"\s+").token(T::Text),
            rule(r#"[^=\s\[\]{}()$"\'`\\;#]+"#).token(T::Text),
            rule(r"\d+(?= |\Z)").token(T::LiteralNumber),
            rule(r"\$#?(\w+|.)").token(T::NameVariable),
        ]),
        ("curly", &[
            rule(r"\}").token(T::Keyword).pop(1),
            rule(r":-").token(T::Keyword),
            rule(r"\w+").token(T::NameVariable),
            rule(r#"[^}:"\'`$]+"#).token(T::Punctuation),
            rule(r":").token(T::Punctuation),
            include("root"),
        ]),
        ("paren", &[
            rule(r"\)").token(T::Keyword).pop(1),
            include("root"),
        ]),
        ("backticks", &[
            rule(r"`").token(T::LiteralStringBacktick).pop(1),
            include("root"),
        ]),
        ("root", &[
            include("basic"),
            rule(r"\$\(").token(T::Keyword).push(&["paren"]),
            rule(r"\$\{#?").token(T::Keyword).push(&["curly"]),
            rule(r"`").token(T::LiteralStringBacktick).push(&["backticks"]),
            include("data"),
        ]),
    ],
};

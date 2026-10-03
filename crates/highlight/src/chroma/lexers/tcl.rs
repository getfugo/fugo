//! Chroma's `tcl.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "tcl",
    config: ConfigDef {
        name: "Tcl",
        aliases: &["tcl"],
        filenames: &["*.tcl", "*.rvt"],
        mime_types: &["text/x-tcl", "text/x-script.tcl", "application/x-tcl"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("command-in-bracket", &[
            rule(r"\b(namespace|continue|variable|uplevel|foreach|return|update|elseif|global|rename|switch|upvar|error|vwait|catch|break|unset|array|apply|trace|after|while|then|else|expr|eval|proc|for|set|if)\b").token(T::Keyword).push(&["params-in-bracket"]),
            rule(r"\b(platform::shell|pkg::create|pkg_mkIndex|fconfigure|re_syntax|fileevent|platform|fblocked|lreverse|mathfunc|encoding|registry|lreplace|history|bgerror|llength|lsearch|linsert|lassign|lappend|refchan|unknown|package|lrepeat|msgcat|mathop|format|interp|lrange|string|source|lindex|socket|concat|regsub|regexp|loadTk|memory|binary|append|unload|subst|split|lsort|clock|close|flush|fcopy|chan|glob|time|gets|http|dict|file|puts|tell|join|read|exit|exec|open|list|scan|seek|incr|info|lset|load|dde|pwd|pid|eof|tm|cd)\b").token(T::NameBuiltin).push(&["params-in-bracket"]),
            rule(r"([\w.-]+)").token(T::NameVariable).push(&["params-in-bracket"]),
            rule(r"#").token(T::Comment).push(&["comment"]),
        ]),
        ("command-in-paren", &[
            rule(r"\b(namespace|continue|variable|uplevel|foreach|return|update|elseif|global|rename|switch|upvar|error|vwait|catch|break|unset|array|apply|trace|after|while|then|else|expr|eval|proc|for|set|if)\b").token(T::Keyword).push(&["params-in-paren"]),
            rule(r"\b(platform::shell|pkg::create|pkg_mkIndex|fconfigure|re_syntax|fileevent|platform|fblocked|lreverse|mathfunc|encoding|registry|lreplace|history|bgerror|llength|lsearch|linsert|lassign|lappend|refchan|unknown|package|lrepeat|msgcat|mathop|format|interp|lrange|string|source|lindex|socket|concat|regsub|regexp|loadTk|memory|binary|append|unload|subst|split|lsort|clock|close|flush|fcopy|chan|glob|time|gets|http|dict|file|puts|tell|join|read|exit|exec|open|list|scan|seek|incr|info|lset|load|dde|pwd|pid|eof|tm|cd)\b").token(T::NameBuiltin).push(&["params-in-paren"]),
            rule(r"([\w.-]+)").token(T::NameVariable).push(&["params-in-paren"]),
            rule(r"#").token(T::Comment).push(&["comment"]),
        ]),
        ("command-in-brace", &[
            rule(r"\b(namespace|continue|variable|uplevel|foreach|return|update|elseif|global|rename|switch|upvar|error|vwait|catch|break|unset|array|apply|trace|after|while|then|else|expr|eval|proc|for|set|if)\b").token(T::Keyword).push(&["params-in-brace"]),
            rule(r"\b(platform::shell|pkg::create|pkg_mkIndex|fconfigure|re_syntax|fileevent|platform|fblocked|lreverse|mathfunc|encoding|registry|lreplace|history|bgerror|llength|lsearch|linsert|lassign|lappend|refchan|unknown|package|lrepeat|msgcat|mathop|format|interp|lrange|string|source|lindex|socket|concat|regsub|regexp|loadTk|memory|binary|append|unload|subst|split|lsort|clock|close|flush|fcopy|chan|glob|time|gets|http|dict|file|puts|tell|join|read|exit|exec|open|list|scan|seek|incr|info|lset|load|dde|pwd|pid|eof|tm|cd)\b").token(T::NameBuiltin).push(&["params-in-brace"]),
            rule(r"([\w.-]+)").token(T::NameVariable).push(&["params-in-brace"]),
            rule(r"#").token(T::Comment).push(&["comment"]),
        ]),
        ("basic", &[
            rule(r"\(").token(T::Keyword).push(&["paren"]),
            rule(r"\[").token(T::Keyword).push(&["bracket"]),
            rule(r"\{").token(T::Keyword).push(&["brace"]),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["string"]),
            rule(r"(eq|ne|in|ni)\b").token(T::OperatorWord),
            rule(r"!=|==|<<|>>|<=|>=|&&|\|\||\*\*|[-+~!*/%<>&^|?:]").token(T::Operator),
        ]),
        ("params-in-bracket", &[
            rule(r"\]").token(T::Keyword).push(&["#pop", "#pop"]),
            include("params"),
        ]),
        ("data", &[
            rule(r"\s+").token(T::Text),
            rule(r"0x[a-fA-F0-9]+").token(T::LiteralNumberHex),
            rule(r"0[0-7]+").token(T::LiteralNumberOct),
            rule(r"\d+\.\d+").token(T::LiteralNumberFloat),
            rule(r"\d+").token(T::LiteralNumberInteger),
            rule(r"\$([\w.:-]+)").token(T::NameVariable),
            rule(r"([\w.:-]+)").token(T::Text),
        ]),
        ("command", &[
            rule(r"\b(namespace|continue|variable|uplevel|foreach|return|update|elseif|global|rename|switch|upvar|error|vwait|catch|break|unset|array|apply|trace|after|while|then|else|expr|eval|proc|for|set|if)\b").token(T::Keyword).push(&["params"]),
            rule(r"\b(platform::shell|pkg::create|pkg_mkIndex|fconfigure|re_syntax|fileevent|platform|fblocked|lreverse|mathfunc|encoding|registry|lreplace|history|bgerror|llength|lsearch|linsert|lassign|lappend|refchan|unknown|package|lrepeat|msgcat|mathop|format|interp|lrange|string|source|lindex|socket|concat|regsub|regexp|loadTk|memory|binary|append|unload|subst|split|lsort|clock|close|flush|fcopy|chan|glob|time|gets|http|dict|file|puts|tell|join|read|exit|exec|open|list|scan|seek|incr|info|lset|load|dde|pwd|pid|eof|tm|cd)\b").token(T::NameBuiltin).push(&["params"]),
            rule(r"([\w.-]+)").token(T::NameVariable).push(&["params"]),
            rule(r"#").token(T::Comment).push(&["comment"]),
        ]),
        ("params-in-brace", &[
            rule(r"\}").token(T::Keyword).push(&["#pop", "#pop"]),
            include("params"),
        ]),
        ("string-square", &[
            rule(r"\[").token(T::LiteralStringDouble).push(&["string-square"]),
            rule(r"(?s)(\\\\|\\[0-7]+|\\.|\\\n|[^\]\\])").token(T::LiteralStringDouble),
            rule(r"\]").token(T::LiteralStringDouble).pop(1),
        ]),
        ("bracket", &[
            rule(r"\]").token(T::Keyword).pop(1),
            include("command-in-bracket"),
            include("basic"),
            include("data"),
        ]),
        ("params-in-paren", &[
            rule(r"\)").token(T::Keyword).push(&["#pop", "#pop"]),
            include("params"),
        ]),
        ("paren", &[
            rule(r"\)").token(T::Keyword).pop(1),
            include("command-in-paren"),
            include("basic"),
            include("data"),
        ]),
        ("comment", &[
            rule(r".*[^\\]\n").token(T::Comment).pop(1),
            rule(r".*\\\n").token(T::Comment),
        ]),
        ("root", &[
            include("command"),
            include("basic"),
            include("data"),
            rule(r"\}").token(T::Keyword),
        ]),
        ("brace", &[
            rule(r"\}").token(T::Keyword).pop(1),
            include("command-in-brace"),
            include("basic"),
            include("data"),
        ]),
        ("params", &[
            rule(r";").token(T::Keyword).pop(1),
            rule(r"\n").token(T::Text).pop(1),
            rule(r"(else|elseif|then)\b").token(T::Keyword),
            include("basic"),
            include("data"),
        ]),
        ("string", &[
            rule(r"\[").token(T::LiteralStringDouble).push(&["string-square"]),
            rule(r#"(?s)(\\\\|\\[0-7]+|\\.|[^"\\])"#).token(T::LiteralStringDouble),
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
        ]),
    ],
};

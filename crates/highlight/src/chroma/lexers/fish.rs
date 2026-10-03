//! Chroma's `fish.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "fish",
    config: ConfigDef {
        name: "Fish",
        aliases: &["fish", "fishshell"],
        filenames: &["*.fish", "*.load"],
        mime_types: &["application/x-fish"],
        ..ConfigDef::EMPTY
    },
    states: &[
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
        ("root", &[
            include("basic"),
            include("interp"),
            include("data"),
        ]),
        ("interp", &[
            rule(r"\$\(\(").token(T::Keyword).push(&["math"]),
            rule(r"\(").token(T::Keyword).push(&["paren"]),
            rule(r"\$#?(\w+|.)").token(T::NameVariable),
        ]),
        ("basic", &[
            rule(r"(?<=(?:^|\A|;|&&|\|\||\||\b(continue|function|return|switch|begin|while|break|count|false|block|echo|case|true|else|exit|test|set|cdh|and|pwd|for|end|not|if|cd|or)\b)\s*)(continue|function|return|switch|begin|while|break|count|false|block|test|case|true|echo|exit|else|set|cdh|and|pwd|for|end|not|if|cd|or)(?=;?\b)").token(T::Keyword),
            rule(r"(?<=for\s+\S+\s+)in\b").token(T::Keyword),
            rule(r"\b(fish_update_completions|fish_command_not_found|fish_breakpoint_prompt|fish_status_to_signal|fish_right_prompt|fish_is_root_user|fish_mode_prompt|fish_vcs_prompt|fish_key_reader|fish_svn_prompt|fish_git_prompt|fish_hg_prompt|fish_greeting|fish_add_path|commandline|fish_prompt|fish_indent|fish_config|fish_pager|breakpoint|fish_title|prompt_pwd|functions|set_color|realpath|funcsave|contains|complete|argparse|fish_opt|history|builtin|getopts|suspend|command|mimedb|printf|ulimit|disown|string|source|funced|status|random|isatty|fishd|prevd|vared|umask|nextd|alias|pushd|emit|jobs|popd|help|psub|wait|fish|read|time|exec|eval|math|trap|type|dirs|dirh|abbr|kill|bind|hash|open|fc|bg|fg)\s*\b(?!\.)").token(T::NameBuiltin),
            rule(r"#!.*\n").token(T::CommentHashbang),
            rule(r"#.*\n").token(T::Comment),
            rule(r"\\[\w\W]").token(T::LiteralStringEscape),
            rule(r"(\b\w+)(\s*)(=)").groups(&[T::NameVariable, T::Text, T::Operator]),
            rule(r"[\[\]()={}]").token(T::Operator),
            rule(r"(?<=\[[^\]]+)\.\.|-(?=[^\[]+\])").token(T::Operator),
            rule(r"<<-?\s*(\'?)\\?(\w+)[\w\W]+?\2").token(T::LiteralString),
            rule(r"(?<=set\s+(?:--?[^\d\W][\w-]*\s+)?)\w+").token(T::NameVariable),
            rule(r"(?<=for\s+)\w[\w-]*(?=\s+in)").token(T::NameVariable),
            rule(r"(?<=function\s+)\w(?:[^\n])*?(?= *[-\n])").token(T::NameFunction),
            rule(r"(?<=(?:^|\b(?:and|or|sudo)\b|;|\|\||&&|\||\(|(?:\b\w+\s*=\S+\s)) *)\w[\w-]*").token(T::NameFunction),
        ]),
        ("data", &[
            rule(r#"(?s)\$?"(\\\\|\\[0-7]+|\\.|[^"\\$])*""#).token(T::LiteralStringDouble),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["string"]),
            rule(r"(?s)\$'(\\\\|\\[0-7]+|\\.|[^'\\])*'").token(T::LiteralStringSingle),
            rule(r"(?s)'.*?'").token(T::LiteralStringSingle),
            rule(r";").token(T::Punctuation),
            rule(r"&&|\|\||&|\||\^|<|>").token(T::Operator),
            rule(r"\s+").token(T::Text),
            rule(r"\b\d+\b").token(T::LiteralNumber),
            rule(r"(?<=\s+)--?[^\d][\w-]*").token(T::NameAttribute),
            rule(r".+?").token(T::Text),
        ]),
        ("string", &[
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            rule(r#"(?s)(\\\\|\\[0-7]+|\\.|[^"\\$])+"#).token(T::LiteralStringDouble),
            include("interp"),
        ]),
    ],
};

//! Chroma's `meson.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "meson",
    config: ConfigDef {
        name: "Meson",
        aliases: &["meson", "meson.build"],
        filenames: &["meson.build", "meson_options.txt"],
        mime_types: &["text/x-meson"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"#.*?$").token(T::Comment),
            rule(r"'''.*'''").token(T::LiteralStringSingle),
            rule(r"[1-9][0-9]*").token(T::LiteralNumberInteger),
            rule(r"0o[0-7]+").token(T::LiteralNumberOct),
            rule(r"0x[a-fA-F0-9]+").token(T::LiteralNumberHex),
            include("string"),
            include("keywords"),
            include("expr"),
            rule(r"[a-zA-Z_][a-zA-Z_0-9]*").token(T::Name),
            rule(r"\s+").token(T::TextWhitespace),
        ]),
        ("string", &[
            rule(r"[']{3}([']{0,2}([^\\']|\\(.|\n)))*[']{3}").token(T::LiteralString),
            rule(r"'.*?(?<!\\)(\\\\)*?'").token(T::LiteralString),
        ]),
        ("keywords", &[
            rule(r"(endforeach|continue|foreach|break|endif|else|elif|if)\b").token(T::Keyword),
        ]),
        ("expr", &[
            rule(r"(in|and|or|not)\b").token(T::OperatorWord),
            rule(r"(\*=|/=|%=|\+]=|-=|==|!=|\+|-|=)").token(T::Operator),
            rule(r"[\[\]{}:().,?]").token(T::Punctuation),
            rule(r"(false|true)\b").token(T::KeywordConstant),
            include("builtins"),
            rule(r"(target_machine|build_machine|host_machine|meson)\b").token(T::NameVariableMagic),
        ]),
        ("builtins", &[
            rule(r"(?<!\.)(add_project_link_arguments|add_global_link_arguments|add_project_arguments|add_global_arguments|include_directories|configuration_data|declare_dependency|install_headers|both_libraries|install_subdir|add_test_setup|configure_file|static_library|shared_library|custom_target|add_languages|shared_module|set_variable|get_variable|find_library|find_program|build_target|install_data|environment|is_disabler|run_command|subdir_done|install_man|is_variable|subproject|dependency|join_paths|get_option|executable|generator|benchmark|disabler|project|message|library|summary|vcs_tag|warning|assert|subdir|range|files|error|test|jar)\b").token(T::NameBuiltin),
            rule(r"(?<!\.)import\b").token(T::NameNamespace),
        ]),
    ],
};

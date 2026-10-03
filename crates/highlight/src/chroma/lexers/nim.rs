//! Chroma's `nim.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "nim",
    config: ConfigDef {
        name: "Nim",
        aliases: &["nim", "nimrod"],
        filenames: &["*.nim", "*.nimrod"],
        mime_types: &["text/x-nim"],
        case_insensitive: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("dqs", &[
            rule(r#"\\([\\abcefnrtvl"\']|\n|x[a-f0-9]{2}|[0-9]{1,3})"#).token(T::LiteralStringEscape),
            rule(r#"""#).token(T::LiteralString).pop(1),
            include("strings"),
        ]),
        ("tdqs", &[
            rule(r#""""(?!")"#).token(T::LiteralString).pop(1),
            include("strings"),
            include("nl"),
        ]),
        ("funcname", &[
            rule(r"((?![\d_])\w)(((?!_)\w)|(_(?!_)\w))*").token(T::NameFunction).pop(1),
            rule(r"`.+`").token(T::NameFunction).pop(1),
        ]),
        ("int-suffix", &[
            rule(r"\'(i|u)(32|64)").token(T::LiteralNumberIntegerLong),
            rule(r"\'(u|(i|u)(8|16))").token(T::LiteralNumberInteger),
            rule("").pop(1),
        ]),
        ("float-suffix", &[
            rule(r"\'(f|d|f(32|64))").token(T::LiteralNumberFloat),
            rule("").pop(1),
        ]),
        ("strings", &[
            rule(r"(?<!\$)\$(\d+|#|\w+)+").token(T::LiteralStringInterpol),
            rule(r#"[^\\\'"$\n]+"#).token(T::LiteralString),
            rule(r#"[\'"\\]"#).token(T::LiteralString),
            rule(r"\$").token(T::LiteralString),
        ]),
        ("nl", &[
            rule(r"\n").token(T::LiteralString),
        ]),
        ("chars", &[
            rule(r#"\\([\\abcefnrtvl"\']|x[a-f0-9]{2}|[0-9]{1,3})"#).token(T::LiteralStringEscape),
            rule(r"'").token(T::LiteralStringChar).pop(1),
            rule(r".").token(T::LiteralStringChar),
        ]),
        ("rdqs", &[
            rule(r#""(?!")"#).token(T::LiteralString).pop(1),
            rule(r#""""#).token(T::LiteralStringEscape),
            include("strings"),
        ]),
        ("float-number", &[
            rule(r"\.(?!\.)[0-9_]*").token(T::LiteralNumberFloat),
            rule(r"e[+-]?[0-9][0-9_]*").token(T::LiteralNumberFloat),
            rule("").pop(1),
        ]),
        ("root", &[
            rule(r"#\[[\s\S]*?\]#").token(T::CommentMultiline),
            rule(r"##.*$").token(T::LiteralStringDoc),
            rule(r"#.*$").token(T::Comment),
            rule(r"[*=><+\-/@$~&%!?|\\\[\]]").token(T::Operator),
            rule(r"\.\.|\.|,|\[\.|\.\]|\{\.|\.\}|\(\.|\.\)|\{|\}|\(|\)|:|\^|`|;").token(T::Punctuation),
            rule(r#"(?:[\w]+)""""#).token(T::LiteralString).push(&["tdqs"]),
            rule(r#"(?:[\w]+)""#).token(T::LiteralString).push(&["rdqs"]),
            rule(r#"""""#).token(T::LiteralString).push(&["tdqs"]),
            rule(r#"""#).token(T::LiteralString).push(&["dqs"]),
            rule(r"'").token(T::LiteralStringChar).push(&["chars"]),
            rule(r"(a_?n_?d_?|o_?r_?|n_?o_?t_?|x_?o_?r_?|s_?h_?l_?|s_?h_?r_?|d_?i_?v_?|m_?o_?d_?|i_?n_?|n_?o_?t_?i_?n_?|i_?s_?|i_?s_?n_?o_?t_?)\b").token(T::OperatorWord),
            rule(r"(p_?r_?o_?c_?\s)(?![(\[\]])").token(T::Keyword).push(&["funcname"]),
            rule(r"(a_?d_?d_?r_?|a_?n_?d_?|a_?s_?|a_?s_?m_?|a_?t_?o_?m_?i_?c_?|b_?i_?n_?d_?|b_?l_?o_?c_?k_?|b_?r_?e_?a_?k_?|c_?a_?s_?e_?|c_?a_?s_?t_?|c_?o_?n_?c_?e_?p_?t_?|c_?o_?n_?s_?t_?|c_?o_?n_?t_?i_?n_?u_?e_?|c_?o_?n_?v_?e_?r_?t_?e_?r_?|d_?e_?f_?e_?r_?|d_?i_?s_?c_?a_?r_?d_?|d_?i_?s_?t_?i_?n_?c_?t_?|d_?i_?v_?|d_?o_?|e_?l_?i_?f_?|e_?l_?s_?e_?|e_?n_?d_?|e_?n_?u_?m_?|e_?x_?c_?e_?p_?t_?|e_?x_?p_?o_?r_?t_?|f_?i_?n_?a_?l_?l_?y_?|f_?o_?r_?|f_?u_?n_?c_?|i_?f_?|i_?n_?|y_?i_?e_?l_?d_?|i_?n_?t_?e_?r_?f_?a_?c_?e_?|i_?s_?|i_?s_?n_?o_?t_?|i_?t_?e_?r_?a_?t_?o_?r_?|l_?e_?t_?|m_?a_?c_?r_?o_?|m_?e_?t_?h_?o_?d_?|m_?i_?x_?i_?n_?|m_?o_?d_?|n_?o_?t_?|n_?o_?t_?i_?n_?|o_?b_?j_?e_?c_?t_?|o_?f_?|o_?r_?|o_?u_?t_?|p_?r_?o_?c_?|p_?t_?r_?|r_?a_?i_?s_?e_?|r_?e_?f_?|r_?e_?t_?u_?r_?n_?|s_?h_?a_?r_?e_?d_?|s_?h_?l_?|s_?h_?r_?|s_?t_?a_?t_?i_?c_?|t_?e_?m_?p_?l_?a_?t_?e_?|t_?r_?y_?|t_?u_?p_?l_?e_?|t_?y_?p_?e_?|w_?h_?e_?n_?|w_?h_?i_?l_?e_?|w_?i_?t_?h_?|w_?i_?t_?h_?o_?u_?t_?|x_?o_?r_?)\b").token(T::Keyword),
            rule(r"(f_?r_?o_?m_?|i_?m_?p_?o_?r_?t_?|i_?n_?c_?l_?u_?d_?e_?)\b").token(T::KeywordNamespace),
            rule(r"(v_?a_?r)\b").token(T::KeywordDeclaration),
            rule(r"(i_?n_?t_?|i_?n_?t_?8_?|i_?n_?t_?1_?6_?|i_?n_?t_?3_?2_?|i_?n_?t_?6_?4_?|f_?l_?o_?a_?t_?|f_?l_?o_?a_?t_?3_?2_?|f_?l_?o_?a_?t_?6_?4_?|b_?o_?o_?l_?|c_?h_?a_?r_?|r_?a_?n_?g_?e_?|a_?r_?r_?a_?y_?|s_?e_?q_?|s_?e_?t_?|s_?t_?r_?i_?n_?g_?)\b").token(T::KeywordType),
            rule(r"(n_?i_?l_?|t_?r_?u_?e_?|f_?a_?l_?s_?e_?)\b").token(T::KeywordPseudo),
            rule(r"\b_\b").token(T::Name),
            rule(r"\b((?![_\d])\w)(((?!_)\w)|(_(?!_)\w))*").token(T::Name),
            rule(r"[0-9][0-9_]*(?=([e.]|\'(f|d|f(32|64))))").token(T::LiteralNumberFloat).push(&["float-suffix", "float-number"]),
            rule(r"0x[a-f0-9][a-f0-9_]*").token(T::LiteralNumberHex).push(&["int-suffix"]),
            rule(r"0b[01][01_]*").token(T::LiteralNumberBin).push(&["int-suffix"]),
            rule(r"0o[0-7][0-7_]*").token(T::LiteralNumberOct).push(&["int-suffix"]),
            rule(r"[0-9][0-9_]*").token(T::LiteralNumberInteger).push(&["int-suffix"]),
            rule(r"\s+").token(T::Text),
            rule(r".+$").token(T::Error),
        ]),
    ],
};

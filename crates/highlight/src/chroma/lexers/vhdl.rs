//! Chroma's `vhdl.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "vhdl",
    config: ConfigDef {
        name: "VHDL",
        aliases: &["vhdl"],
        filenames: &["*.vhdl", "*.vhd"],
        mime_types: &["text/x-vhdl"],
        case_insensitive: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\n").token(T::Text),
            rule(r"\s+").token(T::Text),
            rule(r"\\\n").token(T::Text),
            rule(r"--.*?$").token(T::CommentSingle),
            rule(r"'(U|X|0|1|Z|W|L|H|-)'").token(T::LiteralStringChar),
            rule(r"[~!%^&*+=|?:<>/-]").token(T::Operator),
            rule(r"'[a-z_]\w*").token(T::NameAttribute),
            rule(r"[()\[\],.;\']").token(T::Punctuation),
            rule(r#""[^\n\\"]*""#).token(T::LiteralString),
            rule(r"(library)(\s+)([a-z_]\w*)").groups(&[T::Keyword, T::Text, T::NameNamespace]),
            rule(r"(use)(\s+)(entity)").groups(&[T::Keyword, T::Text, T::Keyword]),
            rule(r"(use)(\s+)([a-z_][\w.]*\.)(all)").groups(&[T::Keyword, T::Text, T::NameNamespace, T::Keyword]),
            rule(r"(use)(\s+)([a-z_][\w.]*)").groups(&[T::Keyword, T::Text, T::NameNamespace]),
            rule(r"(std|ieee)(\.[a-z_]\w*)").groups(&[T::NameNamespace, T::NameNamespace]),
            rule(r"(ieee|work|std)\b").token(T::NameNamespace),
            rule(r"(entity|component)(\s+)([a-z_]\w*)").groups(&[T::Keyword, T::Text, T::NameClass]),
            rule(r"(architecture|configuration)(\s+)([a-z_]\w*)(\s+)(of)(\s+)([a-z_]\w*)(\s+)(is)").groups(&[T::Keyword, T::Text, T::NameClass, T::Text, T::Keyword, T::Text, T::NameClass, T::Text, T::Keyword]),
            rule(r"([a-z_]\w*)(:)(\s+)(process|for)").groups(&[T::NameClass, T::Operator, T::Text, T::Keyword]),
            rule(r"(end)(\s+)").groups(&[T::Keyword, T::Text]).push(&["endblock"]),
            include("types"),
            include("keywords"),
            include("numbers"),
            rule(r"[a-z_]\w*").token(T::Name),
        ]),
        ("endblock", &[
            include("keywords"),
            rule(r"[a-z_]\w*").token(T::NameClass),
            rule(r"(\s+)").token(T::Text),
            rule(r";").token(T::Punctuation).pop(1),
        ]),
        ("types", &[
            rule(r"(std_ulogic_vector|file_open_status|std_logic_vector|severity_level|file_open_kind|delay_length|std_ulogic|bit_vector|character|std_logic|positive|unsigned|boolean|natural|integer|signed|string|time|bit)\b").token(T::KeywordType),
        ]),
        ("keywords", &[
            rule(r"(configuration|architecture|disconnect|attribute|transport|postponed|procedure|component|function|variable|severity|constant|generate|register|inertial|package|library|guarded|linkage|generic|subtype|process|literal|record|entity|others|shared|signal|downto|access|assert|return|reject|buffer|impure|select|elsif|inout|until|label|range|group|units|begin|array|alias|after|block|while|null|next|file|when|wait|open|nand|exit|then|case|port|type|loop|else|pure|with|xnor|body|not|rem|bus|rol|ror|xor|abs|end|and|sla|sll|sra|srl|all|out|nor|mod|map|for|new|use|or|on|of|in|if|is|to)\b").token(T::Keyword),
        ]),
        ("numbers", &[
            rule(r"\d{1,2}#[0-9a-f_]+#?").token(T::LiteralNumberInteger),
            rule(r"\d+").token(T::LiteralNumberInteger),
            rule(r"(\d+\.\d*|\.\d+|\d+)E[+-]?\d+").token(T::LiteralNumberFloat),
            rule(r#"X"[0-9a-f_]+""#).token(T::LiteralNumberHex),
            rule(r#"O"[0-7_]+""#).token(T::LiteralNumberOct),
            rule(r#"B"[01_]+""#).token(T::LiteralNumberBin),
        ]),
    ],
};

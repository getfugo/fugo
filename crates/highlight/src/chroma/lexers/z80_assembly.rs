//! Chroma's `z80_assembly.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "z80_assembly",
    config: ConfigDef {
        name: "Z80 Assembly",
        aliases: &["z80"],
        filenames: &["*.z80", "*.asm"],
        case_insensitive: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("string", &[
            rule(r#"[^"\\]+"#).token(T::LiteralString),
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r#"""#).token(T::LiteralString).pop(1),
        ]),
        ("root", &[
            rule(r";.*?$").token(T::CommentSingle),
            rule(r"^[.\w]+:").token(T::NameLabel),
            rule(r"((0x)|\$)[0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"[0-9][0-9a-fA-F]+h").token(T::LiteralNumberHex),
            rule(r"((0b)|%)[01]+").token(T::LiteralNumberBin),
            rule(r"-?[0-9]+").token(T::LiteralNumberInteger),
            rule(r#"""#).token(T::LiteralString).push(&["string"]),
            rule(r"'\\?.'").token(T::LiteralStringChar),
            rule(r"[,=()\\]").token(T::Punctuation),
            rule(r"^\s*#\w+").token(T::CommentPreproc),
            rule(r"\.(db|dw|end|org|byte|word|fill|block|addinstr|echo|error|list|nolist|equ|show|option|seek)").token(T::NameBuiltin),
            rule(r"(ex|exx|ld|ldd|lddr|ldi|ldir|pop|push|adc|add|cp|cpd|cpdr|cpi|cpir|cpl|daa|dec|inc|neg|sbc|sub|and|bit|ccf|or|res|scf|set|xor|rl|rla|rlc|rlca|rld|rr|rra|rrc|rrca|rrd|sla|sra|srl|call|djnz|jp|jr|ret|rst|nop|reti|retn|di|ei|halt|im|in|ind|indr|ini|inir|out|outd|otdr|outi|otir)").token(T::Keyword),
            rule(r"(z|nz|c|nc|po|pe|p|m)").token(T::Keyword),
            rule(r"[+-/*~\^&|]").token(T::Operator),
            rule(r"\w+").token(T::Text),
            rule(r"\s+").token(T::Text),
        ]),
    ],
};

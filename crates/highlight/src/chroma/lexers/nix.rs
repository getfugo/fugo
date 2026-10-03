//! Chroma's `nix.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "nix",
    config: ConfigDef {
        name: "Nix",
        aliases: &["nixos", "nix"],
        filenames: &["*.nix"],
        mime_types: &["text/x-nix"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("space", &[
            rule(r"[ \t\r\n]+").token(T::Text),
        ]),
        ("paren", &[
            rule(r"\)").token(T::Punctuation).pop(1),
            include("root"),
        ]),
        ("scope", &[
            rule(r"}:").token(T::Punctuation).pop(1),
            rule(r"}").token(T::Punctuation).pop(1),
            rule(r"in(?![a-zA-Z0-9_'-])").token(T::Keyword).pop(1),
            rule(r"\${").token(T::LiteralStringInterpol).push(&["interpol"]),
            include("root"),
            rule(r"(=|\?|,)").token(T::Operator),
        ]),
        ("builtins", &[
            rule(r"throw(?![a-zA-Z0-9_'-])").token(T::NameException),
            rule(r"(dependencyClosure|fetchTarball|filterSource|currentTime|removeAttrs|baseNameOf|derivation|toString|builtins|getAttr|hasAttr|getEnv|isNull|abort|dirOf|toXML|map)(?![a-zA-Z0-9_'-])").token(T::NameBuiltin),
        ]),
        ("literals", &[
            rule(r"(false|true|null)(?![a-zA-Z0-9_'-])").token(T::NameConstant),
            include("uri"),
            include("path"),
            include("int"),
            include("float"),
        ]),
        ("keywords", &[
            rule(r"import(?![a-zA-Z0-9_'-])").token(T::KeywordNamespace),
            rule(r"(inherit|assert|with|then|else|rec|if)(?![a-zA-Z0-9_'-])").token(T::Keyword),
        ]),
        ("list", &[
            rule(r"\]").token(T::Punctuation).pop(1),
            include("root"),
        ]),
        ("operators", &[
            rule(r" [/-] ").token(T::Operator),
            rule(r"(\.)(\${)").groups(&[T::Operator, T::LiteralStringInterpol]).push(&["interpol"]),
            rule(r"(\?)(\s*)(\${)").groups(&[T::Operator, T::Text, T::LiteralStringInterpol]).push(&["interpol"]),
            rule(r"(&&|>=|<=|\+\+|->|!=|=|\|\||//|==|@|!|\+|\?|<|\.|>|\*)").token(T::Operator),
            rule(r"[;:]").token(T::Punctuation),
        ]),
        ("comment", &[
            rule(r"\*/").token(T::CommentMultiline).pop(1),
            rule(r".|\n").token(T::CommentMultiline),
        ]),
        ("interpol", &[
            rule(r"}").token(T::LiteralStringInterpol).pop(1),
            include("root"),
        ]),
        ("path", &[
            rule(r"[a-zA-Z0-9._+-]*(/[a-zA-Z0-9._+-]+)+").token(T::LiteralStringRegex),
            rule(r"~(/[a-zA-Z0-9._+-]+)+/?").token(T::LiteralStringRegex),
            rule(r"<[a-zA-Z0-9._+-]+(/[a-zA-Z0-9._+-]+)*>").token(T::LiteralStringRegex),
        ]),
        ("float", &[
            rule(r"-?(([1-9][0-9]*\.[0-9]*)|(0?\.[0-9]+))([Ee][+-]?[0-9]+)?(?![a-zA-Z0-9_'-])").token(T::LiteralNumberFloat),
        ]),
        ("root", &[
            include("keywords"),
            include("builtins"),
            include("literals"),
            include("operators"),
            rule(r"#.*$").token(T::CommentSingle),
            rule(r"/\*").token(T::CommentMultiline).push(&["comment"]),
            rule(r"\(").token(T::Punctuation).push(&["paren"]),
            rule(r"\[").token(T::Punctuation).push(&["list"]),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["qstring"]),
            rule(r"''").token(T::LiteralStringSingle).push(&["istring"]),
            rule(r"{").token(T::Punctuation).push(&["scope"]),
            rule(r"let(?![a-zA-Z0-9_'-])").token(T::Keyword).push(&["scope"]),
            include("id"),
            include("space"),
        ]),
        ("int", &[
            rule(r"-?[0-9]+(?![a-zA-Z0-9_'-])").token(T::LiteralNumberInteger),
        ]),
        ("uri", &[
            rule(r"[a-zA-Z][a-zA-Z0-9+.-]*:[a-zA-Z0-9%/?:@&=+$,_.!~*'-]+").token(T::LiteralStringDoc),
        ]),
        ("qstring", &[
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            rule(r"\${").token(T::LiteralStringInterpol).push(&["interpol"]),
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r".|\n").token(T::LiteralStringDouble),
        ]),
        ("istring", &[
            rule(r"''\$").token(T::LiteralStringEscape),
            rule(r"'''").token(T::LiteralStringEscape),
            rule(r"''\\.").token(T::LiteralStringEscape),
            rule(r"''").token(T::LiteralStringSingle).pop(1),
            rule(r"\${").token(T::LiteralStringInterpol).push(&["interpol"]),
            rule(r"\$.").token(T::LiteralStringSingle),
            rule(r".|\n").token(T::LiteralStringSingle),
        ]),
        ("id", &[
            rule(r"[a-zA-Z_][a-zA-Z0-9_'-]*").token(T::Name),
        ]),
    ],
};

//! Chroma's `ada.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "ada",
    config: ConfigDef {
        name: "Ada",
        aliases: &["ada", "ada95", "ada2005"],
        filenames: &["*.adb", "*.ads", "*.ada"],
        mime_types: &["text/x-ada"],
        case_insensitive: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("end", &[
            rule(r"(if|case|record|loop|select)").token(T::KeywordReserved),
            rule(r#""[^"]+"|[\w.]+"#).token(T::NameFunction),
            rule(r"\s+").token(T::Text),
            rule(r";").token(T::Punctuation).pop(1),
        ]),
        ("array_def", &[
            rule(r";").token(T::Punctuation).pop(1),
            rule(r"(\w+)(\s+)(range)").groups(&[T::KeywordType, T::Text, T::KeywordReserved]),
            include("root"),
        ]),
        ("package_instantiation", &[
            rule(r#"("[^"]+"|\w+)(\s+)(=>)"#).groups(&[T::NameVariable, T::Text, T::Punctuation]),
            rule(r#"[\w.\'"]"#).token(T::Text),
            rule(r"\)").token(T::Punctuation).pop(1),
            include("root"),
        ]),
        ("subprogram", &[
            rule(r"\(").token(T::Punctuation).push(&["#pop", "formal_part"]),
            rule(r";").token(T::Punctuation).pop(1),
            rule(r"is\b").token(T::KeywordReserved).pop(1),
            rule(r#""[^"]+"|\w+"#).token(T::NameFunction),
            include("root"),
        ]),
        ("type_def", &[
            rule(r";").token(T::Punctuation).pop(1),
            rule(r"\(").token(T::Punctuation).push(&["formal_part"]),
            rule(r"with|and|use").token(T::KeywordReserved),
            rule(r"array\b").token(T::KeywordReserved).push(&["#pop", "array_def"]),
            rule(r"record\b").token(T::KeywordReserved).push(&["record_def"]),
            rule(r"(null record)(;)").groups(&[T::KeywordReserved, T::Punctuation]).pop(1),
            include("root"),
        ]),
        ("import", &[
            rule(r"[\w.]+").token(T::NameNamespace).pop(1),
            rule("").pop(1),
        ]),
        ("formal_part", &[
            rule(r"\)").token(T::Punctuation).pop(1),
            rule(r"\w+").token(T::NameVariable),
            rule(r",|:[^=]").token(T::Punctuation),
            rule(r"(in|not|null|out|access)\b").token(T::KeywordReserved),
            include("root"),
        ]),
        ("package", &[
            rule(r"body").token(T::KeywordDeclaration),
            rule(r"is\s+new|renames").token(T::KeywordReserved),
            rule(r"is").token(T::KeywordReserved).pop(1),
            rule(r";").token(T::Punctuation).pop(1),
            rule(r"\(").token(T::Punctuation).push(&["package_instantiation"]),
            rule(r"([\w.]+)").token(T::NameClass),
            include("root"),
        ]),
        ("attribute", &[
            rule(r"(')(\w+)").groups(&[T::Punctuation, T::NameAttribute]),
        ]),
        ("record_def", &[
            rule(r"end record").token(T::KeywordReserved).pop(1),
            include("root"),
        ]),
        ("root", &[
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"--.*?\n").token(T::CommentSingle),
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"function|procedure|entry").token(T::KeywordDeclaration).push(&["subprogram"]),
            rule(r"(subtype|type)(\s+)(\w+)").groups(&[T::KeywordDeclaration, T::Text, T::KeywordType]).push(&["type_def"]),
            rule(r"task|protected").token(T::KeywordDeclaration),
            rule(r"(subtype)(\s+)").groups(&[T::KeywordDeclaration, T::Text]),
            rule(r"(end)(\s+)").groups(&[T::KeywordReserved, T::Text]).push(&["end"]),
            rule(r"(pragma)(\s+)(\w+)").groups(&[T::KeywordReserved, T::Text, T::CommentPreproc]),
            rule(r"(true|false|null)\b").token(T::KeywordConstant),
            rule(r"(Short_Short_Integer|Short_Short_Float|Long_Long_Integer|Long_Long_Float|Wide_Character|Reference_Type|Short_Integer|Long_Integer|Wide_String|Short_Float|Controlled|Long_Float|Character|Generator|File_Type|File_Mode|Positive|Duration|Boolean|Natural|Integer|Address|Cursor|String|Count|Float|Byte)\b").token(T::KeywordType),
            rule(r"(and(\s+then)?|in|mod|not|or(\s+else)|rem)\b").token(T::OperatorWord),
            rule(r"generic|private").token(T::KeywordDeclaration),
            rule(r"package").token(T::KeywordDeclaration).push(&["package"]),
            rule(r"array\b").token(T::KeywordReserved).push(&["array_def"]),
            rule(r"(with|use)(\s+)").groups(&[T::KeywordNamespace, T::Text]).push(&["import"]),
            rule(r"(\w+)(\s*)(:)(\s*)(constant)").groups(&[T::NameConstant, T::Text, T::Punctuation, T::Text, T::KeywordReserved]),
            rule(r"<<\w+>>").token(T::NameLabel),
            rule(r"(\w+)(\s*)(:)(\s*)(declare|begin|loop|for|while)").groups(&[T::NameLabel, T::Text, T::Punctuation, T::Text, T::KeywordReserved]),
            rule(r"\b(synchronized|overriding|terminate|interface|exception|protected|separate|constant|abstract|renames|reverse|subtype|aliased|declare|requeue|limited|return|tagged|access|record|select|accept|digits|others|pragma|entry|elsif|delta|delay|array|until|range|raise|while|begin|abort|else|loop|when|type|null|then|body|task|goto|case|exit|end|for|abs|xor|all|new|out|is|of|if|or|do|at)\b").token(T::KeywordReserved),
            rule(r#""[^"]*""#).token(T::LiteralString),
            include("attribute"),
            include("numbers"),
            rule(r"'[^']'").token(T::LiteralStringChar),
            rule(r"(\w+)(\s*|[(,])").bygroups(&[E::Token(T::Name), E::UsingSelf("root")]),
            rule(r"(<>|=>|:=|[()|:;,.'])").token(T::Punctuation),
            rule(r"[*<>+=/&-]").token(T::Operator),
            rule(r"\n+").token(T::Text),
        ]),
        ("numbers", &[
            rule(r"[0-9_]+#[0-9a-f]+#").token(T::LiteralNumberHex),
            rule(r"[0-9_]+\.[0-9_]*").token(T::LiteralNumberFloat),
            rule(r"[0-9_]+").token(T::LiteralNumberInteger),
        ]),
    ],
};

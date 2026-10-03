//! Chroma's `monkeyc.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "monkeyc",
    config: ConfigDef {
        name: "MonkeyC",
        aliases: &["monkeyc"],
        filenames: &["*.mc"],
        mime_types: &["text/x-monkeyc"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("class", &[
            rule(r"([a-zA-Z_][\w_\.]*)(?:(\s+)(extends)(\s+)([a-zA-Z_][\w_\.]*))?").groups(&[T::NameClass, T::Text, T::KeywordDeclaration, T::Text, T::NameClass]),
            rule("").pop(1),
        ]),
        ("function", &[
            rule(r"initialize").token(T::NameFunctionMagic),
            rule(r"[a-zA-Z_][\w_\.]*").token(T::NameFunction),
            rule("").pop(1),
        ]),
        ("module", &[
            rule(r"[a-zA-Z_][\w_\.]*").token(T::NameNamespace),
            rule("").pop(1),
        ]),
        ("root", &[
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"\n").token(T::Text),
            rule(r"//(\n|[\w\W]*?[^\\]\n)").token(T::CommentSingle),
            rule(r"/(\\\n)?[*][\w\W]*?[*](\\\n)?/").token(T::CommentMultiline),
            rule(r"/(\\\n)?[*][\w\W]*").token(T::CommentMultiline),
            rule(r":[a-zA-Z_][\w_\.]*").token(T::LiteralStringSymbol),
            rule(r"[{}\[\]\(\),;:\.]").token(T::Punctuation),
            rule(r"[&~\|\^!+\-*\/%=?]").token(T::Operator),
            rule(r"=>|[+-]=|&&|\|\||>>|<<|[<>]=?|[!=]=").token(T::Operator),
            rule(r"\b(and|or|instanceof|has|extends|new)").token(T::OperatorWord),
            rule(r"(false|null|true|NaN)\b").token(T::KeywordConstant),
            rule(r"(using)((?:\s|\\\\s)+)").groups(&[T::KeywordNamespace, T::Text]).push(&["import"]),
            rule(r"(class)((?:\s|\\\\s)+)").groups(&[T::KeywordDeclaration, T::Text]).push(&["class"]),
            rule(r"(function)((?:\s|\\\\s)+)").groups(&[T::KeywordDeclaration, T::Text]).push(&["function"]),
            rule(r"(module)((?:\s|\\\\s)+)").groups(&[T::KeywordDeclaration, T::Text]).push(&["module"]),
            rule(r"\b(if|else|for|switch|case|while|break|continue|default|do|try|catch|finally|return|throw|extends|function)\b").token(T::Keyword),
            rule(r"\b(const|enum|hidden|public|protected|private|static)\b").token(T::KeywordType),
            rule(r"\bvar\b").token(T::KeywordDeclaration),
            rule(r"\b(Activity(Monitor|Recording)?|Ant(Plus)?|Application|Attention|Background|Communications|Cryptography|FitContributor|Graphics|Gregorian|Lang|Math|Media|Persisted(Content|Locations)|Position|Properties|Sensor(History|Logging)?|Storage|StringUtil|System|Test|Time(r)?|Toybox|UserProfile|WatchUi|Rez|Drawables|Strings|Fonts|method)\b").token(T::NameBuiltin),
            rule(r"\b(me|self|\$)\b").token(T::NameBuiltinPseudo),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r"'(\\\\|\\'|[^''])*'").token(T::LiteralStringSingle),
            rule(r"-?(0x[0-9a-fA-F]+l?)").token(T::LiteralNumberHex),
            rule(r"-?([0-9]+(\.[0-9]+[df]?|[df]))\b").token(T::LiteralNumberFloat),
            rule(r"-?([0-9]+l?)").token(T::LiteralNumberInteger),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
        ]),
        ("import", &[
            rule(r"([a-zA-Z_][\w_\.]*)(?:(\s+)(as)(\s+)([a-zA-Z_][\w_]*))?").groups(&[T::NameNamespace, T::Text, T::KeywordNamespace, T::Text, T::NameNamespace]),
            rule("").pop(1),
        ]),
    ],
};

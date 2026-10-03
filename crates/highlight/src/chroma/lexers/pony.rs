//! Chroma's `pony.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "pony",
    config: ConfigDef {
        name: "Pony",
        aliases: &["pony"],
        filenames: &["*.pony"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("string", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"\\""#).token(T::LiteralString),
            rule(r#"[^\\"]+"#).token(T::LiteralString),
        ]),
        ("root", &[
            rule(r"\n").token(T::Text),
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"//.*\n").token(T::CommentSingle),
            rule(r"/\*").token(T::CommentMultiline).push(&["nested_comment"]),
            rule(r#""""(?:.|\n)*?""""#).token(T::LiteralStringDoc),
            rule(r#"""#).token(T::LiteralString).push(&["string"]),
            rule(r"\'.*\'").token(T::LiteralStringChar),
            rule(r"=>|[]{}:().~;,|&!^?[]").token(T::Punctuation),
            rule(r"(addressof|digestof|consume|isnt|and|not|as|is|or)\b").token(T::OperatorWord),
            rule(r"!=|==|<<|>>|[-+/*%=<>]").token(T::Operator),
            rule(r"(compile_intrinsic|compile_error|continue|recover|return|repeat|lambda|elseif|object|#share|match|#send|#read|ifdef|until|embed|while|where|error|break|with|else|#any|this|then|tag|for|trn|try|ref|use|var|val|let|end|iso|box|in|if|do)\b").token(T::Keyword),
            rule(r"(actor|class|struct|primitive|interface|trait|type)((?:\s)+)").groups(&[T::Keyword, T::Text]).push(&["typename"]),
            rule(r"(new|fun|be)((?:\s)+)").groups(&[T::Keyword, T::Text]).push(&["methodname"]),
            rule(r"(DisposableActor|NullablePointer|AsioEventNotify|UnsignedInteger|RuntimeOptions|DoNotOptimise|FloatingPoint|SignedInteger|ReadElement|ArrayValues|StringBytes|StringRunes|InputNotify|InputStream|AsioEventID|ByteSeqIter|AmbientAuth|Comparable|ArrayPairs|Stringable|OutStream|SourceLoc|ArrayKeys|StdStream|Equatable|AsioEvent|Iterator|Platform|Unsigned|Greater|Compare|Integer|Pointer|ReadSeq|ByteSeq|String|Number|Signed|Float|USize|Stdin|ILong|ISize|HasEq|Array|ULong|Equal|I128|U128|Bool|Less|Real|None|Seq|I64|Any|F32|F64|U64|U32|I32|Int|I16|U16|Env|I8|U8)\b").token(T::KeywordType),
            rule(r"_?[A-Z]\w*").token(T::NameClass),
            rule(r"string\(\)").token(T::NameOther),
            rule(r"(\d+\.\d*|\.\d+|\d+)[eE][+-]?\d+").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"\d+").token(T::LiteralNumberInteger),
            rule(r"(true|false)\b").token(T::Keyword),
            rule(r"_\d*").token(T::Name),
            rule(r"_?[a-z][\w\'_]*").token(T::Name),
        ]),
        ("typename", &[
            rule(r"(iso|trn|ref|val|box|tag)?((?:\s)*)(_?[A-Z]\w*)").groups(&[T::Keyword, T::Text, T::NameClass]).pop(1),
        ]),
        ("methodname", &[
            rule(r"(iso|trn|ref|val|box|tag)?((?:\s)*)(_?[a-z]\w*)").groups(&[T::Keyword, T::Text, T::NameFunction]).pop(1),
        ]),
        ("nested_comment", &[
            rule(r"[^*/]+").token(T::CommentMultiline),
            rule(r"/\*").token(T::CommentMultiline).push(&[]),
            rule(r"\*/").token(T::CommentMultiline).pop(1),
            rule(r"[*/]").token(T::CommentMultiline),
        ]),
    ],
};

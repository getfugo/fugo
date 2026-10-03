//! Chroma's `plutus_core.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "plutus_core",
    config: ConfigDef {
        name: "Plutus Core",
        aliases: &["plutus-core", "plc"],
        filenames: &["*.plc"],
        mime_types: &["text/x-plutus-core", "application/x-plutus-core"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::Text),
            rule(r"(\(|\))").token(T::Punctuation),
            rule(r"(\[|\])").token(T::Punctuation),
            rule(r"({|})").token(T::Punctuation),
            rule(r"([+-]?\d+)").token(T::LiteralNumberInteger),
            rule(r"(#([a-fA-F0-9][a-fA-F0-9])+)").token(T::LiteralString),
            rule(r"(\(\))").token(T::NameConstant),
            rule(r"(True|False)").token(T::NameConstant),
            rule(r"(con |abs |iwrap |unwrap |lam |builtin |delay |force |error)").token(T::Keyword),
            rule(r"(fun |all |ifix |lam |con )").token(T::Keyword),
            rule(r"(type|fun )").token(T::Keyword),
            rule(r"(program )(\S+)").groups(&[T::Keyword, T::LiteralString]),
            rule(r"(unit|bool|integer|bytestring|string)").token(T::KeywordType),
            rule(r"(addInteger |subtractInteger |multiplyInteger |divideInteger |quotientInteger |remainderInteger |modInteger |equalsInteger |lessThanInteger |lessThanEqualsInteger )").token(T::NameBuiltin),
            rule(r"(appendByteString |consByteString |sliceByteString |lengthOfByteString |indexByteString |equalsByteString |lessThanByteString |lessThanEqualsByteString )").token(T::NameBuiltin),
            rule(r"(sha2_256 |sha3_256 |blake2b_256 |verifySignature )").token(T::NameBuiltin),
            rule(r"(appendString |equalsString |encodeUtf8 |decodeUtf8 )").token(T::NameBuiltin),
            rule(r"(ifThenElse )").token(T::NameBuiltin),
            rule(r"(chooseUnit )").token(T::NameBuiltin),
            rule(r"(trace )").token(T::NameBuiltin),
            rule(r"(fstPair |sndPair )").token(T::NameBuiltin),
            rule(r"(chooseList |mkCons |headList |tailList |nullList )").token(T::NameBuiltin),
            rule(r"(chooseData |constrData |mapData |listData |iData |bData |unConstrData |unMapData |unListData |unIData |unBData |equalsData )").token(T::NameBuiltin),
            rule(r"(mkPairData |mkNilData |mkNilPairData )").token(T::NameBuiltin),
            rule(r"([a-zA-Z][a-zA-Z0-9_']*)").token(T::Name),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["string"]),
        ]),
        ("string", &[
            rule(r#"[^\\"]+"#).token(T::LiteralStringDouble),
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
        ]),
    ],
};

//! Chroma's `onesenterprise.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "onesenterprise",
    config: ConfigDef {
        name: "OnesEnterprise",
        aliases: &["ones", "onesenterprise", "1S", "1S:Enterprise"],
        filenames: &["*.EPF", "*.epf", "*.ERF", "*.erf"],
        mime_types: &["application/octet-stream"],
        case_insensitive: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\n").token(T::Text),
            rule(r"\s+").token(T::Text),
            rule(r"\\\n").token(T::Text),
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"//(.*?)\n").token(T::Comment),
            rule(r"(#область|#region|#конецобласти|#endregion|#если|#if|#иначе|#else|#конецесли|#endif).*").token(T::CommentPreproc),
            rule(r"(&наклиенте|&atclient|&насервере|&atserver|&насерверебезконтекста|&atservernocontext|&наклиентенасерверебезконтекста|&atclientatservernocontext).*").token(T::CommentPreproc),
            rule(r"(>=|<=|<>|\+|-|=|>|<|\*|/|%)").token(T::Operator),
            rule(r"(;|,|\)|\(|\.)").token(T::Punctuation),
            rule(r"(истина|ложь|или|false|true|не|and|not|и|or)\b").token(T::Operator),
            rule(r"(иначеесли|конецесли|иначе|тогда|если|elsif|endif|else|then|if)\b").token(T::Operator),
            rule(r"(конеццикла|каждого|цикл|пока|для|while|enddo|по|each|из|for|do|in|to)\b").token(T::Operator),
            rule(r"(продолжить|прервать|возврат|перейти|continue|return|break|goto)\b").token(T::Operator),
            rule(r"(конецпроцедуры|конецфункции|процедура|функция|endprocedure|endfunction|procedure|function)\b").token(T::Keyword),
            rule(r"(экспорт|новый|перем|знач|export|new|val|var)\b").token(T::Keyword),
            rule(r"(вызватьисключение|конецпопытки|исключение|попытка|endtry|except|raise|try)\b").token(T::Keyword),
            rule(r"(выполнить|вычислить|execute|eval)\b").token(T::Keyword),
            rule(r#"""#).token(T::LiteralString).push(&["string"]),
            rule(r"[_а-яА-Я0-9][а-яА-Я0-9]*").token(T::Name),
            rule(r"[_\w][\w]*").token(T::Name),
        ]),
        ("string", &[
            rule(r#""""#).token(T::LiteralString),
            rule(r#""C?"#).token(T::LiteralString).pop(1),
            rule(r#"[^"]+"#).token(T::LiteralString),
        ]),
    ],
};

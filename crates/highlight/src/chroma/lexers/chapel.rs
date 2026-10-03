//! Chroma's `chapel.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "chapel",
    config: ConfigDef {
        name: "Chapel",
        aliases: &["chapel", "chpl"],
        filenames: &["*.chpl"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("procname", &[
            rule(r"([a-zA-Z_][.\w$]*|\~[a-zA-Z_][.\w$]*|[+*/!~%<>=&^|\-:]{1,2})").token(T::NameFunction).pop(1),
            rule(r"\(").token(T::Punctuation).push(&["receivertype"]),
            rule(r"\)+\.").token(T::Punctuation),
        ]),
        ("receivertype", &[
            rule(r"(unmanaged|borrowed|atomic|single|shared|owned|sync)\b").token(T::Keyword),
            rule(r"(complex|nothing|opaque|string|locale|bytes|range|imag|real|bool|uint|void|int)\b").token(T::KeywordType),
            rule(r"[^()]*").token(T::NameOther).pop(1),
        ]),
        ("root", &[
            rule(r"\n").token(T::TextWhitespace),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"\\\n").token(T::Text),
            rule(r"//(.*?)\n").token(T::CommentSingle),
            rule(r"/(\\\n)?[*](.|\n)*?[*](\\\n)?/").token(T::CommentMultiline),
            rule(r"(config|const|inout|param|type|out|ref|var|in)\b").token(T::KeywordDeclaration),
            rule(r"(false|none|true|nil)\b").token(T::KeywordConstant),
            rule(r"(complex|nothing|opaque|string|locale|bytes|range|imag|real|bool|uint|void|int)\b").token(T::KeywordType),
            rule(r"(implements|forwarding|prototype|otherwise|subdomain|primitive|unmanaged|override|borrowed|lifetime|coforall|continue|private|require|dmapped|cobegin|foreach|lambda|sparse|shared|domain|pragma|reduce|except|export|extern|throws|forall|delete|return|noinit|single|import|select|public|inline|serial|atomic|defer|break|local|index|throw|catch|label|begin|where|while|align|yield|owned|only|this|sync|with|scan|else|enum|init|when|then|let|for|try|use|new|zip|if|by|as|on|do)\b").token(T::Keyword),
            rule(r"(iter)(\s+)").groups(&[T::Keyword, T::TextWhitespace]).push(&["procname"]),
            rule(r"(proc)(\s+)").groups(&[T::Keyword, T::TextWhitespace]).push(&["procname"]),
            rule(r"(operator)(\s+)").groups(&[T::Keyword, T::TextWhitespace]).push(&["procname"]),
            rule(r"(class|interface|module|record|union)(\s+)").groups(&[T::Keyword, T::TextWhitespace]).push(&["classname"]),
            rule(r"\d+i").token(T::LiteralNumber),
            rule(r"\d+\.\d*([Ee][-+]\d+)?i").token(T::LiteralNumber),
            rule(r"\.\d+([Ee][-+]\d+)?i").token(T::LiteralNumber),
            rule(r"\d+[Ee][-+]\d+i").token(T::LiteralNumber),
            rule(r"(\d*\.\d+)([eE][+-]?[0-9]+)?i?").token(T::LiteralNumberFloat),
            rule(r"\d+[eE][+-]?[0-9]+i?").token(T::LiteralNumberFloat),
            rule(r"0[bB][01]+").token(T::LiteralNumberBin),
            rule(r"0[xX][0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"0[oO][0-7]+").token(T::LiteralNumberOct),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            rule(r"'(\\\\|\\'|[^'])*'").token(T::LiteralString),
            rule(r"(=|\+=|-=|\*=|/=|\*\*=|%=|&=|\|=|\^=|&&=|\|\|=|<<=|>>=|<=>|<~>|\.\.|by|#|\.\.\.|&&|\|\||!|&|\||\^|~|<<|>>|==|!=|<=|>=|<|>|[+\-*/%]|\*\*)").token(T::Operator),
            rule(r"[:;,.?()\[\]{}]").token(T::Punctuation),
            rule(r"[a-zA-Z_][\w$]*").token(T::NameOther),
        ]),
        ("classname", &[
            rule(r"[a-zA-Z_][\w$]*").token(T::NameClass).pop(1),
        ]),
    ],
};

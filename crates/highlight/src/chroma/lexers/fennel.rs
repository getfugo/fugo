//! Chroma's `fennel.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "fennel",
    config: ConfigDef {
        name: "Fennel",
        aliases: &["fennel", "fnl"],
        filenames: &["*.fennel"],
        mime_types: &["text/x-fennel", "application/x-fennel"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r";.*$").token(T::CommentSingle),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"-?\d+\.\d+").token(T::LiteralNumberFloat),
            rule(r"-?\d+").token(T::LiteralNumberInteger),
            rule(r"0x-?[abcdef\d]+").token(T::LiteralNumberHex),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            rule(r"'(?!#)[\w!$%*+<=>?/.#-]+").token(T::LiteralStringSymbol),
            rule(r"\\(.|[a-z]+)").token(T::LiteralStringChar),
            rule(r"::?#?(?!#)[\w!$%*+<=>?/.#-]+").token(T::LiteralStringSymbol),
            rule(r"~@|[`\'#^~&@]").token(T::Operator),
            rule(r"(require-macros|set-forcibly!|import-macros|eval-compiler|pick-values|accumulate|macrodebug|pick-args|with-open|icollect|partial|comment|include|collect|hashfn|rshift|values|length|lshift|quote|match|while|doto|band|when|bnot|bxor|not=|tset|-\?>>|each|->>|let|doc|for|and|set|not|-\?>|bor|lua|\?\.|do|>=|<=|//|\.\.|->|or|if|~=|\^|>|=|<|:|/|\.|-|\+|\*|%|#) ").token(T::Keyword),
            rule(r"(global|lambda|macros|local|macro|var|fn|λ) ").token(T::KeywordDeclaration),
            rule(r"(debug\.setuservalue|debug\.getmetatable|debug\.getuservalue|package\.searchpath|debug\.setmetatable|debug\.upvaluejoin|debug\.getregistry|coroutine\.running|coroutine\.create|debug\.setupvalue|debug\.getupvalue|coroutine\.status|coroutine\.resume|debug\.upvalueid|package\.loadlib|debug\.traceback|math\.randomseed|coroutine\.yield|collectgarbage|debug\.getlocal|package\.seeall|string\.reverse|coroutine\.wrap|debug\.setlocal|bit32\.replace|bit32\.lrotate|debug\.gethook|debug\.getinfo|bit32\.extract|string\.gmatch|string\.format|bit32\.arshift|bit32\.rrotate|debug\.sethook|table\.concat|os\.setlocale|table\.remove|string\.lower|bit32\.rshift|bit32\.lshift|string\.match|table\.unpack|setmetatable|getmetatable|table\.insert|string\.upper|string\.byte|debug\.debug|string\.gsub|bit32\.btest|math\.random|string\.find|string\.dump|os\.difftime|string\.char|table\.sort|loadstring|io\.tmpfile|bit32\.band|bit32\.bnot|string\.sub|os\.execute|os\.tmpname|table\.maxn|math\.log10|math\.atan2|table\.pack|math\.frexp|math\.ldexp|bit32\.bxor|string\.len|math\.floor|string\.rep|coroutine|math\.cosh|math\.ceil|math\.atan|math\.asin|math\.acos|math\.modf|os\.rename|os\.remove|io\.output|os\.getenv|bit32\.bor|math\.sinh|math\.fmod|math\.tanh|math\.sqrt|math\.cos|math\.tan|io\.lines|os\.clock|tostring|io\.input|math\.sin|tonumber|loadfile|math\.rad|math\.pow|io\.flush|math\.abs|math\.min|rawequal|math\.max|math\.log|io\.close|io\.popen|math\.exp|math\.deg|io\.write|os\.time|io\.read|io\.open|require|os\.exit|os\.date|package|io\.type|module|select|rawset|rawlen|rawget|unpack|assert|dofile|ipairs|string|xpcall|table|pcall|bit32|print|debug|error|pairs|math|type|next|load|arg|io|os|_G) ").token(T::NameBuiltin),
            rule(r"(?<=\()(?!#)[\w!$%*+<=>?/.#-]+").token(T::NameFunction),
            rule(r"(?!#)[\w!$%*+<=>?/.#-]+").token(T::NameVariable),
            rule(r"(\[|\])").token(T::Punctuation),
            rule(r"(\{|\})").token(T::Punctuation),
            rule(r"(\(|\))").token(T::Punctuation),
        ]),
    ],
};

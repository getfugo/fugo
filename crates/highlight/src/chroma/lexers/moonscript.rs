//! Chroma's `moonscript.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "moonscript",
    config: ConfigDef {
        name: "MoonScript",
        aliases: &["moonscript", "moon"],
        filenames: &["*.moon"],
        mime_types: &["text/x-moonscript", "application/x-moonscript"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"#!(.*?)$").token(T::CommentPreproc),
            rule("").push(&["base"]),
        ]),
        ("base", &[
            rule(r"--.*$").token(T::CommentSingle),
            rule(r"(?i)(\d*\.\d+|\d+\.\d*)(e[+-]?\d+)?").token(T::LiteralNumberFloat),
            rule(r"(?i)\d+e[+-]?\d+").token(T::LiteralNumberFloat),
            rule(r"(?i)0x[0-9a-f]*").token(T::LiteralNumberHex),
            rule(r"\d+").token(T::LiteralNumberInteger),
            rule(r"\n").token(T::TextWhitespace),
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"(?s)\[(=*)\[.*?\]\1\]").token(T::LiteralString),
            rule(r"(->|=>)").token(T::NameFunction),
            rule(r":[a-zA-Z_]\w*").token(T::NameVariable),
            rule(r"(==|!=|~=|<=|>=|\.\.\.|\.\.|[=+\-*/%^<>#!.\\:])").token(T::Operator),
            rule(r"[;,]").token(T::Punctuation),
            rule(r"[\[\]{}()]").token(T::KeywordType),
            rule(r"[a-zA-Z_]\w*:").token(T::NameVariable),
            rule(r"(class|extends|if|then|super|do|with|import|export|while|elseif|return|for|in|from|when|using|else|switch|break)\b").token(T::Keyword),
            rule(r"(true|false|nil)\b").token(T::KeywordConstant),
            rule(r"(and|or|not)\b").token(T::OperatorWord),
            rule(r"(self)\b").token(T::NameBuiltinPseudo),
            rule(r"@@?([a-zA-Z_]\w*)?").token(T::NameVariableClass),
            rule(r"[A-Z]\w*").token(T::NameClass),
            rule(r"(_G|_VERSION|assert|collectgarbage|dofile|error|getmetatable|ipairs|load|loadfile|next|pairs|pcall|print|rawequal|rawget|rawlen|rawset|select|setmetatable|tonumber|tostring|type|warn|xpcall|bit32\.arshift|bit32\.band|bit32\.bnot|bit32\.bor|bit32\.btest|bit32\.bxor|bit32\.extract|bit32\.lrotate|bit32\.lshift|bit32\.replace|bit32\.rrotate|bit32\.rshift|coroutine\.close|coroutine\.create|coroutine\.isyieldable|coroutine\.resume|coroutine\.running|coroutine\.status|coroutine\.wrap|coroutine\.yield|debug\.debug|debug\.gethook|debug\.getinfo|debug\.getlocal|debug\.getmetatable|debug\.getregistry|debug\.getupvalue|debug\.getuservalue|debug\.sethook|debug\.setlocal|debug\.setmetatable|debug\.setupvalue|debug\.setuservalue|debug\.traceback|debug\.upvalueid|debug\.upvaluejoin|io\.close|io\.flush|io\.input|io\.lines|io\.open|io\.output|io\.popen|io\.read|io\.stderr|io\.stdin|io\.stdout|io\.tmpfile|io\.type|io\.write|math\.abs|math\.acos|math\.asin|math\.atan|math\.atan2|math\.ceil|math\.cos|math\.cosh|math\.deg|math\.exp|math\.floor|math\.fmod|math\.frexp|math\.huge|math\.ldexp|math\.log|math\.max|math\.maxinteger|math\.min|math\.mininteger|math\.modf|math\.pi|math\.pow|math\.rad|math\.random|math\.randomseed|math\.sin|math\.sinh|math\.sqrt|math\.tan|math\.tanh|math\.tointeger|math\.type|math\.ult|package\.config|package\.cpath|package\.loaded|package\.loadlib|package\.path|package\.preload|package\.searchers|package\.searchpath|require|os\.clock|os\.date|os\.difftime|os\.execute|os\.exit|os\.getenv|os\.remove|os\.rename|os\.setlocale|os\.time|os\.tmpname|string\.byte|string\.char|string\.dump|string\.find|string\.format|string\.gmatch|string\.gsub|string\.len|string\.lower|string\.match|string\.pack|string\.packsize|string\.rep|string\.reverse|string\.sub|string\.unpack|string\.upper|table\.concat|table\.insert|table\.move|table\.pack|table\.remove|table\.sort|table\.unpack|utf8\.char|utf8\.charpattern|utf8\.codepoint|utf8\.codes|utf8\.len|utf8\.offset)\b").token(T::NameBuiltin),
            rule(r"[A-Za-z_]\w*").token(T::Name),
            rule(r"'").token(T::LiteralStringSingle).combined(&["stringescape", "sqs"]),
            rule(r#"""#).token(T::LiteralStringDouble).combined(&["stringescape", "dqs"]),
        ]),
        ("stringescape", &[
            rule(r#"\\([abfnrtv\\"\']|\d{1,3})"#).token(T::LiteralStringEscape),
        ]),
        ("sqs", &[
            rule(r"'").token(T::LiteralStringSingle).pop(1),
            rule(r"[^\\']+").token(T::LiteralString),
        ]),
        ("dqs", &[
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            rule(r#"[^\\"]+"#).token(T::LiteralString),
        ]),
        ("ws", &[
            rule(r"(?:--\[(?<level>=*)\[[\w\W]*?\](\k<level>)\])").token(T::CommentMultiline),
            rule(r"(?:--.*$)").token(T::CommentSingle),
            rule(r"(?:\s+)").token(T::TextWhitespace),
        ]),
        ("varname", &[
            include("ws"),
            rule(r"\.\.").token(T::Operator).pop(1),
            rule(r"[.:]").token(T::Punctuation),
            rule(r"(?:[^\W\d]\w*)(?=(?:(?:--\[(?<level>=*)\[[\w\W]*?\](\k<level>)\])|(?:--.*$)|(?:\s+))*[.:])").token(T::NameProperty),
            rule(r"(?:[^\W\d]\w*)(?=(?:(?:--\[(?<level>=*)\[[\w\W]*?\](\k<level>)\])|(?:--.*$)|(?:\s+))*\()").token(T::NameFunction).pop(1),
            rule(r"(?:[^\W\d]\w*)").token(T::NameProperty).pop(1),
        ]),
        ("funcname", &[
            include("ws"),
            rule(r"[.:]").token(T::Punctuation),
            rule(r"(?:[^\W\d]\w*)(?=(?:(?:--\[(?<level>=*)\[[\w\W]*?\](\k<level>)\])|(?:--.*$)|(?:\s+))*[.:])").token(T::NameClass),
            rule(r"(?:[^\W\d]\w*)").token(T::NameFunction).pop(1),
            rule(r"\(").token(T::Punctuation).pop(1),
        ]),
        ("goto", &[
            include("ws"),
            rule(r"(?:[^\W\d]\w*)").token(T::NameLabel).pop(1),
        ]),
        ("label", &[
            include("ws"),
            rule(r"::").token(T::Punctuation).pop(1),
            rule(r"(?:[^\W\d]\w*)").token(T::NameLabel),
        ]),
    ],
};

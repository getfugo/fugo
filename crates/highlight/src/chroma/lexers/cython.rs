//! Chroma's `cython.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "cython",
    config: ConfigDef {
        name: "Cython",
        aliases: &["cython", "pyx", "pyrex"],
        filenames: &["*.pyx", "*.pxd", "*.pxi"],
        mime_types: &["text/x-cython", "application/x-cython"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("funcname", &[
            rule(r"[a-zA-Z_]\w*").token(T::NameFunction).pop(1),
        ]),
        ("root", &[
            rule(r"\n").token(T::Text),
            rule(r#"^(\s*)("""(?:.|\n)*?""")"#).groups(&[T::Text, T::LiteralStringDoc]),
            rule(r"^(\s*)('''(?:.|\n)*?''')").groups(&[T::Text, T::LiteralStringDoc]),
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"#.*$").token(T::Comment),
            rule(r"[]{}:(),;[]").token(T::Punctuation),
            rule(r"\\\n").token(T::Text),
            rule(r"\\").token(T::Text),
            rule(r"(in|is|and|or|not)\b").token(T::OperatorWord),
            rule(r"(<)([a-zA-Z0-9.?]+)(>)").groups(&[T::Punctuation, T::KeywordType, T::Punctuation]),
            rule(r"!=|==|<<|>>|[-~+/*%=<>&^|.?]").token(T::Operator),
            rule(r"(from)(\d+)(<=)(\s+)(<)(\d+)(:)").groups(&[T::Keyword, T::LiteralNumberInteger, T::Operator, T::Name, T::Operator, T::Name, T::Punctuation]),
            include("keywords"),
            rule(r"(def|property)(\s+)").groups(&[T::Keyword, T::Text]).push(&["funcname"]),
            rule(r"(cp?def)(\s+)").groups(&[T::Keyword, T::Text]).push(&["cdef"]),
            rule(r"(cdef)(:)").groups(&[T::Keyword, T::Punctuation]),
            rule(r"(class|struct)(\s+)").groups(&[T::Keyword, T::Text]).push(&["classname"]),
            rule(r"(from)(\s+)").groups(&[T::Keyword, T::Text]).push(&["fromimport"]),
            rule(r"(c?import)(\s+)").groups(&[T::Keyword, T::Text]).push(&["import"]),
            include("builtins"),
            include("backtick"),
            rule(r#"(?:[rR]|[uU][rR]|[rR][uU])""""#).token(T::LiteralString).push(&["tdqs"]),
            rule(r"(?:[rR]|[uU][rR]|[rR][uU])'''").token(T::LiteralString).push(&["tsqs"]),
            rule(r#"(?:[rR]|[uU][rR]|[rR][uU])""#).token(T::LiteralString).push(&["dqs"]),
            rule(r"(?:[rR]|[uU][rR]|[rR][uU])'").token(T::LiteralString).push(&["sqs"]),
            rule(r#"[uU]?""""#).token(T::LiteralString).combined(&["stringescape", "tdqs"]),
            rule(r"[uU]?'''").token(T::LiteralString).combined(&["stringescape", "tsqs"]),
            rule(r#"[uU]?""#).token(T::LiteralString).combined(&["stringescape", "dqs"]),
            rule(r"[uU]?'").token(T::LiteralString).combined(&["stringescape", "sqs"]),
            include("name"),
            include("numbers"),
        ]),
        ("stringescape", &[
            rule(r#"\\([\\abfnrtv"\']|\n|N\{.*?\}|u[a-fA-F0-9]{4}|U[a-fA-F0-9]{8}|x[a-fA-F0-9]{2}|[0-7]{1,3})"#).token(T::LiteralStringEscape),
        ]),
        ("strings", &[
            rule(r"%(\([a-zA-Z0-9]+\))?[-#0 +]*([0-9]+|[*])?(\.([0-9]+|[*]))?[hlL]?[E-GXc-giorsux%]").token(T::LiteralStringInterpol),
            rule(r#"[^\\\'"%\n]+"#).token(T::LiteralString),
            rule(r#"[\'"\\]"#).token(T::LiteralString),
            rule(r"%").token(T::LiteralString),
        ]),
        ("backtick", &[
            rule(r"`.*?`").token(T::LiteralStringBacktick),
        ]),
        ("numbers", &[
            rule(r"(\d+\.?\d*|\d*\.\d+)([eE][+-]?[0-9]+)?").token(T::LiteralNumberFloat),
            rule(r"0\d+").token(T::LiteralNumberOct),
            rule(r"0[xX][a-fA-F0-9]+").token(T::LiteralNumberHex),
            rule(r"\d+L").token(T::LiteralNumberIntegerLong),
            rule(r"\d+").token(T::LiteralNumberInteger),
        ]),
        ("keywords", &[
            rule(r"(continue|ctypedef|except\?|include|finally|global|return|lambda|assert|except|print|nogil|while|fused|yield|break|raise|exec|else|elif|pass|with|gil|for|try|del|by|as|if)\b").token(T::Keyword),
            rule(r"(DEF|IF|ELIF|ELSE)\b").token(T::CommentPreproc),
        ]),
        ("fromimport", &[
            rule(r"(\s+)(c?import)\b").groups(&[T::Text, T::Keyword]).pop(1),
            rule(r"[a-zA-Z_.][\w.]*").token(T::NameNamespace),
            rule("").pop(1),
        ]),
        ("nl", &[
            rule(r"\n").token(T::LiteralString),
        ]),
        ("dqs", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"\\\\|\\"|\\\n"#).token(T::LiteralStringEscape),
            include("strings"),
        ]),
        ("tsqs", &[
            rule(r"'''").token(T::LiteralString).pop(1),
            include("strings"),
            include("nl"),
        ]),
        ("import", &[
            rule(r"(\s+)(as)(\s+)").groups(&[T::Text, T::Keyword, T::Text]),
            rule(r"[a-zA-Z_][\w.]*").token(T::NameNamespace),
            rule(r"(\s*)(,)(\s*)").groups(&[T::Text, T::Operator, T::Text]),
            rule("").pop(1),
        ]),
        ("name", &[
            rule(r"@\w+").token(T::NameDecorator),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
        ]),
        ("cdef", &[
            rule(r"(public|readonly|extern|api|inline)\b").token(T::KeywordReserved),
            rule(r"(struct|enum|union|class)\b").token(T::Keyword),
            rule(r"([a-zA-Z_]\w*)(\s*)(?=[(:#=]|$)").groups(&[T::NameFunction, T::Text]).pop(1),
            rule(r"([a-zA-Z_]\w*)(\s*)(,)").groups(&[T::NameFunction, T::Text, T::Punctuation]),
            rule(r"from\b").token(T::Keyword).pop(1),
            rule(r"as\b").token(T::Keyword),
            rule(r":").token(T::Punctuation).pop(1),
            rule(r#"(?=["\'])"#).token(T::Text).pop(1),
            rule(r"[a-zA-Z_]\w*").token(T::KeywordType),
            rule(r".").token(T::Text),
        ]),
        ("classname", &[
            rule(r"[a-zA-Z_]\w*").token(T::NameClass).pop(1),
        ]),
        ("sqs", &[
            rule(r"'").token(T::LiteralString).pop(1),
            rule(r"\\\\|\\'|\\\n").token(T::LiteralStringEscape),
            include("strings"),
        ]),
        ("tdqs", &[
            rule(r#"""""#).token(T::LiteralString).pop(1),
            include("strings"),
            include("nl"),
        ]),
        ("builtins", &[
            rule(r"(?<!\.)(staticmethod|classmethod|__import__|issubclass|isinstance|basestring|bytearray|raw_input|frozenset|enumerate|property|unsigned|reversed|callable|execfile|hasattr|compile|complex|delattr|setattr|unicode|globals|getattr|reload|divmod|xrange|unichr|filter|reduce|buffer|intern|coerce|sorted|locals|object|round|input|range|super|tuple|bytes|float|slice|apply|bool|long|exit|vars|file|next|type|iter|open|dict|repr|hash|list|eval|oct|map|zip|int|hex|set|sum|chr|cmp|any|str|pow|ord|dir|len|min|all|abs|max|bin|id)\b").token(T::NameBuiltin),
            rule(r"(?<!\.)(self|None|Ellipsis|NotImplemented|False|True|NULL)\b").token(T::NameBuiltinPseudo),
            rule(r"(?<!\.)(PendingDeprecationWarning|UnicodeTranslateError|NotImplementedError|FloatingPointError|DeprecationWarning|UnicodeDecodeError|UnicodeEncodeError|UnboundLocalError|KeyboardInterrupt|ZeroDivisionError|IndentationError|EnvironmentError|OverflowWarning|ArithmeticError|RuntimeWarning|UnicodeWarning|AttributeError|AssertionError|NotImplemented|ReferenceError|StopIteration|SyntaxWarning|OverflowError|GeneratorExit|FutureWarning|BaseException|ImportWarning|StandardError|RuntimeError|UnicodeError|LookupError|ImportError|SyntaxError|MemoryError|SystemError|UserWarning|SystemExit|ValueError|IndexError|NameError|TypeError|Exception|KeyError|EOFError|TabError|OSError|Warning|IOError)\b").token(T::NameException),
        ]),
    ],
};

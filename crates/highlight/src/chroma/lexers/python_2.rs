//! Chroma's `python_2.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "python_2",
    config: ConfigDef {
        name: "Python 2",
        aliases: &["python2", "py2"],
        mime_types: &["text/x-python2", "application/x-python2"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("tdqs", &[
            rule(r#"""""#).token(T::LiteralStringDouble).pop(1),
            include("strings-double"),
            rule(r"\n").token(T::LiteralStringDouble),
        ]),
        ("name", &[
            rule(r"@[\w.]+").token(T::NameDecorator),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
        ]),
        ("magicfuncs", &[
            rule(r"(__instancecheck__|__subclasscheck__|__getattribute__|__rfloordiv__|__ifloordiv__|__setslice__|__getslice__|__contains__|__reversed__|__floordiv__|__rtruediv__|__itruediv__|__delslice__|__rlshift__|__rrshift__|__delitem__|__rdivmod__|__nonzero__|__missing__|__delattr__|__setattr__|__irshift__|__complex__|__setitem__|__getitem__|__truediv__|__unicode__|__ilshift__|__getattr__|__delete__|__coerce__|__invert__|__lshift__|__divmod__|__rshift__|__enter__|__index__|__float__|__iadd__|__rsub__|__init__|__imul__|__rpow__|__repr__|__rmul__|__isub__|__iter__|__rmod__|__ixor__|__call__|__imod__|__long__|__hash__|__rxor__|__idiv__|__iand__|__rdiv__|__ipow__|__rcmp__|__rand__|__exit__|__radd__|__str__|__cmp__|__pos__|__pow__|__oct__|__new__|__neg__|__mul__|__mod__|__set__|__xor__|__sub__|__len__|__and__|__get__|__rop__|__add__|__ior__|__div__|__iop__|__int__|__abs__|__hex__|__ror__|__del__|__eq__|__or__|__ne__|__lt__|__le__|__ge__|__gt__|__op__)\b").token(T::NameFunctionMagic),
        ]),
        ("keywords", &[
            rule(r"(yield from|continue|finally|lambda|assert|global|except|return|print|yield|while|break|raise|elif|pass|exec|else|with|try|for|del|as|if)\b").token(T::Keyword),
        ]),
        ("tsqs", &[
            rule(r"'''").token(T::LiteralStringSingle).pop(1),
            include("strings-single"),
            rule(r"\n").token(T::LiteralStringSingle),
        ]),
        ("stringescape", &[
            rule(r#"\\([\\abfnrtv"\']|\n|N\{.*?\}|u[a-fA-F0-9]{4}|U[a-fA-F0-9]{8}|x[a-fA-F0-9]{2}|[0-7]{1,3})"#).token(T::LiteralStringEscape),
        ]),
        ("numbers", &[
            rule(r"(\d+\.\d*|\d*\.\d+)([eE][+-]?[0-9]+)?j?").token(T::LiteralNumberFloat),
            rule(r"\d+[eE][+-]?[0-9]+j?").token(T::LiteralNumberFloat),
            rule(r"0[0-7]+j?").token(T::LiteralNumberOct),
            rule(r"0[bB][01]+").token(T::LiteralNumberBin),
            rule(r"0[xX][a-fA-F0-9]+").token(T::LiteralNumberHex),
            rule(r"\d+L").token(T::LiteralNumberIntegerLong),
            rule(r"\d+j?").token(T::LiteralNumberInteger),
        ]),
        ("import", &[
            rule(r"(?:[ \t]|\\\n)+").token(T::Text),
            rule(r"as\b").token(T::KeywordNamespace),
            rule(r",").token(T::Operator),
            rule(r"[a-zA-Z_][\w.]*").token(T::NameNamespace),
            rule("").pop(1),
        ]),
        ("magicvars", &[
            rule(r"(__metaclass__|__defaults__|__globals__|__closure__|__weakref__|__module__|__slots__|__class__|__bases__|__file__|__func__|__dict__|__name__|__self__|__code__|__mro__|__doc__)\b").token(T::NameVariableMagic),
        ]),
        ("fromimport", &[
            rule(r"(?:[ \t]|\\\n)+").token(T::Text),
            rule(r"import\b").token(T::KeywordNamespace).pop(1),
            rule(r"None\b").token(T::NameBuiltinPseudo).pop(1),
            rule(r"[a-zA-Z_.][\w.]*").token(T::NameNamespace),
            rule("").pop(1),
        ]),
        ("strings-single", &[
            rule(r"%(\(\w+\))?[-#0 +]*([0-9]+|[*])?(\.([0-9]+|[*]))?[hlL]?[E-GXc-giorsux%]").token(T::LiteralStringInterpol),
            rule(r#"[^\\\'"%\n]+"#).token(T::LiteralStringSingle),
            rule(r#"[\'"\\]"#).token(T::LiteralStringSingle),
            rule(r"%").token(T::LiteralStringSingle),
        ]),
        ("funcname", &[
            include("magicfuncs"),
            rule(r"[a-zA-Z_]\w*").token(T::NameFunction).pop(1),
            rule("").pop(1),
        ]),
        ("classname", &[
            rule(r"[a-zA-Z_]\w*").token(T::NameClass).pop(1),
        ]),
        ("backtick", &[
            rule(r"`.*?`").token(T::LiteralStringBacktick),
        ]),
        ("strings-double", &[
            rule(r"%(\(\w+\))?[-#0 +]*([0-9]+|[*])?(\.([0-9]+|[*]))?[hlL]?[E-GXc-giorsux%]").token(T::LiteralStringInterpol),
            rule(r#"[^\\\'"%\n]+"#).token(T::LiteralStringDouble),
            rule(r#"[\'"\\]"#).token(T::LiteralStringDouble),
            rule(r"%").token(T::LiteralStringDouble),
        ]),
        ("dqs", &[
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            rule(r#"\\\\|\\"|\\\n"#).token(T::LiteralStringEscape),
            include("strings-double"),
        ]),
        ("root", &[
            rule(r"\n").token(T::Text),
            rule(r#"^(\s*)([rRuUbB]{,2})("""(?:.|\n)*?""")"#).groups(&[T::Text, T::LiteralStringAffix, T::LiteralStringDoc]),
            rule(r"^(\s*)([rRuUbB]{,2})('''(?:.|\n)*?''')").groups(&[T::Text, T::LiteralStringAffix, T::LiteralStringDoc]),
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"\A#!.+$").token(T::CommentHashbang),
            rule(r"#.*$").token(T::CommentSingle),
            rule(r"[]{}:(),;[]").token(T::Punctuation),
            rule(r"\\\n").token(T::Text),
            rule(r"\\").token(T::Text),
            rule(r"(in|is|and|or|not)\b").token(T::OperatorWord),
            rule(r"!=|==|<<|>>|[-~+/*%=<>&^|.]").token(T::Operator),
            include("keywords"),
            rule(r"(def)((?:\s|\\\s)+)").groups(&[T::Keyword, T::Text]).push(&["funcname"]),
            rule(r"(class)((?:\s|\\\s)+)").groups(&[T::Keyword, T::Text]).push(&["classname"]),
            rule(r"(from)((?:\s|\\\s)+)").groups(&[T::KeywordNamespace, T::Text]).push(&["fromimport"]),
            rule(r"(import)((?:\s|\\\s)+)").groups(&[T::KeywordNamespace, T::Text]).push(&["import"]),
            include("builtins"),
            include("magicfuncs"),
            include("magicvars"),
            include("backtick"),
            rule(r#"([rR]|[uUbB][rR]|[rR][uUbB])(""")"#).groups(&[T::LiteralStringAffix, T::LiteralStringDouble]).push(&["tdqs"]),
            rule(r"([rR]|[uUbB][rR]|[rR][uUbB])(''')").groups(&[T::LiteralStringAffix, T::LiteralStringSingle]).push(&["tsqs"]),
            rule(r#"([rR]|[uUbB][rR]|[rR][uUbB])(")"#).groups(&[T::LiteralStringAffix, T::LiteralStringDouble]).push(&["dqs"]),
            rule(r"([rR]|[uUbB][rR]|[rR][uUbB])(')").groups(&[T::LiteralStringAffix, T::LiteralStringSingle]).push(&["sqs"]),
            rule(r#"([uUbB]?)(""")"#).groups(&[T::LiteralStringAffix, T::LiteralStringDouble]).combined(&["stringescape", "tdqs"]),
            rule(r"([uUbB]?)(''')").groups(&[T::LiteralStringAffix, T::LiteralStringSingle]).combined(&["stringescape", "tsqs"]),
            rule(r#"([uUbB]?)(")"#).groups(&[T::LiteralStringAffix, T::LiteralStringDouble]).combined(&["stringescape", "dqs"]),
            rule(r"([uUbB]?)(')").groups(&[T::LiteralStringAffix, T::LiteralStringSingle]).combined(&["stringescape", "sqs"]),
            include("name"),
            include("numbers"),
        ]),
        ("sqs", &[
            rule(r"'").token(T::LiteralStringSingle).pop(1),
            rule(r"\\\\|\\'|\\\n").token(T::LiteralStringEscape),
            include("strings-single"),
        ]),
        ("builtins", &[
            rule(r"(?<!\.)(staticmethod|classmethod|__import__|isinstance|basestring|issubclass|frozenset|raw_input|bytearray|enumerate|property|callable|reversed|execfile|hasattr|setattr|compile|complex|delattr|unicode|globals|getattr|unichr|reduce|xrange|buffer|intern|filter|locals|divmod|coerce|sorted|reload|object|slice|round|float|super|input|bytes|apply|tuple|range|iter|dict|long|type|hash|vars|next|file|exit|open|repr|eval|bool|list|bin|pow|zip|ord|oct|min|set|any|max|map|all|len|sum|int|dir|hex|chr|abs|cmp|str|id)\b").token(T::NameBuiltin),
            rule(r"(?<!\.)(self|None|Ellipsis|NotImplemented|False|True|cls)\b").token(T::NameBuiltinPseudo),
            rule(r"(?<!\.)(PendingDeprecationWarning|UnicodeTranslateError|NotImplementedError|UnicodeDecodeError|DeprecationWarning|UnicodeEncodeError|FloatingPointError|ZeroDivisionError|UnboundLocalError|KeyboardInterrupt|EnvironmentError|IndentationError|OverflowWarning|ArithmeticError|ReferenceError|AttributeError|AssertionError|RuntimeWarning|UnicodeWarning|GeneratorExit|SyntaxWarning|StandardError|BaseException|OverflowError|FutureWarning|ImportWarning|StopIteration|UnicodeError|WindowsError|RuntimeError|ImportError|UserWarning|LookupError|SyntaxError|SystemError|MemoryError|SystemExit|ValueError|IndexError|NameError|Exception|TypeError|EOFError|KeyError|VMSError|TabError|IOError|Warning|OSError)\b").token(T::NameException),
        ]),
    ],
};

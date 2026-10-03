//! Chroma's `hy.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "hy",
    config: ConfigDef {
        name: "Hy",
        aliases: &["hylang"],
        filenames: &["*.hy"],
        mime_types: &["text/x-hy", "application/x-hy"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r";.*$").token(T::CommentSingle),
            rule(r"[,\s]+").token(T::Text),
            rule(r"-?\d+\.\d+").token(T::LiteralNumberFloat),
            rule(r"-?\d+").token(T::LiteralNumberInteger),
            rule(r"0[0-7]+j?").token(T::LiteralNumberOct),
            rule(r"0[xX][a-fA-F0-9]+").token(T::LiteralNumberHex),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            rule(r"'(?!#)[\w!$%*+<=>?/.#-]+").token(T::LiteralStringSymbol),
            rule(r"\\(.|[a-z]+)").token(T::LiteralStringChar),
            rule(r#"^(\s*)([rRuU]{,2}"""(?:.|\n)*?""")"#).groups(&[T::Text, T::LiteralStringDoc]),
            rule(r"^(\s*)([rRuU]{,2}'''(?:.|\n)*?''')").groups(&[T::Text, T::LiteralStringDoc]),
            rule(r"::?(?!#)[\w!$%*+<=>?/.#-]+").token(T::LiteralStringSymbol),
            rule(r"~@|[`\'#^~&@]").token(T::Operator),
            include("py-keywords"),
            include("py-builtins"),
            rule(r"(eval-when-compile|eval-and-compile|with-decorator|unquote-splice|quasiquote|list_comp|unquote|foreach|kwapply|import|not-in|unless|is-not|quote|progn|slice|assoc|first|while|when|rest|cond|<<=|->>|for|get|>>=|let|cdr|car|is|->|do|in|\||~|,) ").token(T::Keyword),
            rule(r"(defmacro|defclass|lambda|defun|defn|setv|def|fn) ").token(T::KeywordDeclaration),
            rule(r"(repeatedly|take_while|iterator\?|iterable\?|instance\?|distinct|take_nth|numeric\?|iterate|filter|repeat|remove|even\?|none\?|cycle|zero\?|odd\?|pos\?|neg\?|take|drop|inc|dec|nth) ").token(T::NameBuiltin),
            rule(r"(?<=\()(?!#)[\w!$%*+<=>?/.#-]+").token(T::NameFunction),
            rule(r"(?!#)[\w!$%*+<=>?/.#-]+").token(T::NameVariable),
            rule(r"(\[|\])").token(T::Punctuation),
            rule(r"(\{|\})").token(T::Punctuation),
            rule(r"(\(|\))").token(T::Punctuation),
        ]),
        ("py-keywords", &[
            rule(r"(yield from|continue|finally|lambda|assert|global|except|return|print|yield|while|break|raise|elif|pass|exec|else|with|try|for|del|as|if)\b").token(T::Keyword),
        ]),
        ("py-builtins", &[
            rule(r"(?<!\.)(staticmethod|classmethod|__import__|isinstance|basestring|issubclass|frozenset|raw_input|bytearray|enumerate|property|callable|reversed|execfile|hasattr|setattr|compile|complex|delattr|unicode|globals|getattr|unichr|reduce|xrange|buffer|intern|filter|locals|divmod|coerce|sorted|reload|object|slice|round|float|super|input|bytes|apply|tuple|range|iter|dict|long|type|hash|vars|next|file|exit|open|repr|eval|bool|list|bin|pow|zip|ord|oct|min|set|any|max|map|all|len|sum|int|dir|hex|chr|abs|cmp|str|id)\b").token(T::NameBuiltin),
            rule(r"(?<!\.)(self|None|Ellipsis|NotImplemented|False|True|cls)\b").token(T::NameBuiltinPseudo),
            rule(r"(?<!\.)(PendingDeprecationWarning|UnicodeTranslateError|NotImplementedError|UnicodeEncodeError|UnicodeDecodeError|DeprecationWarning|FloatingPointError|UnboundLocalError|KeyboardInterrupt|ZeroDivisionError|EnvironmentError|IndentationError|ArithmeticError|OverflowWarning|ReferenceError|RuntimeWarning|AttributeError|AssertionError|NotImplemented|UnicodeWarning|FutureWarning|BaseException|StopIteration|SyntaxWarning|OverflowError|StandardError|ImportWarning|GeneratorExit|RuntimeError|WindowsError|UnicodeError|LookupError|SyntaxError|SystemError|ImportError|MemoryError|UserWarning|ValueError|IndexError|SystemExit|Exception|TypeError|NameError|EOFError|VMSError|KeyError|TabError|IOError|OSError|Warning)\b").token(T::NameException),
        ]),
    ],
};

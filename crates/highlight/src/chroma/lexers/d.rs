//! Chroma's `d.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "d",
    config: ConfigDef {
        name: "D",
        aliases: &["d"],
        filenames: &["*.d", "*.di"],
        mime_types: &["text/x-d"],
        ensure_nl: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
            rule(r"/\+.*?\+/").token(T::CommentMultiline),
            rule(r"(asm|assert|body|break|case|cast|catch|continue|default|debug|delete|do|else|finally|for|foreach|foreach_reverse|goto|if|in|invariant|is|macro|mixin|new|out|pragma|return|super|switch|this|throw|try|typeid|typeof|version|while|with)\b").token(T::Keyword),
            rule(r"__(FILE|FILE_FULL_PATH|MODULE|LINE|FUNCTION|PRETTY_FUNCTION|DATE|EOF|TIME|TIMESTAMP|VENDOR|VERSION)__\b").token(T::NameBuiltin),
            rule(r"__(traits|vector|parameters)\b").token(T::NameBuiltin),
            rule(r"((?:(?:[^\W\d]|\$)[\w.\[\]$<>]*\s+)+?)((?:[^\W\d]|\$)[\w$]*)(\s*)(\()").bygroups(&[E::UsingSelf("root"), E::Token(T::NameFunction), E::Token(T::Text), E::Token(T::Operator)]),
            rule(r"@[\w.]*").token(T::NameDecorator),
            rule(r"(abstract|auto|alias|align|const|delegate|deprecated|enum|export|extern|final|function|immutable|inout|lazy|nothrow|override|package|private|protected|public|pure|ref|scope|shared|static|synchronized|template|unittest|__gshared)\b").token(T::KeywordDeclaration),
            rule(r"(void|bool|byte|ubyte|short|ushort|int|uint|long|ulong|cent|ucent|float|double|real|ifloat|idouble|ireal|cfloat|cdouble|creal|char|wchar|dchar)\b").token(T::KeywordType),
            rule(r"(size_t|ptrdiff_t|noreturn|string|wstring|dstring|Object|Throwable|Exception|Error|imported)\b").token(T::NameBuiltin),
            rule(r"(module)(\s+)").groups(&[T::KeywordNamespace, T::Text]).push(&["import"]),
            rule(r"(true|false|null)\b").token(T::KeywordConstant),
            rule(r"(class|interface|struct|template|union)(\s+)").groups(&[T::KeywordDeclaration, T::Text]).push(&["class"]),
            rule(r"(import)(\s+)").groups(&[T::KeywordNamespace, T::Text]).push(&["import"]),
            rule(r#"[qr]?"(\\\\|\\"|[^"])*"[cwd]?"#).token(T::LiteralString),
            rule(r"(`)([^`]*)(`)[cwd]?").token(T::LiteralString),
            rule(r"'\\.'|'[^\\]'|'\\u[0-9a-fA-F]{4}'").token(T::LiteralStringChar),
            rule(r"(\.)((?:[^\W\d]|\$)[\w$]*)").groups(&[T::Operator, T::NameAttribute]),
            rule(r"^\s*([^\W\d]|\$)[\w$]*:").token(T::NameLabel),
            rule(r"([0-9][0-9_]*\.([0-9][0-9_]*)?|\.[0-9][0-9_]*)([eE][+\-]?[0-9][0-9_]*)?[fFL]?i?|[0-9][eE][+\-]?[0-9][0-9_]*[fFL]?|[0-9]([eE][+\-]?[0-9][0-9_]*)?[fFL]|0[xX]([0-9a-fA-F][0-9a-fA-F_]*\.?|([0-9a-fA-F][0-9a-fA-F_]*)?\.[0-9a-fA-F][0-9a-fA-F_]*)[pP][+\-]?[0-9][0-9_]*[fFL]?").token(T::LiteralNumberFloat),
            rule(r"0[xX][0-9a-fA-F][0-9a-fA-F_]*[lL]?").token(T::LiteralNumberHex),
            rule(r"0[bB][01][01_]*[lL]?").token(T::LiteralNumberBin),
            rule(r"0[0-7_]+[lL]?").token(T::LiteralNumberOct),
            rule(r"0|[1-9][0-9_]*[lL]?").token(T::LiteralNumberInteger),
            rule(r"([~^*!%&\[\](){}<>|+=:;,./?-]|q{)").token(T::Operator),
            rule(r"([^\W\d]|\$)[\w$]*").token(T::Name),
            rule(r"\n").token(T::Text),
        ]),
        ("class", &[
            rule(r"([^\W\d]|\$)[\w$]*").token(T::NameClass).pop(1),
        ]),
        ("import", &[
            rule(r"[\w.]+\*?").token(T::NameNamespace).pop(1),
        ]),
    ],
};

//! Chroma's `phtml.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "phtml",
    config: ConfigDef {
        name: "PHTML",
        aliases: &["phtml"],
        filenames: &["*.phtml", "*.php", "*.php[345]", "*.inc"],
        mime_types: &[
            "application/x-php",
            "application/x-httpd-php",
            "application/x-httpd-php3",
            "application/x-httpd-php4",
            "application/x-httpd-php5",
            "text/x-php",
        ],
        case_insensitive: true,
        dot_all: true,
        ensure_nl: true,
        priority: 2.0,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("classname", &[
            rule(r"(?:[\\_a-z]|[^\x00-\x7f])(?:[\\\w]|[^\x00-\x7f])*").token(T::NameClass).pop(1),
        ]),
        ("functionname", &[
            include("magicfuncs"),
            rule(r"(?:[\\_a-z]|[^\x00-\x7f])(?:[\\\w]|[^\x00-\x7f])*").token(T::NameFunction).pop(1),
            rule("").pop(1),
        ]),
        ("magicconstants", &[
            rule(r"(__NAMESPACE__|__FUNCTION__|__METHOD__|__CLASS__|__TRAIT__|__LINE__|__FILE__|__DIR__)\b").token(T::NameConstant),
        ]),
        ("magicfuncs", &[
            rule(r"(__callStatic|__set_state|__construct|__debugInfo|__toString|__destruct|__invoke|__wakeup|__clone|__sleep|__isset|__unset|__call|__get|__set)\b").token(T::NameFunctionMagic),
        ]),
        ("php", &[
            rule(r"\?>").token(T::CommentPreproc).pop(1),
            rule(r#"(<<<)([\'"]?)((?:[\\_a-z]|[^\x00-\x7f])(?:[\\\w]|[^\x00-\x7f])*)(\2\n.*?\n\s*)(\3)(;?)(\n)"#).groups(&[T::LiteralString, T::LiteralString, T::LiteralStringDelimiter, T::LiteralString, T::LiteralStringDelimiter, T::Punctuation, T::Text]),
            rule(r"\s+").token(T::Text),
            rule(r"#.*?\n").token(T::CommentSingle),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/\*\*/").token(T::CommentMultiline),
            rule(r"/\*\*.*?\*/").token(T::LiteralStringDoc),
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
            rule(r"(->|::)(\s*)((?:[\\_a-z]|[^\x00-\x7f])(?:[\\\w]|[^\x00-\x7f])*)").groups(&[T::Operator, T::Text, T::NameAttribute]),
            rule(r"[~!%^&*+=|:.<>/@-]+").token(T::Operator),
            rule(r"\?").token(T::Operator),
            rule(r"[\[\]{}();,]+").token(T::Punctuation),
            rule(r"(class)(\s+)").groups(&[T::Keyword, T::Text]).push(&["classname"]),
            rule(r"(function)(\s*)(?=\()").groups(&[T::Keyword, T::Text]),
            rule(r"(function)(\s+)(&?)(\s*)").groups(&[T::Keyword, T::Text, T::Operator, T::Text]).push(&["functionname"]),
            rule(r"(const)(\s+)((?:[\\_a-z]|[^\x00-\x7f])(?:[\\\w]|[^\x00-\x7f])*)").groups(&[T::Keyword, T::Text, T::NameConstant]),
            rule(r"(and|E_PARSE|old_function|E_ERROR|or|as|E_WARNING|parent|eval|PHP_OS|break|exit|case|extends|PHP_VERSION|cfunction|FALSE|print|for|require|continue|foreach|require_once|declare|return|default|static|do|switch|die|stdClass|echo|else|TRUE|elseif|var|empty|if|xor|enddeclare|include|virtual|endfor|include_once|while|endforeach|global|endif|list|endswitch|new|endwhile|not|array|E_ALL|NULL|final|php_user_filter|interface|implements|public|private|protected|abstract|clone|try|catch|throw|this|use|namespace|trait|yield|finally)\b").token(T::Keyword),
            rule(r"(true|false|null)\b").token(T::KeywordConstant),
            include("magicconstants"),
            rule(r"\$\{\$+(?:[\\_a-z]|[^\x00-\x7f])(?:[\\\w]|[^\x00-\x7f])*\}").token(T::NameVariable),
            rule(r"\$+(?:[\\_a-z]|[^\x00-\x7f])(?:[\\\w]|[^\x00-\x7f])*").token(T::NameVariable),
            rule(r"(?:[\\_a-z]|[^\x00-\x7f])(?:[\\\w]|[^\x00-\x7f])*").token(T::NameOther),
            rule(r"(\d+\.\d*|\d*\.\d+)(e[+-]?[0-9]+)?").token(T::LiteralNumberFloat),
            rule(r"\d+e[+-]?[0-9]+").token(T::LiteralNumberFloat),
            rule(r"0[0-7]+").token(T::LiteralNumberOct),
            rule(r"0x[a-f0-9_]+").token(T::LiteralNumberHex),
            rule(r"\d[\d_]*").token(T::LiteralNumberInteger),
            rule(r"0b[01]+").token(T::LiteralNumberBin),
            rule(r"'([^'\\]*(?:\\.[^'\\]*)*)'").token(T::LiteralStringSingle),
            rule(r"`([^`\\]*(?:\\.[^`\\]*)*)`").token(T::LiteralStringBacktick),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["string"]),
        ]),
        ("root", &[
            rule(r"<\?(php)?").token(T::CommentPreproc).push(&["php"]),
            rule(r"[^<]+").token(T::Other),
            rule(r"<").token(T::Other),
        ]),
        ("string", &[
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            rule(r#"[^{$"\\]+"#).token(T::LiteralStringDouble),
            rule(r#"\\([nrt"$\\]|[0-7]{1,3}|x[0-9a-f]{1,2})"#).token(T::LiteralStringEscape),
            rule(r"\$(?:[\\_a-z]|[^\x00-\x7f])(?:[\\\w]|[^\x00-\x7f])*(\[\S+?\]|->(?:[\\_a-z]|[^\x00-\x7f])(?:[\\\w]|[^\x00-\x7f])*)?").token(T::LiteralStringInterpol),
            rule(r"(\{\$\{)(.*?)(\}\})").bygroups(&[E::Token(T::LiteralStringInterpol), E::UsingSelf("root"), E::Token(T::LiteralStringInterpol)]),
            rule(r"(\{)(\$.*?)(\})").bygroups(&[E::Token(T::LiteralStringInterpol), E::UsingSelf("root"), E::Token(T::LiteralStringInterpol)]),
            rule(r"(\$\{)(\S+)(\})").groups(&[T::LiteralStringInterpol, T::NameVariable, T::LiteralStringInterpol]),
            rule(r"[${\\]").token(T::LiteralStringDouble),
        ]),
    ],
};

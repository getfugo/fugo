//! Chroma's `rpm_spec.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "rpm_spec",
    config: ConfigDef {
        name: "RPMSpec",
        aliases: &["spec"],
        filenames: &["*.spec"],
        mime_types: &["text/x-rpm-spec"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"#.*$").token(T::Comment),
            include("basic"),
        ]),
        ("description", &[
            rule(r"^(%(?:package|prep|build|install|clean|check|pre[a-z]*|post[a-z]*|trigger[a-z]*|files))(.*)$").groups(&[T::NameDecorator, T::Text]).pop(1),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r".").token(T::Text),
        ]),
        ("changelog", &[
            rule(r"\*.*$").token(T::GenericSubheading),
            rule(r"^(%(?:package|prep|build|install|clean|check|pre[a-z]*|post[a-z]*|trigger[a-z]*|files))(.*)$").groups(&[T::NameDecorator, T::Text]).pop(1),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r".").token(T::Text),
        ]),
        ("string", &[
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            rule(r#"\\([\\abfnrtv"\']|x[a-fA-F0-9]{2,4}|[0-7]{1,3})"#).token(T::LiteralStringEscape),
            include("interpol"),
            rule(r".").token(T::LiteralStringDouble),
        ]),
        ("basic", &[
            include("macro"),
            rule(r"(?i)^(Name|Version|Release|Epoch|Summary|Group|License|Packager|Vendor|Icon|URL|Distribution|Prefix|Patch[0-9]*|Source[0-9]*|Requires\(?[a-z]*\)?|[a-z]+Req|Obsoletes|Suggests|Provides|Conflicts|Build[a-z]+|[a-z]+Arch|Auto[a-z]+)(:)(.*)$").bygroups(&[E::Token(T::GenericHeading), E::Token(T::Punctuation), E::UsingSelf("root")]),
            rule(r"^%description").token(T::NameDecorator).push(&["description"]),
            rule(r"^%changelog").token(T::NameDecorator).push(&["changelog"]),
            rule(r"^(%(?:package|prep|build|install|clean|check|pre[a-z]*|post[a-z]*|trigger[a-z]*|files))(.*)$").groups(&[T::NameDecorator, T::Text]),
            rule(r"%(attr|defattr|dir|doc(?:dir)?|setup|config(?:ure)?|make(?:install)|ghost|patch[0-9]+|find_lang|exclude|verify)").token(T::Keyword),
            include("interpol"),
            rule(r"'.*?'").token(T::LiteralStringSingle),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["string"]),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r".").token(T::Text),
        ]),
        ("macro", &[
            rule(r"%define.*$").token(T::CommentPreproc),
            rule(r"%\{\!\?.*%define.*\}").token(T::CommentPreproc),
            rule(r"(%(?:if(?:n?arch)?|else(?:if)?|endif))(.*)$").groups(&[T::CommentPreproc, T::Text]),
        ]),
        ("interpol", &[
            rule(r"%\{?__[a-z_]+\}?").token(T::NameFunction),
            rule(r"%\{?_([a-z_]+dir|[a-z_]+path|prefix)\}?").token(T::KeywordPseudo),
            rule(r"%\{\?\w+\}").token(T::NameVariable),
            rule(r"\$\{?RPM_[A-Z0-9_]+\}?").token(T::NameVariableGlobal),
            rule(r"%\{[a-zA-Z]\w+\}").token(T::KeywordConstant),
        ]),
    ],
};

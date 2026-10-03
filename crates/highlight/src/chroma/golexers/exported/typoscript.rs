//! Chroma's `typoscript.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "typoscript",
    config: ConfigDef {
        name: "TypoScript",
        aliases: &["typoscript"],
        filenames: &["*.ts"],
        mime_types: &["text/x-typoscript"],
        dot_all: true,
        priority: 0.1,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("comment", &[
            rule(r#"(?<!(#|\'|"))(?:#(?!(?:[a-fA-F0-9]{6}|[a-fA-F0-9]{3}))[^\n#]+|//[^\n]*)"#).token(T::Comment),
            rule(r"/\*(?:(?!\*/).)*\*/").token(T::Comment),
            rule(r"(\s*#\s*\n)").token(T::Comment),
        ]),
        ("constant", &[
            rule(r"(\{)(\$)((?:[\w\-]+\.)*)([\w\-]+)(\})").groups(&[T::LiteralStringSymbol, T::Operator, T::NameConstant, T::NameConstant, T::LiteralStringSymbol]),
            rule(r"(\{)([\w\-]+)(\s*:\s*)([\w\-]+)(\})").groups(&[T::LiteralStringSymbol, T::NameConstant, T::Operator, T::NameConstant, T::LiteralStringSymbol]),
            rule(r"(#[a-fA-F0-9]{6}\b|#[a-fA-F0-9]{3}\b)").token(T::LiteralStringChar),
        ]),
        ("html", &[
            rule(r"<\S[^\n>]*>").using("TypoScriptHTMLData"),
            rule(r"&[^;\n]*;").token(T::LiteralString),
            rule(r"(_CSS_DEFAULT_STYLE)(\s*)(\()(?s)(.*(?=\n\)))").bygroups(&[E::Token(T::NameClass), E::Token(T::Text), E::Token(T::LiteralStringSymbol), E::Using("TypoScriptCSSData")]),
        ]),
        ("keywords", &[
            rule(r"(\[)(?i)(browser|compatVersion|dayofmonth|dayofweek|dayofyear|device|ELSE|END|GLOBAL|globalString|globalVar|hostname|hour|IP|language|loginUser|loginuser|minute|month|page|PIDinRootline|PIDupinRootline|system|treeLevel|useragent|userFunc|usergroup|version)([^\]]*)(\])").groups(&[T::LiteralStringSymbol, T::NameConstant, T::Text, T::LiteralStringSymbol]),
            rule(r"(?=[\w\-])(HTMLparser|HTMLparser_tags|addParams|cache|encapsLines|filelink|if|imageLinkWrap|imgResource|makelinks|numRows|numberFormat|parseFunc|replacement|round|select|split|stdWrap|strPad|tableStyle|tags|textStyle|typolink)(?![\w\-])").token(T::NameFunction),
            rule(r"(?:(=?\s*<?\s+|^\s*))(cObj|field|config|content|constants|FEData|file|frameset|includeLibs|lib|page|plugin|register|resources|sitemap|sitetitle|styles|temp|tt_[^:.\s]*|types|xmlnews|INCLUDE_TYPOSCRIPT|_CSS_DEFAULT_STYLE|_DEFAULT_PI_VARS|_LOCAL_LANG)(?![\w\-])").groups(&[T::Operator, T::NameBuiltin]),
            rule(r"(?=[\w\-])(CASE|CLEARGIF|COA|COA_INT|COBJ_ARRAY|COLUMNS|CONTENT|CTABLE|EDITPANEL|FILE|FILES|FLUIDTEMPLATE|FORM|HMENU|HRULER|HTML|IMAGE|IMGTEXT|IMG_RESOURCE|LOAD_REGISTER|MEDIA|MULTIMEDIA|OTABLE|PAGE|QTOBJECT|RECORDS|RESTORE_REGISTER|SEARCHRESULT|SVG|SWFOBJECT|TEMPLATE|TEXT|USER|USER_INT)(?![\w\-])").token(T::NameClass),
            rule(r"(?=[\w\-])(ACTIFSUBRO|ACTIFSUB|ACTRO|ACT|CURIFSUBRO|CURIFSUB|CURRO|CUR|IFSUBRO|IFSUB|NO|SPC|USERDEF1RO|USERDEF1|USERDEF2RO|USERDEF2|USRRO|USR)").token(T::NameClass),
            rule(r"(?=[\w\-])(GMENU_FOLDOUT|GMENU_LAYERS|GMENU|IMGMENUITEM|IMGMENU|JSMENUITEM|JSMENU|TMENUITEM|TMENU_LAYERS|TMENU)").token(T::NameClass),
            rule(r"(?=[\w\-])(PHP_SCRIPT(_EXT|_INT)?)").token(T::NameClass),
            rule(r"(?=[\w\-])(userFunc)(?![\w\-])").token(T::NameFunction),
        ]),
        ("label", &[
            rule(r#"(EXT|FILE|LLL):[^}\n"]*"#).token(T::LiteralString),
            rule(r"(?![^\w\-])([\w\-]+(?:/[\w\-]+)+/?)(\S*\n)").groups(&[T::LiteralString, T::LiteralString]),
        ]),
        ("literal", &[
            rule(r"0x[0-9A-Fa-f]+t?").token(T::LiteralNumberHex),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r"(###\w+###)").token(T::NameConstant),
        ]),
        ("operator", &[
            rule(r"[<>,:=.*%+|]").token(T::Operator),
        ]),
        ("other", &[
            rule(r#"[\w"\-!/&;]+"#).token(T::Text),
        ]),
        ("punctuation", &[
            rule(r"[,.]").token(T::Punctuation),
        ]),
        ("root", &[
            include("comment"),
            include("constant"),
            include("html"),
            include("label"),
            include("whitespace"),
            include("keywords"),
            include("punctuation"),
            include("operator"),
            include("structure"),
            include("literal"),
            include("other"),
        ]),
        ("structure", &[
            rule(r"[{}()\[\]\\]").token(T::LiteralStringSymbol),
        ]),
        ("whitespace", &[
            rule(r"\s+").token(T::Text),
        ]),
    ],
};

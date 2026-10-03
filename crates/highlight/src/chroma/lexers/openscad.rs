//! Chroma's `openscad.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "openscad",
    config: ConfigDef {
        name: "OpenSCAD",
        aliases: &["openscad"],
        filenames: &["*.scad"],
        mime_types: &["text/x-scad"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"\n").token(T::Text),
            rule(r"//(\n|[\w\W]*?[^\\]\n)").token(T::CommentSingle),
            rule(r"/(\\\n)?[*][\w\W]*?[*](\\\n)?/").token(T::CommentMultiline),
            rule(r"/(\\\n)?[*][\w\W]*").token(T::CommentMultiline),
            rule(r"[{}\[\]\(\),;:]").token(T::Punctuation),
            rule(r"[*!#%\-+=?/]").token(T::Operator),
            rule(r"<|<=|==|!=|>=|>|&&|\|\|").token(T::Operator),
            rule(r"\$(f[asn]|t|vp[rtd]|children)").token(T::NameVariableMagic),
            rule(r"(undef|PI)\b").token(T::KeywordConstant),
            rule(r"(use|include)((?:\s|\\\\s)+)").groups(&[T::KeywordNamespace, T::Text]).push(&["includes"]),
            rule(r"(module)(\s*)([^\s\(]+)").groups(&[T::KeywordNamespace, T::Text, T::NameNamespace]),
            rule(r"(function)(\s*)([^\s\(]+)").groups(&[T::KeywordDeclaration, T::Text, T::NameFunction]),
            rule(r"\b(true|false)\b").token(T::Literal),
            rule(r"\b(function|module|include|use|for|intersection_for|if|else|return)\b").token(T::Keyword),
            rule(r"\b(circle|square|polygon|text|sphere|cube|cylinder|polyhedron|translate|rotate|scale|resize|mirror|multmatrix|color|offset|hull|minkowski|union|difference|intersection|abs|sign|sin|cos|tan|acos|asin|atan|atan2|floor|round|ceil|ln|log|pow|sqrt|exp|rands|min|max|concat|lookup|str|chr|search|version|version_num|norm|cross|parent_module|echo|import|import_dxf|dxf_linear_extrude|linear_extrude|rotate_extrude|surface|projection|render|dxf_cross|dxf_dim|let|assign|len)\b").token(T::NameBuiltin),
            rule(r"\bchildren\b").token(T::NameBuiltinPseudo),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r"-?\d+(\.\d+)?(e[+-]?\d+)?").token(T::LiteralNumber),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
        ]),
        ("includes", &[
            rule(r"(<)([^>]*)(>)").groups(&[T::Punctuation, T::CommentPreprocFile, T::Punctuation]),
            rule("").pop(1),
        ]),
    ],
};

//! Chroma's `glsl.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "glsl",
    config: ConfigDef {
        name: "GLSL",
        aliases: &["glsl"],
        filenames: &["*.vert", "*.frag", "*.geo"],
        mime_types: &["text/x-glslsrc"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"^#.*").token(T::CommentPreproc),
            rule(r"//.*").token(T::CommentSingle),
            rule(r"/(\\\n)?[*](.|\n)*?[*](\\\n)?/").token(T::CommentMultiline),
            rule(r"\+|-|~|!=?|\*|/|%|<<|>>|<=?|>=?|==?|&&?|\^|\|\|?").token(T::Operator),
            rule(r"[?:]").token(T::Operator),
            rule(r"\bdefined\b").token(T::Operator),
            rule(r"[;{}(),\[\]]").token(T::Punctuation),
            rule(r"[+-]?\d*\.\d+([eE][-+]?\d+)?").token(T::LiteralNumberFloat),
            rule(r"[+-]?\d+\.\d*([eE][-+]?\d+)?").token(T::LiteralNumberFloat),
            rule(r"0[xX][0-9a-fA-F]*").token(T::LiteralNumberHex),
            rule(r"0[0-7]*").token(T::LiteralNumberOct),
            rule(r"[1-9][0-9]*").token(T::LiteralNumberInteger),
            rule(r"\b(sampler3DsamplerCube|sampler2DShadow|sampler1DShadow|invariant|sampler1D|sampler2D|attribute|mat3mat4|centroid|continue|varying|uniform|discard|mat4x4|mat3x3|mat2x3|mat4x2|mat3x2|mat2x2|mat2x4|mat3x4|struct|return|mat4x3|bvec4|false|ivec4|ivec3|const|float|inout|ivec2|break|while|bvec3|bvec2|vec3|else|true|void|bool|vec2|vec4|mat2|for|out|int|in|do|if)\b").token(T::Keyword),
            rule(r"\b(sampler2DRectShadow|sampler2DRect|sampler3DRect|namespace|precision|interface|volatile|template|unsigned|external|noinline|mediump|typedef|default|switch|static|extern|inline|sizeof|output|packed|double|public|fvec3|class|union|short|highp|fixed|input|fvec4|hvec2|hvec3|hvec4|dvec2|dvec3|dvec4|fvec2|using|long|this|enum|lowp|cast|goto|half|asm)\b").token(T::Keyword),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
            rule(r"\.").token(T::Punctuation),
            rule(r"\s+").token(T::Text),
        ]),
    ],
};

//! Chroma's `hlsl.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "hlsl",
    config: ConfigDef {
        name: "HLSL",
        aliases: &["hlsl"],
        filenames: &["*.hlsl", "*.hlsli", "*.cginc", "*.fx", "*.fxh"],
        mime_types: &["text/x-hlsl"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"^#.*$").token(T::CommentPreproc),
            rule(r"//.*$").token(T::CommentSingle),
            rule(r"/(\\\n)?[*](.|\n)*?[*](\\\n)?/").token(T::CommentMultiline),
            rule(r"\+|-|~|!=?|\*|/|%|<<|>>|<=?|>=?|==?|&&?|\^|\|\|?").token(T::Operator),
            rule(r"[?:]").token(T::Operator),
            rule(r"\bdefined\b").token(T::Operator),
            rule(r"[;{}(),.\[\]]").token(T::Punctuation),
            rule(r"[+-]?\d*\.\d+([eE][-+]?\d+)?f?").token(T::LiteralNumberFloat),
            rule(r"[+-]?\d+\.\d*([eE][-+]?\d+)?f?").token(T::LiteralNumberFloat),
            rule(r"0[xX][0-9a-fA-F]*").token(T::LiteralNumberHex),
            rule(r"0[0-7]*").token(T::LiteralNumberOct),
            rule(r"[1-9][0-9]*").token(T::LiteralNumberInteger),
            rule(r#"""#).token(T::LiteralString).push(&["string"]),
            rule(r"\b(asm|asm_fragment|break|case|cbuffer|centroid|class|column_major|compile|compile_fragment|const|continue|default|discard|do|else|export|extern|for|fxgroup|globallycoherent|groupshared|if|in|inline|inout|interface|line|lineadj|linear|namespace|nointerpolation|noperspective|NULL|out|packoffset|pass|pixelfragment|point|precise|return|register|row_major|sample|sampler|shared|stateblock|stateblock_state|static|struct|switch|tbuffer|technique|technique10|technique11|texture|typedef|triangle|triangleadj|uniform|vertexfragment|volatile|while)\b").token(T::Keyword),
            rule(r"\b(true|false)\b").token(T::KeywordConstant),
            rule(r"\b(auto|catch|char|const_cast|delete|dynamic_cast|enum|explicit|friend|goto|long|mutable|new|operator|private|protected|public|reinterpret_cast|short|signed|sizeof|static_cast|template|this|throw|try|typename|union|unsigned|using|virtual)\b").token(T::KeywordReserved),
            rule(r"\b(dword|matrix|snorm|string|unorm|unsigned|void|vector|BlendState|Buffer|ByteAddressBuffer|ComputeShader|DepthStencilState|DepthStencilView|DomainShader|GeometryShader|HullShader|InputPatch|LineStream|OutputPatch|PixelShader|PointStream|RasterizerState|RenderTargetView|RasterizerOrderedBuffer|RasterizerOrderedByteAddressBuffer|RasterizerOrderedStructuredBuffer|RasterizerOrderedTexture1D|RasterizerOrderedTexture1DArray|RasterizerOrderedTexture2D|RasterizerOrderedTexture2DArray|RasterizerOrderedTexture3D|RWBuffer|RWByteAddressBuffer|RWStructuredBuffer|RWTexture1D|RWTexture1DArray|RWTexture2D|RWTexture2DArray|RWTexture3D|SamplerState|SamplerComparisonState|StructuredBuffer|Texture1D|Texture1DArray|Texture2D|Texture2DArray|Texture2DMS|Texture2DMSArray|Texture3D|TextureCube|TextureCubeArray|TriangleStream|VertexShader)\b").token(T::KeywordType),
            rule(r"\b(bool|double|float|int|half|min16float|min10float|min16int|min12int|min16uint|uint)([1-4](x[1-4])?)?\b").token(T::KeywordType),
            rule(r"\b(abort|abs|acos|all|AllMemoryBarrier|AllMemoryBarrierWithGroupSync|any|AppendStructuredBuffer|asdouble|asfloat|asin|asint|asuint|asuint|atan|atan2|ceil|CheckAccessFullyMapped|clamp|clip|CompileShader|ConsumeStructuredBuffer|cos|cosh|countbits|cross|D3DCOLORtoUBYTE4|ddx|ddx_coarse|ddx_fine|ddy|ddy_coarse|ddy_fine|degrees|determinant|DeviceMemoryBarrier|DeviceMemoryBarrierWithGroupSync|distance|dot|dst|errorf|EvaluateAttributeAtCentroid|EvaluateAttributeAtSample|EvaluateAttributeSnapped|exp|exp2|f16tof32|f32tof16|faceforward|firstbithigh|firstbitlow|floor|fma|fmod|frac|frexp|fwidth|GetRenderTargetSampleCount|GetRenderTargetSamplePosition|GlobalOrderedCountIncrement|GroupMemoryBarrier|GroupMemoryBarrierWithGroupSync|InterlockedAdd|InterlockedAnd|InterlockedCompareExchange|InterlockedCompareStore|InterlockedExchange|InterlockedMax|InterlockedMin|InterlockedOr|InterlockedXor|isfinite|isinf|isnan|ldexp|length|lerp|lit|log|log10|log2|mad|max|min|modf|msad4|mul|noise|normalize|pow|printf|Process2DQuadTessFactorsAvg|Process2DQuadTessFactorsMax|Process2DQuadTessFactorsMin|ProcessIsolineTessFactors|ProcessQuadTessFactorsAvg|ProcessQuadTessFactorsMax|ProcessQuadTessFactorsMin|ProcessTriTessFactorsAvg|ProcessTriTessFactorsMax|ProcessTriTessFactorsMin|QuadReadLaneAt|QuadSwapX|QuadSwapY|radians|rcp|reflect|refract|reversebits|round|rsqrt|saturate|sign|sin|sincos|sinh|smoothstep|sqrt|step|tan|tanh|tex1D|tex1D|tex1Dbias|tex1Dgrad|tex1Dlod|tex1Dproj|tex2D|tex2D|tex2Dbias|tex2Dgrad|tex2Dlod|tex2Dproj|tex3D|tex3D|tex3Dbias|tex3Dgrad|tex3Dlod|tex3Dproj|texCUBE|texCUBE|texCUBEbias|texCUBEgrad|texCUBElod|texCUBEproj|transpose|trunc|WaveAllBitAnd|WaveAllMax|WaveAllMin|WaveAllBitOr|WaveAllBitXor|WaveAllEqual|WaveAllProduct|WaveAllSum|WaveAllTrue|WaveAnyTrue|WaveBallot|WaveGetLaneCount|WaveGetLaneIndex|WaveGetOrderedIndex|WaveIsHelperLane|WaveOnce|WavePrefixProduct|WavePrefixSum|WaveReadFirstLane|WaveReadLaneAt)\b").token(T::NameBuiltin),
            rule(r"\b(SV_ClipDistance|SV_ClipDistance0|SV_ClipDistance1|SV_Culldistance|SV_CullDistance0|SV_CullDistance1|SV_Coverage|SV_Depth|SV_DepthGreaterEqual|SV_DepthLessEqual|SV_DispatchThreadID|SV_DomainLocation|SV_GroupID|SV_GroupIndex|SV_GroupThreadID|SV_GSInstanceID|SV_InnerCoverage|SV_InsideTessFactor|SV_InstanceID|SV_IsFrontFace|SV_OutputControlPointID|SV_Position|SV_PrimitiveID|SV_RenderTargetArrayIndex|SV_SampleIndex|SV_StencilRef|SV_TessFactor|SV_VertexID|SV_ViewportArrayIndex)\b").token(T::NameDecorator),
            rule(r"\bSV_Target[0-7]?\b").token(T::NameDecorator),
            rule(r"\b(allow_uav_condition|branch|call|domain|earlydepthstencil|fastopt|flatten|forcecase|instance|loop|maxtessfactor|numthreads|outputcontrolpoints|outputtopology|partitioning|patchconstantfunc|unroll)\b").token(T::NameDecorator),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
            rule(r"\\$").token(T::CommentPreproc),
            rule(r"\s+").token(T::Text),
        ]),
        ("string", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"\\([\\abfnrtv"\']|x[a-fA-F0-9]{2,4}|u[a-fA-F0-9]{4}|U[a-fA-F0-9]{8}|[0-7]{1,3})"#).token(T::LiteralStringEscape),
            rule(r#"[^\\"\n]+"#).token(T::LiteralString),
            rule(r"\\\n").token(T::LiteralString),
            rule(r"\\").token(T::LiteralString),
        ]),
    ],
};

//! Chroma's `matlab.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "matlab",
    config: ConfigDef {
        name: "Matlab",
        aliases: &["matlab"],
        filenames: &["*.m"],
        mime_types: &["text/matlab"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("blockcomment", &[
            rule(r"^\s*%\}").token(T::CommentMultiline).pop(1),
            rule(r"^.*\n").token(T::CommentMultiline),
            rule(r".").token(T::CommentMultiline),
        ]),
        ("deffunc", &[
            rule(r"(\s*)(?:(.+)(\s*)(=)(\s*))?(.+)(\()(.*)(\))(\s*)").groups(&[T::TextWhitespace, T::Text, T::TextWhitespace, T::Punctuation, T::TextWhitespace, T::NameFunction, T::Punctuation, T::Text, T::Punctuation, T::TextWhitespace]).pop(1),
            rule(r"(\s*)([a-zA-Z_]\w*)").groups(&[T::Text, T::NameFunction]).pop(1),
        ]),
        ("root", &[
            rule(r"\n").token(T::Text),
            rule(r"^!.*").token(T::LiteralStringOther),
            rule(r"%\{\s*\n").token(T::CommentMultiline).push(&["blockcomment"]),
            rule(r"%.*$").token(T::Comment),
            rule(r"^\s*function").token(T::Keyword).push(&["deffunc"]),
            rule(r"(properties|persistent|enumerated|otherwise|continue|function|classdef|methods|elseif|events|switch|return|global|parfor|catch|break|while|else|spmd|case|try|end|for|if)\b").token(T::Keyword),
            rule(r"(sin|sind|sinh|asin|asind|asinh|cos|cosd|cosh|acos|acosd|acosh|tan|tand|tanh|atan|atand|atan2|atanh|sec|secd|sech|asec|asecd|asech|csc|cscd|csch|acsc|acscd|acsch|cot|cotd|coth|acot|acotd|acoth|hypot|exp|expm1|log|log1p|log10|log2|pow2|realpow|reallog|realsqrt|sqrt|nthroot|nextpow2|abs|angle|complex|conj|imag|real|unwrap|isreal|cplxpair|fix|floor|ceil|round|mod|rem|sign|airy|besselj|bessely|besselh|besseli|besselk|beta|betainc|betaln|ellipj|ellipke|erf|erfc|erfcx|erfinv|expint|gamma|gammainc|gammaln|psi|legendre|cross|dot|factor|isprime|primes|gcd|lcm|rat|rats|perms|nchoosek|factorial|cart2sph|cart2pol|pol2cart|sph2cart|hsv2rgb|rgb2hsv|zeros|ones|eye|repmat|rand|randn|linspace|logspace|freqspace|meshgrid|accumarray|size|length|ndims|numel|disp|isempty|isequal|isequalwithequalnans|cat|reshape|diag|blkdiag|tril|triu|fliplr|flipud|flipdim|rot90|find|end|sub2ind|ind2sub|bsxfun|ndgrid|permute|ipermute|shiftdim|circshift|squeeze|isscalar|isvector|ans|eps|realmax|realmin|pi|i|inf|nan|isnan|isinf|isfinite|j|why|compan|gallery|hadamard|hankel|hilb|invhilb|magic|pascal|rosser|toeplitz|vander|wilkinson)\b").token(T::NameBuiltin),
            rule(r"\.\.\..*$").token(T::Comment),
            rule(r"-|==|~=|<|>|<=|>=|&&|&|~|\|\|?").token(T::Operator),
            rule(r"\.\*|\*|\+|\.\^|\.\\|\.\/|\/|\\").token(T::Operator),
            rule(r"\[|\]|\(|\)|\{|\}|:|@|\.|,").token(T::Punctuation),
            rule(r"=|:|;").token(T::Punctuation),
            rule(r"(?<=[\w)\].])\'+").token(T::Operator),
            rule(r"(\d+\.\d*|\d*\.\d+)([eEf][+-]?[0-9]+)?").token(T::LiteralNumberFloat),
            rule(r"\d+[eEf][+-]?[0-9]+").token(T::LiteralNumberFloat),
            rule(r"\d+").token(T::LiteralNumberInteger),
            rule(r"(?<![\w)\].])\'").token(T::LiteralString).push(&["string"]),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
            rule(r".").token(T::Text),
        ]),
        ("string", &[
            rule(r"[^\']*\'").token(T::LiteralString).pop(1),
        ]),
    ],
};

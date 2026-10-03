//! Chroma's `webvtt.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "webvtt",
    config: ConfigDef {
        name: "WebVTT",
        aliases: &["vtt"],
        filenames: &["*.vtt"],
        mime_types: &["text/vtt"],
        ..ConfigDef::EMPTY
    },
    // The WebVTT spec refers to a WebVTT line terminator as either CRLF, CR or LF.
    // (https://www.w3.org/TR/webvtt1/#webvtt-line-terminator) However, with this
    // definition it is unclear whether CRLF is one line terminator (CRLF) or two
    // line terminators (CR and LF).
    //
    // To work around this ambiguity, only CRLF and LF are considered as line terminators.
    // To my knowledge only classic Mac OS uses CR as line terminators, so the lexer should
    // still work for most files.
    states: &[
        // https://www.w3.org/TR/webvtt1/#webvtt-file-body
        ("root", &[
            rule(r"(\AWEBVTT)((?:[ \t][^\r\n]*)?(?:\r?\n){2,})").groups(&[T::Keyword, T::Text]),
            rule(r"(^REGION)([ \t]*$)").groups(&[T::Keyword, T::Text]).push(&["region-settings-list"]),
            rule(r"(^STYLE)([ \t]*$)((?:(?!-->)[\s\S])*?)((?:\r?\n){2})").bygroups(&[E::Token(T::Keyword), E::Token(T::Text), E::Using("CSS"), E::Token(T::Text)]),
            include("comment"),
            rule(r"(?=((?![^\r\n]*-->)[^\r\n]*\r?\n)?(\d{2}:)?(?:[0-5][0-9]):(?:[0-5][0-9])\.\d{3}[ \t]+-->[ \t]+(\d{2}:)?(?:[0-5][0-9]):(?:[0-5][0-9])\.\d{3})").push(&["cues"]),
        ]),
        // https://www.w3.org/TR/webvtt1/#webvtt-region-settings-list
        ("region-settings-list", &[
            rule(r"(?: |\t|\r?\n(?!\r?\n))+").token(T::Text),
            rule(r"(?:\r?\n){2}").token(T::Text).pop(1),
            rule(r"(id)(:)(?!-->)(\S+)").groups(&[T::Keyword, T::Punctuation, T::Literal]),
            rule(r"(width)(:)((?:[1-9]?\d|100)(?:\.\d+)?)(%)").groups(&[T::Keyword, T::Punctuation, T::Literal, T::KeywordType]),
            rule(r"(lines)(:)(\d+)").groups(&[T::Keyword, T::Punctuation, T::Literal]),
            rule(r"(regionanchor|viewportanchor)(:)((?:[1-9]?\d|100)(?:\.\d+)?)(%)(,)((?:[1-9]?\d|100)(?:\.\d+)?)(%)").groups(&[T::Keyword, T::Punctuation, T::Literal, T::KeywordType, T::Punctuation, T::Literal, T::KeywordType]),
            rule(r"(scroll)(:)(up)").groups(&[T::Keyword, T::Punctuation, T::KeywordConstant]),
        ]),
        // https://www.w3.org/TR/webvtt1/#webvtt-comment-block
        ("comment", &[
            rule(r"^NOTE( |\t|\r?\n)((?!-->)[\s\S])*?(?:(\r?\n){2}|\Z)").token(T::Comment),
        ]),
        // "Zero or more WebVTT cue blocks and WebVTT comment blocks separated from each other by one or more
        // WebVTT line terminators." (https://www.w3.org/TR/webvtt1/#file-structure)
        ("cues", &[
            rule(r"(?:((?!-->)[^\r\n]+)?(\r?\n))?((?:\d{2}:)?(?:[0-5][0-9]):(?:[0-5][0-9])\.\d{3})([ \t]+)(-->)([ \t]+)((?:\d{2}:)?(?:[0-5][0-9]):(?:[0-5][0-9])\.\d{3})([ \t]*)").groups(&[T::Name, T::Text, T::LiteralDate, T::Text, T::Operator, T::Text, T::LiteralDate, T::Text]).push(&["cue-settings-list"]),
            include("comment"),
        ]),
        // https://www.w3.org/TR/webvtt1/#webvtt-cue-settings-list
        ("cue-settings-list", &[
            rule(r"[ \t]+").token(T::Text),
            rule(r"(vertical)(:)?(rl|lr)?").groups(&[T::Keyword, T::Punctuation, T::KeywordConstant]),
            rule(r"(line)(:)?(?:(?:((?:[1-9]?\d|100)(?:\.\d+)?)(%)|(-?\d+))(?:(,)(start|center|end))?)?").groups(&[T::Keyword, T::Punctuation, T::Literal, T::KeywordType, T::Literal, T::Punctuation, T::KeywordConstant]),
            rule(r"(position)(:)?(?:(?:((?:[1-9]?\d|100)(?:\.\d+)?)(%)|(-?\d+))(?:(,)(line-left|center|line-right))?)?").groups(&[T::Keyword, T::Punctuation, T::Literal, T::KeywordType, T::Literal, T::Punctuation, T::KeywordConstant]),
            rule(r"(size)(:)?(?:((?:[1-9]?\d|100)(?:\.\d+)?)(%))?").groups(&[T::Keyword, T::Punctuation, T::Literal, T::KeywordType]),
            rule(r"(align)(:)?(start|center|end|left|right)?").groups(&[T::Keyword, T::Punctuation, T::KeywordConstant]),
            rule(r"(region)(:)?((?![^\r\n]*-->(?=[ \t]+?))[^ \t\r\n]+)?").groups(&[T::Keyword, T::Punctuation, T::Literal]),
            rule(r"(?=\r?\n)").push(&["cue-payload"]),
        ]),
        // https://www.w3.org/TR/webvtt1/#cue-payload
        ("cue-payload", &[
            rule(r"(\r?\n){2,}").token(T::Text).pop(2),
            rule(r"[^<&]+?").token(T::Text),
            rule(r"&(#\d+|#x[0-9A-Fa-f]+|[a-zA-Z0-9]+);").token(T::Text),
            rule(r"(?=<)").token(T::Text).push(&["cue-span-tag"]),
        ]),
        ("cue-span-tag", &[
            rule(r"<(?=c|i|b|u|ruby|rt|v|lang|(?:\d{2}:)?(?:[0-5][0-9]):(?:[0-5][0-9])\.\d{3})").token(T::Punctuation).push(&["cue-span-start-tag-name"]),
            rule(r"(</)(c|i|b|u|ruby|rt|v|lang)").groups(&[T::Punctuation, T::NameTag]),
            rule(r">").token(T::Punctuation).pop(1),
        ]),
        ("cue-span-start-tag-name", &[
            rule(r"(c|i|b|u|ruby|rt)|((?:\d{2}:)?(?:[0-5][0-9]):(?:[0-5][0-9])\.\d{3})").groups(&[T::NameTag, T::LiteralDate]).push(&["cue-span-classes-without-annotations"]),
            rule(r"v|lang").token(T::NameTag).push(&["cue-span-classes-with-annotations"]),
        ]),
        ("cue-span-classes-without-annotations", &[
            include("cue-span-classes"),
            rule(r"(?=>)").pop(2),
        ]),
        ("cue-span-classes-with-annotations", &[
            include("cue-span-classes"),
            rule(r"(?=[ \t])").push(&["cue-span-start-tag-annotations"]),
        ]),
        ("cue-span-classes", &[
            rule(r"(\.)([^ \t\n\r&<>\.]+)").groups(&[T::Punctuation, T::NameTag]),
        ]),
        ("cue-span-start-tag-annotations", &[
            rule(r"[ \t](?:[^\n\r&>]|&(?:#\d+|#x[0-9A-Fa-f]+|[a-zA-Z0-9]+);)+").token(T::Text),
            rule(r"(?=>)").token(T::Text).pop(3),
        ]),
    ],
};

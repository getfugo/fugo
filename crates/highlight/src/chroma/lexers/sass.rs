//! Chroma's `sass.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "sass",
    config: ConfigDef {
        name: "Sass",
        aliases: &["sass"],
        filenames: &["*.sass"],
        mime_types: &["text/x-sass"],
        case_insensitive: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("import", &[
            rule(r"[ \t]+").token(T::Text),
            rule(r"\S+").token(T::LiteralString),
            rule(r"\n").token(T::Text).push(&["root"]),
        ]),
        ("string-single", &[
            rule(r"(\\.|#(?=[^\n{])|[^\n'#])+").token(T::LiteralStringSingle),
            rule(r"#\{").token(T::LiteralStringInterpol).push(&["interpolation"]),
            rule(r"'").token(T::LiteralStringSingle).pop(1),
        ]),
        ("string-double", &[
            rule(r##"(\\.|#(?=[^\n{])|[^\n"#])+"##).token(T::LiteralStringDouble),
            rule(r"#\{").token(T::LiteralStringInterpol).push(&["interpolation"]),
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
        ]),
        ("pseudo-class", &[
            rule(r"[\w-]+").token(T::NameDecorator),
            rule(r"#\{").token(T::LiteralStringInterpol).push(&["interpolation"]),
            rule("").pop(1),
        ]),
        ("for", &[
            rule(r"(from|to|through)").token(T::OperatorWord),
            include("value"),
        ]),
        ("selector", &[
            rule(r"[ \t]+").token(T::Text),
            rule(r"\:").token(T::NameDecorator).push(&["pseudo-class"]),
            rule(r"\.").token(T::NameClass).push(&["class"]),
            rule(r"\#").token(T::NameNamespace).push(&["id"]),
            rule(r"[\w-]+").token(T::NameTag),
            rule(r"#\{").token(T::LiteralStringInterpol).push(&["interpolation"]),
            rule(r"&").token(T::Keyword),
            rule(r"[~^*!&\[\]()<>|+=@:;,./?-]").token(T::Operator),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["string-double"]),
            rule(r"'").token(T::LiteralStringSingle).push(&["string-single"]),
            rule(r"\n").token(T::Text).push(&["root"]),
        ]),
        ("value", &[
            rule(r"[ \t]+").token(T::Text),
            rule(r"[!$][\w-]+").token(T::NameVariable),
            rule(r"url\(").token(T::LiteralStringOther).push(&["string-url"]),
            rule(r"[a-z_-][\w-]*(?=\()").token(T::NameFunction),
            rule(r"(color-interpolation-filters|glyph-orientation-vertical|transition-timing-function|border-bottom-right-radius|border-bottom-left-radius|animation-iteration-count|animation-timing-function|font-variant-alternates|font-variant-east-asian|text-underline-position|border-top-right-radius|scroll-snap-destination|scroll-snap-coordinate|font-language-override|text-emphasis-position|font-variant-ligatures|border-top-left-radius|background-blend-mode|background-attachment|text-decoration-color|text-decoration-style|font-feature-settings|grid-template-columns|shape-image-threshold|hyphenate-limit-lines|hyphenate-limit-chars|font-variant-position|hyphenate-limit-last|scroll-snap-points-x|initial-letter-align|decimal-leading-zero|box-decoration-break|hyphenate-limit-zone|text-combine-upright|text-decoration-skip|font-variant-numeric|scroll-snap-points-y|animation-play-state|text-decoration-line|hyphenate-character|transition-duration|text-emphasis-style|border-bottom-width|border-image-repeat|grid-template-areas|border-image-source|animation-direction|initial-letter-wrap|list-style-position|transition-property|text-space-collapse|border-bottom-style|hanging-punctuation|text-emphasis-color|border-image-outset|background-position|animation-fill-mode|backface-visibility|border-bottom-color|border-right-width|alignment-baseline|mask-border-outset|border-right-style|border-right-color|mask-border-repeat|table-header-group|grid-template-rows|animation-duration|perspective-origin|presentation-level|table-footer-group|mask-border-source|border-image-width|border-image-slice|table-column-group|counter-increment|dominant-baseline|column-rule-width|speak-punctuation|page-break-before|image-orientation|font-variant-caps|grid-auto-columns|page-break-inside|border-left-width|marquee-direction|border-left-style|border-left-color|grid-column-start|mask-border-width|column-rule-color|column-rule-style|background-repeat|background-origin|mask-border-slice|scroll-snap-type|border-top-style|border-top-width|background-image|page-break-after|transform-origin|border-top-color|text-orientation|background-color|transition-delay|footnote-display|list-style-image|mask-border-mode|font-size-adjust|image-resolution|footnote-policy|object-position|ultra-condensed|motion-rotation|cjk-ideographic|justify-content|animation-delay|region-fragment|table-row-group|caret-animation|text-align-last|float-reference|extra-condensed|transform-style|text-decoration|grid-column-gap|border-collapse|background-clip|border-boundary|text-space-trim|grid-column-end|list-style-type|background-size|font-synthesis|padding-bottom|outline-offset|initial-letter|animation-name|hiragana-iroha|katakana-iroha|no-close-quote|border-spacing|rotation-point|flex-direction|voice-duration|grid-row-start|vertical-align|extra-expanded|polar-distance|baseline-shift|letter-spacing|grid-auto-flow|grid-auto-rows|lighting-color|bookmark-label|text-transform|semi-condensed|bookmark-level|mask-composite|bookmark-state|ultra-expanded|mix-blend-mode|overflow-style|grid-template|border-bottom|small-caption|semi-expanded|mask-position|no-open-quote|motion-offset|border-radius|offset-before|outline-color|outline-style|table-caption|outline-width|overflow-wrap|padding-right|flood-opacity|voice-balance|ruby-position|justify-items|bidi-override|marquee-style|shape-outside|speak-numeral|marquee-speed|counter-reset|margin-bottom|text-overflow|text-emphasis|align-content|break-before|border-width|border-image|shape-margin|speak-header|margin-right|inline-table|table-layout|center-right|marquee-loop|border-color|line-through|box-suppress|shape-inside|justify-self|float-offset|pause-before|table-column|grid-row-gap|grid-row-end|padding-left|text-justify|column-count|wrap-through|font-kerning|font-stretch|border-right|border-style|word-spacing|offset-start|caption-side|voice-volume|voice-stress|font-variant|text-spacing|offset-after|voice-family|unicode-bidi|break-inside|writing-mode|column-width|break-after|user-select|align-items|motion-path|font-weight|grid-column|voice-range|white-space|will-change|wrap-before|transparent|caret-color|text-bottom|caret-shape|mask-repeat|wrap-inside|upper-latin|text-shadow|mask-origin|font-family|flex-shrink|column-fill|message-box|text-indent|lower-roman|column-rule|padding-top|column-span|play-during|upper-roman|flood-color|page-policy|border-left|pause-after|lower-greek|perspective|lower-alpha|counter-set|mask-border|center-left|polar-angle|float-defer|marker-side|speech-rate|upper-alpha|line-height|pitch-range|empty-cells|close-quote|rest-before|margin-left|voice-pitch|overflow-y|small-caps|ruby-align|ruby-merge|wrap-after|box-shadow|word-break|flex-basis|max-height|rest-after|mask-image|list-style|min-height|voice-rate|visibility|sans-serif|object-fit|continuous|status-bar|offset-end|align-self|box-sizing|right-side|string-set|transition|margin-top|text-align|cue-before|border-top|font-style|line-break|background|capitalize|table-cell|open-quote|appearance|overflow-x|column-gap|rightwards|table-row|animation|flow-into|landscape|left-side|clip-rule|font-size|clip-path|flow-from|text-wrap|nw-resize|transform|no-repeat|line-grid|condensed|sw-resize|leftwards|line-snap|direction|crosshair|nav-right|list-item|se-resize|isolation|underline|grid-area|uppercase|min-width|flex-wrap|spell-out|max-width|ne-resize|max-lines|mask-clip|elevation|far-right|flex-flow|mask-type|word-wrap|flex-grow|mask-size|wrap-flow|monospace|mask-mode|cue-after|lowercase|relative|absolute|grid-row|armenian|e-resize|narrower|expanded|baseline|nav-down|nav-left|w-resize|xx-large|text-top|overline|overflow|katakana|n-resize|repeat-x|repeat-y|rotation|position|box-snap|xx-small|collapse|tab-size|speak-as|hiragana|s-resize|portrait|georgian|richness|separate|grid-gap|far-left|inherit|columns|content|oblique|default|outside|display|x-small|decimal|fantasy|z-index|running|azimuth|x-large|lighter|cursive|padding|outline|orphans|opacity|pointer|move-to|visible|hyphens|justify|smaller|nav-up|circle|inline|motion|inside|invert|italic|height|center|higher|x-fast|larger|hidden|digits|hebrew|groove|x-high|faster|quotes|static|behind|square|resize|border|slower|silent|dashed|medium|always|middle|filter|x-loud|bottom|widows|volume|scroll|chains|x-soft|stress|normal|nowrap|repeat|double|dotted|margin|outset|cursor|bolder|avoid|embed|x-low|above|fixed|wider|cross|inset|ridge|large|clear|speak|order|caret|width|thick|block|level|serif|blink|lower|pause|pitch|right|aural|super|below|solid|color|float|hide|soft|rest|grid|auto|slow|flow|show|page|disc|left|clip|text|flex|bold|loud|thin|size|none|help|both|once|icon|crop|fast|crop|high|mask|wait|font|all|url|top|rgb|mix|yes|low|ltr|cue|px)\b").token(T::NameConstant),
            rule(r"(lightgoldenrodyellow|mediumspringgreen|mediumaquamarine|mediumslateblue|mediumturquoise|mediumvioletred|lightsteelblue|cornflowerblue|lightslategray|blanchedalmond|mediumseagreen|lightslategrey|darkolivegreen|darkgoldenrod|darkslateblue|lightseagreen|rebeccapurple|darkslategrey|darkslategray|palegoldenrod|paleturquoise|palevioletred|darkturquoise|lavenderblush|antiquewhite|mediumorchid|lightskyblue|mediumpurple|midnightblue|darkseagreen|lemonchiffon|springgreen|yellowgreen|greenyellow|navajowhite|darkmagenta|lightyellow|transparent|lightsalmon|forestgreen|saddlebrown|deepskyblue|floralwhite|dodgerblue|ghostwhite|lightcoral|sandybrown|darkviolet|papayawhip|mediumblue|chartreuse|lightgreen|whitesmoke|aquamarine|darkorange|darksalmon|powderblue|darkorchid|blueviolet|indianred|mintcream|mistyrose|olivedrab|goldenrod|orangered|lawngreen|gainsboro|lightblue|firebrick|lightcyan|peachpuff|lightgray|darkkhaki|lightgrey|darkgreen|rosybrown|royalblue|slateblue|chocolate|cadetblue|burlywood|slategray|slategrey|limegreen|steelblue|turquoise|palegreen|lightpink|aliceblue|moccasin|darkgrey|darkblue|seagreen|lavender|cornsilk|deeppink|seashell|darkgray|honeydew|darkcyan|dimgrey|magenta|crimson|darkred|hotpink|skyblue|oldlace|dimgray|fuchsia|thistle|orchid|indigo|orange|tomato|violet|salmon|yellow|silver|purple|bisque|sienna|maroon|black|linen|azure|white|wheat|khaki|green|olive|ivory|coral|brown|beige|snow|blue|navy|aqua|teal|gray|gold|grey|lime|peru|cyan|pink|plum|tan|red)\b").token(T::NameEntity),
            rule(r"(fuchsia|purple|yellow|maroon|silver|white|olive|green|black|lime|gray|navy|blue|teal|aqua|red)\b").token(T::NameBuiltin),
            rule(r"\!(important|default)").token(T::NameException),
            rule(r"(true|false)").token(T::NamePseudo),
            rule(r"(and|or|not)").token(T::OperatorWord),
            rule(r"/\*").token(T::CommentMultiline).push(&["inline-comment"]),
            rule(r"//[^\n]*").token(T::CommentSingle),
            rule(r"\#[a-z0-9]{1,6}").token(T::LiteralNumberHex),
            rule(r"(-?\d+)(\%|[a-z]+)?").groups(&[T::LiteralNumberInteger, T::KeywordType]),
            rule(r"(-?\d*\.\d+)(\%|[a-z]+)?").groups(&[T::LiteralNumberFloat, T::KeywordType]),
            rule(r"#\{").token(T::LiteralStringInterpol).push(&["interpolation"]),
            rule(r"[~^*!&%<>|+=@:,./?-]+").token(T::Operator),
            rule(r"[\[\]()]+").token(T::Punctuation),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["string-double"]),
            rule(r"'").token(T::LiteralStringSingle).push(&["string-single"]),
            rule(r"[a-z_-][\w-]*").token(T::Name),
            rule(r"\n").token(T::Text).push(&["root"]),
        ]),
        ("id", &[
            rule(r"[\w-]+").token(T::NameNamespace),
            rule(r"#\{").token(T::LiteralStringInterpol).push(&["interpolation"]),
            rule("").pop(1),
        ]),
        ("multi-comment", &[
            rule(r".+").token(T::CommentMultiline),
            rule(r"\n").token(T::Text).push(&["root"]),
        ]),
        ("interpolation", &[
            rule(r"\}").token(T::LiteralStringInterpol).pop(1),
            include("value"),
        ]),
        ("string-url", &[
            rule(r"(\\#|#(?=[^\n{])|[^\n#)])+").token(T::LiteralStringOther),
            rule(r"#\{").token(T::LiteralStringInterpol).push(&["interpolation"]),
            rule(r"\)").token(T::LiteralStringOther).pop(1),
        ]),
        ("inline-comment", &[
            rule(r"(\\#|#(?=[^\n{])|\*(?=[^\n/])|[^\n#*])+").token(T::CommentMultiline),
            rule(r"#\{").token(T::LiteralStringInterpol).push(&["interpolation"]),
            rule(r"\*/").token(T::Comment).pop(1),
        ]),
        ("root", &[
            rule(r"[ \t]*\n").token(T::Text),
            rule(r"@import").token(T::Keyword).push(&["import"]),
            rule(r"@for").token(T::Keyword).push(&["for"]),
            rule(r"@(debug|warn|if|while)").token(T::Keyword).push(&["value"]),
            rule(r"(@mixin)( [\w-]+)").groups(&[T::Keyword, T::NameFunction]).push(&["value"]),
            rule(r"(@include)( [\w-]+)").groups(&[T::Keyword, T::NameDecorator]).push(&["value"]),
            rule(r"@extend").token(T::Keyword).push(&["selector"]),
            rule(r"@[\w-]+").token(T::Keyword).push(&["selector"]),
            rule(r"=[\w-]+").token(T::NameFunction).push(&["value"]),
            rule(r"\+[\w-]+").token(T::NameDecorator).push(&["value"]),
            rule(r"([!$][\w-]\w*)([ \t]*(?:(?:\|\|)?=|:))").groups(&[T::NameVariable, T::Operator]).push(&["value"]),
            rule(r":").token(T::NameAttribute).push(&["old-style-attr"]),
            rule(r"(?=.+?[=:]([^a-z]|$))").token(T::NameAttribute).push(&["new-style-attr"]),
            rule("").push(&["selector"]),
        ]),
        ("single-comment", &[
            rule(r".+").token(T::CommentSingle),
            rule(r"\n").token(T::Text).push(&["root"]),
        ]),
        ("new-style-attr", &[
            rule(r#"[^\s:="\[]+"#).token(T::NameAttribute),
            rule(r"#\{").token(T::LiteralStringInterpol).push(&["interpolation"]),
            rule(r"[ \t]*[=:]").token(T::Operator).push(&["value"]),
        ]),
        ("class", &[
            rule(r"[\w-]+").token(T::NameClass),
            rule(r"#\{").token(T::LiteralStringInterpol).push(&["interpolation"]),
            rule("").pop(1),
        ]),
        ("old-style-attr", &[
            rule(r#"[^\s:="\[]+"#).token(T::NameAttribute),
            rule(r"#\{").token(T::LiteralStringInterpol).push(&["interpolation"]),
            rule(r"[ \t]*=").token(T::Operator).push(&["value"]),
            rule("").push(&["value"]),
        ]),
    ],
};

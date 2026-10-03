//! Chroma's `agda.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "agda",
    config: ConfigDef {
        name: "Agda",
        aliases: &["agda"],
        filenames: &["*.agda"],
        mime_types: &["text/x-agda"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"^(\s*)([^\s(){}]+)(\s*)(:)(\s*)").groups(&[T::TextWhitespace, T::NameFunction, T::TextWhitespace, T::OperatorWord, T::TextWhitespace]),
            rule(r"--(?![!#$%&*+./<=>?@^|_~:\\]).*?$").token(T::CommentSingle),
            rule(r"\{-").token(T::CommentMultiline).push(&["comment"]),
            rule(r"\{!").token(T::CommentMultiline).push(&["hole"]),
            rule(r"\b(abstract|codata|coinductive|constructor|data|do|eta-equality|field|forall|hiding|in|inductive|infix|infixl|infixr|instance|interleaved|let|macro|mutual|no-eta-equality|open|overlap|pattern|postulate|primitive|private|quote|quoteTerm|record|renaming|rewrite|syntax|tactic|unquote|unquoteDecl|unquoteDef|using|variable|where|with)(?!\')\b").token(T::KeywordReserved),
            rule(r"(import|module)(\s+)").groups(&[T::KeywordReserved, T::TextWhitespace]).push(&["module"]),
            rule(r"\b(Set|Prop)[\u2080-\u2089]*\b").token(T::KeywordType),
            rule(r"(\(|\)|\{|\})").token(T::Operator),
            rule(r"(\.{1,3}|\||\u03BB|\u2200|\u2192|:|=|->)").token(T::OperatorWord),
            rule(r"\d+[eE][+-]?\d+").token(T::LiteralNumberFloat),
            rule(r"\d+\.\d+([eE][+-]?\d+)?").token(T::LiteralNumberFloat),
            rule(r"0[xX][\da-fA-F]+").token(T::LiteralNumberHex),
            rule(r"\d+").token(T::LiteralNumberInteger),
            rule(r"'").token(T::LiteralStringChar).push(&["character"]),
            rule(r#"""#).token(T::LiteralString).push(&["string"]),
            rule(r"[^\s(){}]+").token(T::Text),
            rule(r"\s+?").token(T::TextWhitespace),
        ]),
        ("hole", &[
            rule(r"[^!{}]+").token(T::CommentMultiline),
            rule(r"\{!").token(T::CommentMultiline).push(&[]),
            rule(r"!\}").token(T::CommentMultiline).pop(1),
            rule(r"[!{}]").token(T::CommentMultiline),
        ]),
        ("module", &[
            rule(r"\{-").token(T::CommentMultiline).push(&["comment"]),
            rule(r"[a-zA-Z][\w.\']*").token(T::Name).pop(1),
            rule(r"[\W0-9_]+").token(T::Text),
        ]),
        ("comment", &[
            rule(r"[^-{}]+").token(T::CommentMultiline),
            rule(r"\{-").token(T::CommentMultiline).push(&[]),
            rule(r"-\}").token(T::CommentMultiline).pop(1),
            rule(r"[-{}]").token(T::CommentMultiline),
        ]),
        ("character", &[
            rule(r"[^\\']'").token(T::LiteralStringChar).pop(1),
            rule(r"\\").token(T::LiteralStringEscape).push(&["escape"]),
            rule(r"'").token(T::LiteralStringChar).pop(1),
        ]),
        ("string", &[
            rule(r#"[^\\"]+"#).token(T::LiteralString),
            rule(r"\\").token(T::LiteralStringEscape).push(&["escape"]),
            rule(r#"""#).token(T::LiteralString).pop(1),
        ]),
        ("escape", &[
            rule(r#"[abfnrtv"\'&\\]"#).token(T::LiteralStringEscape).pop(1),
            rule(r"\^[][A-ZÀ-ÖØ-ÞĀĂĄĆĈĊČĎĐĒĔĖĘĚĜĞĠĢĤĦĨĪĬĮİĲĴĶĹĻĽĿŁŃŅŇŊŌŎŐŒŔŖŘŚŜŞŠŢŤŦŨŪŬŮŰŲŴŶŸ-ŹŻŽƁ-ƂƄƆ-ƇƉ-ƋƎ-ƑƓ-ƔƖ-ƘƜ-ƝƟ-ƠƢƤƦ-ƧƩƬƮ-ƯƱ-ƳƵƷ-ƸƼǄǇǊǍǏǑǓǕǗǙǛǞǠǢǤǦǨǪǬǮǱǴǶ-ǸǺǼǾȀȂȄȆȈȊȌȎȐȒȔȖȘȚȜȞȠȢȤȦȨȪȬȮȰȲȺ-ȻȽ-ȾɁɃ-ɆɈɊɌɎͰͲͶͿΆΈ-ΊΌΎ-ΏΑ-ΡΣ-ΫϏϒ-ϔϘϚϜϞϠϢϤϦϨϪϬϮϴϷϹ-ϺϽ-ЯѠѢѤѦѨѪѬѮѰѲѴѶѸѺѼѾҀҊҌҎҐҒҔҖҘҚҜҞҠҢҤҦҨҪҬҮҰҲҴҶҸҺҼҾӀ-ӁӃӅӇӉӋӍӐӒӔӖӘӚӜӞӠӢӤӦӨӪӬӮӰӲӴӶӸӺӼӾԀԂԄԆԈԊԌԎԐԒԔԖԘԚԜԞԠԢԤԦԨԪԬԮԱ-ՖႠ-ჅჇჍᎠ-ᏵᲐ-ᲺᲽ-ᲿḀḂḄḆḈḊḌḎḐḒḔḖḘḚḜḞḠḢḤḦḨḪḬḮḰḲḴḶḸḺḼḾṀṂṄṆṈṊṌṎṐṒṔṖṘṚṜṞṠṢṤṦṨṪṬṮṰṲṴṶṸṺṼṾẀẂẄẆẈẊẌẎẐẒẔẞẠẢẤẦẨẪẬẮẰẲẴẶẸẺẼẾỀỂỄỆỈỊỌỎỐỒỔỖỘỚỜỞỠỢỤỦỨỪỬỮỰỲỴỶỸỺỼỾἈ-ἏἘ-ἝἨ-ἯἸ-ἿὈ-ὍὙὛὝὟὨ-ὯᾸ-ΆῈ-ΉῘ-ΊῨ-ῬῸ-Ώℂℇℋ-ℍℐ-ℒℕℙ-ℝℤΩℨK-ℭℰ-ℳℾ-ℿⅅↃⰀ-ⰮⱠⱢ-ⱤⱧⱩⱫⱭ-ⱰⱲⱵⱾ-ⲀⲂⲄⲆⲈⲊⲌⲎⲐⲒⲔⲖⲘⲚⲜⲞⲠⲢⲤⲦⲨⲪⲬⲮⲰⲲⲴⲶⲸⲺⲼⲾⳀⳂⳄⳆⳈⳊⳌⳎⳐⳒⳔⳖⳘⳚⳜⳞⳠⳢⳫⳭⳲꙀꙂꙄꙆꙈꙊꙌꙎꙐꙒꙔꙖꙘꙚꙜꙞꙠꙢꙤꙦꙨꙪꙬꚀꚂꚄꚆꚈꚊꚌꚎꚐꚒꚔꚖꚘꚚꜢꜤꜦꜨꜪꜬꜮꜲꜴꜶꜸꜺꜼꜾꝀꝂꝄꝆꝈꝊꝌꝎꝐꝒꝔꝖꝘꝚꝜꝞꝠꝢꝤꝦꝨꝪꝬꝮꝹꝻꝽ-ꝾꞀꞂꞄꞆꞋꞍꞐꞒꞖꞘꞚꞜꞞꞠꞢꞤꞦꞨꞪ-ꞮꞰ-ꞴꞶꞸＡ-Ｚ𐐀-𐐧𐒰-𐓓𐲀-𐲲𑢠-𑢿𖹀-𖹟𝐀-𝐙𝐴-𝑍𝑨-𝒁𝒜𝒞-𝒟𝒢𝒥-𝒦𝒩-𝒬𝒮-𝒵𝓐-𝓩𝔄-𝔅𝔇-𝔊𝔍-𝔔𝔖-𝔜𝔸-𝔹𝔻-𝔾𝕀-𝕄𝕆𝕊-𝕐𝕬-𝖅𝖠-𝖹𝗔-𝗭𝘈-𝘡𝘼-𝙕𝙰-𝚉𝚨-𝛀𝛢-𝛺𝜜-𝜴𝝖-𝝮𝞐-𝞨𝟊𞤀-𞤡@^_]").token(T::LiteralStringEscape).pop(1),
            rule(r"NUL|SOH|[SE]TX|EOT|ENQ|ACK|BEL|BS|HT|LF|VT|FF|CR|S[OI]|DLE|DC[1-4]|NAK|SYN|ETB|CAN|EM|SUB|ESC|[FGRU]S|SP|DEL").token(T::LiteralStringEscape).pop(1),
            rule(r"o[0-7]+").token(T::LiteralStringEscape).pop(1),
            rule(r"x[\da-fA-F]+").token(T::LiteralStringEscape).pop(1),
            rule(r"\d+").token(T::LiteralStringEscape).pop(1),
            rule(r"(\s+)(\\)").groups(&[T::TextWhitespace, T::LiteralStringEscape]).pop(1),
        ]),
    ],
};

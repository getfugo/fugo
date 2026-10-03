//! Chroma's `powershell.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "powershell",
    config: ConfigDef {
        name: "PowerShell",
        aliases: &["powershell", "posh", "ps1", "psm1", "psd1", "pwsh"],
        filenames: &["*.ps1", "*.psm1", "*.psd1"],
        mime_types: &["text/x-powershell"],
        case_insensitive: true,
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\(").token(T::Punctuation).push(&["child"]),
            rule(r"\s+").token(T::Text),
            rule(r"(\s*)(#)(requires)(\s+)").groups(&[T::TextWhitespace, T::Comment, T::Keyword, T::TextWhitespace]).push(&["requires"]),
            rule(r"^(\s*#[#\s]*)(\.(?:component|description|example|externalhelp|forwardhelpcategory|forwardhelptargetname|functionality|inputs|link|notes|outputs|parameter|remotehelprunspace|role|synopsis))([^\n]*$)").groups(&[T::Comment, T::LiteralStringDoc, T::Comment]),
            rule(r"#[^\n]*?$").token(T::Comment),
            rule(r"(&lt;|<)#").token(T::CommentMultiline).push(&["multline"]),
            rule(r"(?i)([A-Z]:)").token(T::Name),
            rule(r#"@"\n"#).token(T::LiteralStringHeredoc).push(&["heredoc-double"]),
            rule(r"@'\n.*?\n'@").token(T::LiteralStringHeredoc),
            rule(r"@(?=\(|{)|\$(?=\()").token(T::NameVariableMagic),
            rule(r#"`[\'"$@-]"#).token(T::Punctuation),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["string"]),
            rule(r"'([^']|'')*'").token(T::LiteralStringSingle),
            rule(r"(?<!\S)(function|filter|workflow)(\s*)(global:|script:|private:|env:)?(\w\S*\b)").groups(&[T::KeywordDeclaration, T::TextWhitespace, T::NameVariableMagic, T::NameBuiltin]),
            rule(r"(?<!\S)(class|configuration)(\s+)(\w\S*)(\s*)(:*)").groups(&[T::KeywordDeclaration, T::TextWhitespace, T::NameBuiltin, T::NameBuiltin, T::NameBuiltin]),
            rule(r"\$false|\$null|\$true(?=\b)").token(T::NameVariableMagic),
            rule(r"(\$|@@|@)((global|script|private|env):)?\w+").token(T::NameVariable),
            rule(r"(parameter|validatenotnullorempty|validatescript|validaterange|validateset|validaterange|validatepattern|validatelength|validatecount|validatenotnullorempty|validatescript|cmdletbinding|alias)\b").token(T::NameBuiltin),
            rule(r"[a-z]\w*-[a-z]\w*\b").token(T::NameBuiltin),
            rule(r"(mandatory|parametersetname|position|helpmessage|valuefrompipeline|valuefrompipelinebypropertyname|valuefromremainingarguments|dontshow)\b").token(T::NameAttribute),
            rule(r"(confirmimpact|defaultparametersetname|helpuri|supportspaging|supportsshouldprocess|positionalbinding)\b").token(T::NameAttribute),
            rule(r"(while|until|trap|switch|return|ref|process|param|parameter|in|if|global:|foreach|for|finally|filter|end|elseif|else|dynamicparam|do|default|continue|break|begin|\?|%|#script|#private|#local|#global|try|catch|throw)\b").token(T::Keyword),
            rule(r"-(and|as|band|bnot|bor|bxor|casesensitive|ccontains|ceq|cge|cgt|cle|clike|clt|cmatch|cne|cnotcontains|cnotlike|cnotmatch|contains|creplace|eq|exact|f|file|ge|gt|icontains|ieq|ige|igt|ile|ilike|ilt|imatch|ine|inotcontains|inotlike|inotmatch|ireplace|is|isnot|le|like|lt|match|ne|not|notcontains|notlike|notmatch|or|regex|replace|wildcard)\b").token(T::Operator),
            rule(r"(ac|asnp|cat|cd|cfs|chdir|clc|clear|clhy|cli|clp|cls|clv|cnsn|compare|copy|cp|cpi|cpp|curl|cvpa|dbp|del|diff|dir|dnsn|ebp|echo|epal|epcsv|epsn|erase|etsn|exsn|fc|fhx|fl|foreach|ft|fw|gal|gbp|gc|gci|gcm|gcs|gdr|ghy|gi|gjb|gl|gm|gmo|gp|gps|gpv|group|gsn|gsnp|gsv|gu|gv|gwmi|h|history|icm|iex|ihy|ii|ipal|ipcsv|ipmo|ipsn|irm|ise|iwmi|iwr|kill|lp|ls|man|md|measure|mi|mount|move|mp|mv|nal|ndr|ni|nmo|npssc|nsn|nv|ogv|oh|popd|ps|pushd|pwd|r|rbp|rcjb|rcsn|rd|rdr|ren|ri|rjb|rm|rmdir|rmo|rni|rnp|rp|rsn|rsnp|rujb|rv|rvpa|rwmi|sajb|sal|saps|sasv|sbp|sc|select|set|shcm|si|sl|sleep|sls|sort|sp|spjb|spps|spsv|start|sujb|sv|swmi|tee|trcm|type|wget|where|wjb|write)\s").token(T::NameBuiltin),
            rule(r"(\[)([a-z_\[][\w. `,\[\]]*)(\])").groups(&[T::Punctuation, T::NameConstant, T::Punctuation]),
            rule(r"(?<!\[)(?<=\S[^\*|\n]\.)\w+(?=\s+|\(|\{|\.)").token(T::NameProperty),
            rule(r"(?<!\w)([-+]?(?:[0-9]+)?\.?[0-9]+(?:(?:e|E)[0-9]+)?(?:F|f|D|d|M|m)?)((?i:[kmgtp]b)?)\b").groups(&[T::LiteralNumberFloat, T::Punctuation]),
            rule(r"-[a-z_]\w*:*").token(T::Name),
            rule(r"\w+").token(T::Name),
            rule(r"[.,;@{}\[\]$()=+*/\\&%!~?^\x60|<>-]|::").token(T::Punctuation),
        ]),
        ("requires", &[
            rule(r"\s*\n|\s*$").token(T::TextWhitespace).pop(1),
            rule(r"-(?i:modules|pssnapin|runasadministrator|ahellid|version|assembly|psedition)").token(T::KeywordDeclaration),
            rule(r"-\S*\b").token(T::Comment),
            rule(r"\s+(\S+)").token(T::NameAttribute),
        ]),
        ("child", &[
            rule(r"\)").token(T::Punctuation).pop(1),
            include("root"),
        ]),
        ("multline", &[
            rule(r"[^#&.]+").token(T::CommentMultiline),
            rule(r"#(>|&gt;)").token(T::CommentMultiline).pop(1),
            rule(r"(\s*\.)(component|description|example|externalhelp|forwardhelpcategory|forwardhelptargetname|functionality|inputs|link|notes|outputs|parameter|remotehelprunspace|role|synopsis)(\s*$)").groups(&[T::CommentMultiline, T::LiteralStringDoc, T::CommentMultiline]),
            rule(r"[#&.]").token(T::CommentMultiline),
        ]),
        ("string", &[
            rule(r#"`[0abfnrtv'\"$`]"#).token(T::LiteralStringEscape),
            rule(r#"[^$`"]+"#).token(T::LiteralStringDouble),
            rule(r"\$\(").token(T::Punctuation).push(&["child"]),
            rule(r"((\$)((global|script|private|env):)?\w+)|((\$){((global|script|private|env):)?\w+})").token(T::NameVariable),
            rule(r#""""#).token(T::LiteralStringDouble),
            rule(r"[`$]").token(T::LiteralStringDouble),
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
        ]),
        ("heredoc-double", &[
            rule(r#"\n"@"#).token(T::LiteralStringHeredoc).pop(1),
            rule(r"\$\(").token(T::Punctuation).push(&["child"]),
            rule(r"((\$)((global|script|private|env):)?\w+)|((\$){((global|script|private|env):)?\w+})").token(T::NameVariable),
            rule(r#"[^@\n]+"]"#).token(T::LiteralStringHeredoc),
            rule(r".").token(T::LiteralStringHeredoc),
        ]),
    ],
};

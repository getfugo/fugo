//! Chroma's `vb_net.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "vb_net",
    config: ConfigDef {
        name: "VB.net",
        aliases: &["vb.net", "vbnet"],
        filenames: &["*.vb", "*.bas"],
        mime_types: &["text/x-vbnet", "text/x-vba"],
        case_insensitive: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("dim", &[
            rule(r"[_\w][\w]*").token(T::NameVariable).pop(1),
            rule("").pop(1),
        ]),
        ("funcname", &[
            rule(r"[_\w][\w]*").token(T::NameFunction).pop(1),
        ]),
        ("classname", &[
            rule(r"[_\w][\w]*").token(T::NameClass).pop(1),
        ]),
        ("namespace", &[
            rule(r"[_\w][\w]*").token(T::NameNamespace),
            rule(r"\.").token(T::NameNamespace),
            rule("").pop(1),
        ]),
        ("end", &[
            rule(r"\s+").token(T::Text),
            rule(r"(Function|Sub|Property|Class|Structure|Enum|Module|Namespace)\b").token(T::Keyword).pop(1),
            rule("").pop(1),
        ]),
        ("root", &[
            rule(r"^\s*<.*?>").token(T::NameAttribute),
            rule(r"\s+").token(T::Text),
            rule(r"\n").token(T::Text),
            rule(r"rem\b.*?\n").token(T::Comment),
            rule(r"'.*?\n").token(T::Comment),
            rule(r"#If\s.*?\sThen|#ElseIf\s.*?\sThen|#Else|#End\s+If|#Const|#ExternalSource.*?\n|#End\s+ExternalSource|#Region.*?\n|#End\s+Region|#ExternalChecksum").token(T::CommentPreproc),
            rule(r"[(){}!#,.:]").token(T::Punctuation),
            rule(r"Option\s+(Strict|Explicit|Compare)\s+(On|Off|Binary|Text)").token(T::KeywordDeclaration),
            rule(r"(?<!\.)(NotOverridable|NotInheritable|RemoveHandler|MustOverride|Overridable|MustInherit|Implements|RaiseEvent|AddHandler|ParamArray|WithEvents|DirectCast|Overrides|Overloads|Protected|WriteOnly|Interface|Narrowing|Inherits|Widening|SyncLock|ReadOnly|Operator|Continue|Delegate|Optional|MyClass|Declare|CUShort|Handles|Default|Shadows|TryCast|Finally|Private|Nothing|Partial|CSByte|Select|Option|Return|Friend|Resume|ElseIf|MyBase|Shared|Single|Public|CShort|Static|Global|Catch|CType|Error|CUInt|Using|While|GoSub|False|CDate|Throw|Event|CChar|CULng|CBool|Erase|ByVal|ByRef|Alias|EndIf|CByte|ReDim|Stop|Call|Wend|Next|CLng|Loop|True|CDec|With|Then|GoTo|CObj|CSng|Exit|CStr|Else|Each|Case|CInt|Step|When|CDbl|Set|For|Let|Lib|Try|New|Not|Get|On|To|Do|If|Of|Me)\b").token(T::Keyword),
            rule(r"(?<!\.)End\b").token(T::Keyword).push(&["end"]),
            rule(r"(?<!\.)(Dim|Const)\b").token(T::Keyword).push(&["dim"]),
            rule(r"(?<!\.)(Function|Sub|Property)(\s+)").groups(&[T::Keyword, T::Text]).push(&["funcname"]),
            rule(r"(?<!\.)(Class|Structure|Enum)(\s+)").groups(&[T::Keyword, T::Text]).push(&["classname"]),
            rule(r"(?<!\.)(Module|Namespace|Imports)(\s+)").groups(&[T::Keyword, T::Text]).push(&["namespace"]),
            rule(r"(?<!\.)(Boolean|Byte|Char|Date|Decimal|Double|Integer|Long|Object|SByte|Short|Single|String|Variant|UInteger|ULong|UShort)\b").token(T::KeywordType),
            rule(r"(?<!\.)(AddressOf|And|AndAlso|As|GetType|In|Is|IsNot|Like|Mod|Or|OrElse|TypeOf|Xor)\b").token(T::OperatorWord),
            rule(r"&=|[*]=|/=|\\=|\^=|\+=|-=|<<=|>>=|<<|>>|:=|<=|>=|<>|[-&*/\\^+=<>\[\]]").token(T::Operator),
            rule(r#"""#).token(T::LiteralString).push(&["string"]),
            rule(r"_\n").token(T::Text),
            rule(r"[_\w][\w]*").token(T::Name),
            rule(r"#.*?#").token(T::LiteralDate),
            rule(r"(\d+\.\d*|\d*\.\d+)(F[+-]?[0-9]+)?").token(T::LiteralNumberFloat),
            rule(r"\d+([SILDFR]|US|UI|UL)?").token(T::LiteralNumberInteger),
            rule(r"&H[0-9a-f]+([SILDFR]|US|UI|UL)?").token(T::LiteralNumberInteger),
            rule(r"&O[0-7]+([SILDFR]|US|UI|UL)?").token(T::LiteralNumberInteger),
        ]),
        ("string", &[
            rule(r#""""#).token(T::LiteralString),
            rule(r#""C?"#).token(T::LiteralString).pop(1),
            rule(r#"[^"]+"#).token(T::LiteralString),
        ]),
    ],
};

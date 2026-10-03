//! Chroma's token types and their CSS class names.
//!
//! The numbering, names and classes are Chroma's (`types.go`, v2.19.0, MIT): styles are keyed
//! by these names, the class of a type without its own class is its parent's, and the CSS of
//! a style is written in numeric order. `None` ([`TokenType::NoHighlight`], no highlighting),
//! `Ignore` (dropped by the lexer) and `EOFType` (Chroma's end-of-stream marker) have no class.

macro_rules! token_types {
    ($($variant:ident = $n:literal, $name:literal, $class:expr;)*) => {
        /// A Chroma token type.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(i32)]
        pub enum TokenType {
            $($variant = $n,)*
        }

        impl TokenType {
            /// Every token type, in numeric order.
            pub const ALL: &'static [TokenType] = &[$(TokenType::$variant,)*];

            /// Chroma's name (`LiteralStringDouble`), as used in style files.
            #[must_use]
            pub fn name(self) -> &'static str {
                match self {
                    $(TokenType::$variant => $name,)*
                }
            }

            /// The type's own entry in Chroma's class table (`None` when the type has none).
            fn own_class(self) -> Option<&'static str> {
                match self {
                    $(TokenType::$variant => $class,)*
                }
            }

            /// The type with Chroma's number `n`.
            #[must_use]
            pub fn from_number(n: i32) -> Option<Self> {
                match n {
                    $($n => Some(TokenType::$variant),)*
                    _ => None,
                }
            }

            /// The type named `name` (a style file's `type` attribute).
            #[must_use]
            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(TokenType::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

token_types! {
    Background = -1, "Background", Some("bg");
    PreWrapper = -2, "PreWrapper", Some("chroma");
    Line = -3, "Line", Some("line");
    LineNumbers = -4, "LineNumbers", Some("ln");
    LineNumbersTable = -5, "LineNumbersTable", Some("lnt");
    LineHighlight = -6, "LineHighlight", Some("hl");
    LineTable = -7, "LineTable", Some("lntable");
    LineTableTd = -8, "LineTableTD", Some("lntd");
    LineLink = -9, "LineLink", Some("lnlinks");
    CodeLine = -10, "CodeLine", Some("cl");
    Error = -11, "Error", Some("err");
    Other = -12, "Other", Some("x");
    NoHighlight = -13, "None", None;
    Ignore = -14, "Ignore", None;
    EOFType = 0, "EOFType", None;
    Keyword = 1000, "Keyword", Some("k");
    KeywordConstant = 1001, "KeywordConstant", Some("kc");
    KeywordDeclaration = 1002, "KeywordDeclaration", Some("kd");
    KeywordNamespace = 1003, "KeywordNamespace", Some("kn");
    KeywordPseudo = 1004, "KeywordPseudo", Some("kp");
    KeywordReserved = 1005, "KeywordReserved", Some("kr");
    KeywordType = 1006, "KeywordType", Some("kt");
    Name = 2000, "Name", Some("n");
    NameAttribute = 2001, "NameAttribute", Some("na");
    NameClass = 2002, "NameClass", Some("nc");
    NameConstant = 2003, "NameConstant", Some("no");
    NameDecorator = 2004, "NameDecorator", Some("nd");
    NameEntity = 2005, "NameEntity", Some("ni");
    NameException = 2006, "NameException", Some("ne");
    NameKeyword = 2007, "NameKeyword", None;
    NameLabel = 2008, "NameLabel", Some("nl");
    NameNamespace = 2009, "NameNamespace", Some("nn");
    NameOperator = 2010, "NameOperator", None;
    NameOther = 2011, "NameOther", Some("nx");
    NamePseudo = 2012, "NamePseudo", None;
    NameProperty = 2013, "NameProperty", Some("py");
    NameTag = 2014, "NameTag", Some("nt");
    NameBuiltin = 2100, "NameBuiltin", Some("nb");
    NameBuiltinPseudo = 2101, "NameBuiltinPseudo", Some("bp");
    NameVariable = 2200, "NameVariable", Some("nv");
    NameVariableAnonymous = 2201, "NameVariableAnonymous", None;
    NameVariableClass = 2202, "NameVariableClass", Some("vc");
    NameVariableGlobal = 2203, "NameVariableGlobal", Some("vg");
    NameVariableInstance = 2204, "NameVariableInstance", Some("vi");
    NameVariableMagic = 2205, "NameVariableMagic", Some("vm");
    NameFunction = 2300, "NameFunction", Some("nf");
    NameFunctionMagic = 2301, "NameFunctionMagic", Some("fm");
    Literal = 3000, "Literal", Some("l");
    LiteralDate = 3001, "LiteralDate", Some("ld");
    LiteralOther = 3002, "LiteralOther", None;
    LiteralString = 3100, "LiteralString", Some("s");
    LiteralStringAffix = 3101, "LiteralStringAffix", Some("sa");
    LiteralStringAtom = 3102, "LiteralStringAtom", None;
    LiteralStringBacktick = 3103, "LiteralStringBacktick", Some("sb");
    LiteralStringBoolean = 3104, "LiteralStringBoolean", None;
    LiteralStringChar = 3105, "LiteralStringChar", Some("sc");
    LiteralStringDelimiter = 3106, "LiteralStringDelimiter", Some("dl");
    LiteralStringDoc = 3107, "LiteralStringDoc", Some("sd");
    LiteralStringDouble = 3108, "LiteralStringDouble", Some("s2");
    LiteralStringEscape = 3109, "LiteralStringEscape", Some("se");
    LiteralStringHeredoc = 3110, "LiteralStringHeredoc", Some("sh");
    LiteralStringInterpol = 3111, "LiteralStringInterpol", Some("si");
    LiteralStringName = 3112, "LiteralStringName", None;
    LiteralStringOther = 3113, "LiteralStringOther", Some("sx");
    LiteralStringRegex = 3114, "LiteralStringRegex", Some("sr");
    LiteralStringSingle = 3115, "LiteralStringSingle", Some("s1");
    LiteralStringSymbol = 3116, "LiteralStringSymbol", Some("ss");
    LiteralNumber = 3200, "LiteralNumber", Some("m");
    LiteralNumberBin = 3201, "LiteralNumberBin", Some("mb");
    LiteralNumberFloat = 3202, "LiteralNumberFloat", Some("mf");
    LiteralNumberHex = 3203, "LiteralNumberHex", Some("mh");
    LiteralNumberInteger = 3204, "LiteralNumberInteger", Some("mi");
    LiteralNumberIntegerLong = 3205, "LiteralNumberIntegerLong", Some("il");
    LiteralNumberOct = 3206, "LiteralNumberOct", Some("mo");
    LiteralNumberByte = 3207, "LiteralNumberByte", None;
    Operator = 4000, "Operator", Some("o");
    OperatorWord = 4001, "OperatorWord", Some("ow");
    Punctuation = 5000, "Punctuation", Some("p");
    Comment = 6000, "Comment", Some("c");
    CommentHashbang = 6001, "CommentHashbang", Some("ch");
    CommentMultiline = 6002, "CommentMultiline", Some("cm");
    CommentSingle = 6003, "CommentSingle", Some("c1");
    CommentSpecial = 6004, "CommentSpecial", Some("cs");
    CommentPreproc = 6100, "CommentPreproc", Some("cp");
    CommentPreprocFile = 6101, "CommentPreprocFile", Some("cpf");
    Generic = 7000, "Generic", Some("g");
    GenericDeleted = 7001, "GenericDeleted", Some("gd");
    GenericEmph = 7002, "GenericEmph", Some("ge");
    GenericError = 7003, "GenericError", Some("gr");
    GenericHeading = 7004, "GenericHeading", Some("gh");
    GenericInserted = 7005, "GenericInserted", Some("gi");
    GenericOutput = 7006, "GenericOutput", Some("go");
    GenericPrompt = 7007, "GenericPrompt", Some("gp");
    GenericStrong = 7008, "GenericStrong", Some("gs");
    GenericSubheading = 7009, "GenericSubheading", Some("gu");
    GenericTraceback = 7010, "GenericTraceback", Some("gt");
    GenericUnderline = 7011, "GenericUnderline", Some("gl");
    Text = 8000, "Text", Some("");
    TextWhitespace = 8001, "TextWhitespace", Some("w");
    TextSymbol = 8002, "TextSymbol", None;
    TextPunctuation = 8003, "TextPunctuation", None;
}

impl TokenType {
    /// Chroma's number.
    #[must_use]
    pub fn number(self) -> i32 {
        self as i32
    }

    /// Chroma's `Parent`: the sub-category, then the category, then none.
    #[must_use]
    pub fn parent(self) -> Option<Self> {
        let n = self.number();
        let p = if n % 100 != 0 {
            n / 100 * 100
        } else if n % 1000 != 0 {
            n / 1000 * 1000
        } else {
            return None;
        };
        Self::from_number(p)
    }

    /// Chroma's `Category` (`n / 1000 * 1000`): `EOFType` (0) for the negative (structural)
    /// types.
    #[must_use]
    pub fn category(self) -> Option<Self> {
        Self::from_number(self.number() / 1000 * 1000)
    }

    /// Chroma's `SubCategory` (`n / 100 * 100`).
    #[must_use]
    pub fn sub_category(self) -> Option<Self> {
        Self::from_number(self.number() / 100 * 100)
    }

    /// Whether the type has an entry in Chroma's class table (`StandardTypes`).
    #[must_use]
    pub fn is_standard(self) -> bool {
        self.own_class().is_some()
    }

    /// The CSS class Chroma writes for the type: its own, else its nearest parent's; empty
    /// for plain text.
    #[must_use]
    pub fn class(self) -> &'static str {
        let mut t = Some(self);
        while let Some(tt) = t {
            if let Some(c) = tt.own_class() {
                return c;
            }
            t = tt.parent();
        }
        ""
    }

    /// Every class name Chroma can write.
    pub fn classes() -> impl Iterator<Item = &'static str> {
        Self::ALL.iter().filter_map(|t| t.own_class())
    }
}

#[cfg(test)]
mod tests {
    use super::TokenType;

    #[test]
    fn classes_and_parents() {
        assert_eq!(TokenType::LiteralStringDouble.class(), "s2");
        assert_eq!(TokenType::LiteralStringAtom.class(), "s");
        assert_eq!(TokenType::NameKeyword.class(), "n");
        assert_eq!(TokenType::Text.class(), "");
        assert_eq!(
            TokenType::LiteralNumberHex.parent(),
            Some(TokenType::LiteralNumber)
        );
        assert_eq!(TokenType::LiteralNumber.parent(), Some(TokenType::Literal));
        assert_eq!(TokenType::Literal.parent(), None);
        // Chroma's `Category` of a structural type is 0 (`EOFType`), which has no style.
        assert_eq!(TokenType::Background.category(), Some(TokenType::EOFType));
        assert_eq!(
            TokenType::from_name("LineTableTD"),
            Some(TokenType::LineTableTd)
        );
    }
}

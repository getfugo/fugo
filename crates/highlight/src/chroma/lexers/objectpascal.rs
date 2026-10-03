//! Chroma's `objectpascal.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "objectpascal",
    config: ConfigDef {
        name: "ObjectPascal",
        aliases: &["objectpascal"],
        filenames: &["*.pas", "*.pp", "*.inc", "*.dpr", "*.dpk", "*.lpr", "*.lpk"],
        mime_types: &["text/x-pascal"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            // TextWhitespace
            rule(r"[^\S\n]+").token(T::TextWhitespace),
            rule(r"\n").token(T::Text),
            rule(r"\s+").token(T::Text),
            // Magic Number (BOM)
            rule(r"[^\u0000-\u007F]+").token(T::Text),
            // Compiler Directive
            rule(r"\{[$].*?\}|\{[-](NOD|EXT|OBJ).*?\}|\([*][$].*?[*]\)").token(T::CommentPreproc),
            // Comment Single
            rule(r"(//.*?)(\n)").groups(&[T::CommentSingle, T::TextWhitespace]),
            // Comment Multiline Block
            rule(r"\([*](.|\n)*?[*]\)").token(T::CommentMultiline),
            // Comment Multiline Source Documentation
            rule(r"[{](.|\n)*?[}]").token(T::CommentMultiline),
            // Range Indicator
            rule(r"(?i:(\.\.))").token(T::Operator),
            // Control Character
            rule(r"[\#][0-9a-fA-F]*|[0-9]+[xX][0-9a-fA-F]*").token(T::LiteralStringEscape),
            // Numbers
            rule(r"[\$][0-9a-fA-F]*[xX][0-9a-fA-F]*|[\$][0-9a-fA-F]*|([0-9]+[0-9a-fA-F]+(?=[hH]))").token(T::LiteralNumberHex),
            rule(r"[0-9]+(\'[0-9]+)*\.[0-9]+(\'[0-9]+)*[eE][+-]?[0-9]+(\'[0-9]+)*|[0-9]+(\'[0-9]+)*\.[0-9]+(\'[0-9]+)*|\d+[eE][+-]?[0-9]+").token(T::LiteralNumberFloat),
            rule(r"0|[1-9][0-9_]*?").token(T::LiteralNumberInteger),
            // Multiline string Literal
            rule(r"('''\s*\n)(.|\n)*?(''')(?=\s*;)").token(T::LiteralString),
            // string
            rule(r"(?i:(\')).*?(?i:(\'))").token(T::LiteralString),
            // string (Special case for Delphi Assembler)
            rule(r#"(?i:(")).*?(?i:("))"#).token(T::LiteralString),
            // Simple Types
            rule(r"\b(?!=\.)(?i:(NativeInt|NativeUInt|LongInt|LongWord|Integer|Int64|Cardinal|UInt64|ShortInt|SmallInt|FixedInt|Byte|Word|FixedUInt|Int8|Int16|Int32|UInt8|UInt16|UInt32|Real48|Single|Double|Real|Extended|Comp|Currency|Char|AnsiChar|WideChar|UCS2Char|UCS4Char|string|ShortString|AnsiString|UnicodeString|WideString|RawByteString|UTF8String|File|TextFile|Text|Boolean|ByteBool|WordBool|LongBool|Pointer|Variant|OleVariant))\b(?![<\/(])").token(T::KeywordType),
            // T Types
            rule(r"\b(?!=\.)(?i:(TSingleRec|TDoubleRec|TExtended80Rec|TByteArray|TTextBuf|TVarRec|TWordArray))\b(?![<\/(])").token(T::KeywordType),
            // Pointer Types
            rule(r"\b(?!=\.)(?i:(PChar|PAnsiChar|PWideChar|PRawByteString|PUnicodeString|PString|PAnsiString|PShortString|PTextBuf|PWideString|PByte|PShortInt|PWord|PSmallInt|PCardinal|PLongWord|PFixedUInt|PLongint|PFixedInt|PUInt64|PInt64|PNativeUInt|PNativeInt|PByteArray|PCurrency|PDouble|PExtended|PSingle|PInteger|POleVariant|PVarRec|PVariant|PWordArray|PBoolean|PWordBool|PLongBool|PPointer))\b(?![<\/(])").token(T::KeywordType),
            // More Types
            rule(r"\b(?!=\.)(?i:(IntPtr|UIntPtr|Float32|Float64|_ShortStr|_ShortString|_AnsiStr|_AnsiString|_AnsiChr|_AnsiChar|_WideStr|_WideString|_PAnsiChr|_PAnsiChar|UTF8Char|_AnsiChar|PUTF8Char|_PAnsiChar|MarshaledString|MarshaledAString))\b(?![<\/(])").token(T::KeywordType),
            // Result
            rule(r"\b(?!=\.)(?i:(Result))\b(?![<\/(])").token(T::GenericEmph),
            // Result Constants
            rule(r"\b(?!=\.)(?i:(True|False))\b(?![<\/(])").token(T::NameConstant),
            // Operator (Assign)
            rule(r"[(\:\=)]").token(T::Operator),
            // Operators (Arithmetic, Unary Arithmetic, String, Pointer, Set, Relational, Address)
            rule(r"[\+\-\*\/\^<>\=\@]").token(T::Operator),
            // Operators (Arithmetic, Boolean, Logical (Bitwise), Set)
            rule(r"\b(?i:([div][mod][not][and][or][xor][shl][shr][in]))\b").token(T::OperatorWord),
            // Special Symbols (Escape, Literal Chr, Hex Value, Binary Numeral Expression Indicator)
            rule(r"[&\#\$\%]").token(T::Operator),
            // Special Symbols (Punctuation)
            rule(r"[\(\)\,\.\:\;\[\]]").token(T::Punctuation),
            // Reserved Words
            rule(r"\b(?!=\.)(?i:(and|end|interface|record|var|array|except|is|repeat|while|as|exports|label|resourcestring|with|asm|file|library|set|xor|begin|finalization|mod|shl|case|finally|nil|shr|class|for|not|string|const|function|object|then|constructor|goto|of|threadvar|destructor|if|or|to|dispinterface|implementation|packed|try|div|in|procedure|type|do|inherited|program|unit|downto|initialization|property|until|else|inline|raise|uses))\b(?![<\/(])").token(T::KeywordReserved),
            // Directives
            rule(r"\b(?!=\.)(?i:(absolute|export|name|public|stdcall|abstract|external|published|strict|assembler|nodefault|read|stored|automated|final|operator|readonly|unsafe|cdecl|forward|out|reference|varargs|contains|helper|overload|register|virtual|default|implements|override|reintroduce|winapi|delayed|index|package|requires|write|deprecated|inline|pascal|writeonly|dispid|library|platform|safecall|dynamic|local|private|sealed|experimental|message|protected|static))\b(?![<\/(])").token(T::Keyword),
            // Directives obsolete
            rule(r"\b(?!=\.)(?i:(near|far|resident))\b(?![<\/(])").token(T::Keyword),
            // Constant Expressions
            rule(r"\b(?!=\.)(?i:(Abs|High|Low|Pred|Succ|Chr|Length|Odd|Round|Swap|Hi|Lo|Ord|SizeOf|Trunc))\b(?![<\/(])").token(T::KeywordConstant),
            // everything else
            rule(r"([^\W\d]|\$)[\w$]*").token(T::Text),
        ]),
    ],
};

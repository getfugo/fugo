//! Chroma's `swift.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "swift",
    config: ConfigDef {
        name: "Swift",
        aliases: &["swift"],
        filenames: &["*.swift"],
        mime_types: &["text/x-swift"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("comment", &[
            rule(r":param: [a-zA-Z_]\w*|:returns?:|(FIXME|MARK|TODO):").token(T::CommentSpecial),
        ]),
        ("preproc", &[
            rule(r"\n").token(T::Text).pop(1),
            include("keywords"),
            rule(r"[A-Za-z]\w*").token(T::CommentPreproc),
            include("root"),
        ]),
        ("comment-single", &[
            rule(r"\n").token(T::Text).pop(1),
            include("comment"),
            rule(r"[^\n]").token(T::CommentSingle),
        ]),
        ("module", &[
            rule(r"\n").token(T::Text).pop(1),
            rule(r"[a-zA-Z_]\w*").token(T::NameClass),
            include("root"),
        ]),
        ("string", &[
            rule(r"\\\(").token(T::LiteralStringInterpol).push(&["string-intp"]),
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"\\['"\\nrt]|\\x[0-9a-fA-F]{2}|\\[0-7]{1,3}|\\u[0-9a-fA-F]{4}|\\U[0-9a-fA-F]{8}"#).token(T::LiteralStringEscape),
            rule(r#"[^\\"]+"#).token(T::LiteralString),
            rule(r"\\").token(T::LiteralString),
        ]),
        ("string-intp", &[
            rule(r"\(").token(T::LiteralStringInterpol).push(&[]),
            rule(r"\)").token(T::LiteralStringInterpol).pop(1),
            include("root"),
        ]),
        ("root", &[
            rule(r"\n").token(T::Text),
            rule(r"\s+").token(T::Text),
            rule(r"//").token(T::CommentSingle).push(&["comment-single"]),
            rule(r"/\*").token(T::CommentMultiline).push(&["comment-multi"]),
            rule(r"#(if|elseif|else|endif|available)\b").token(T::CommentPreproc).push(&["preproc"]),
            include("keywords"),
            rule(r"(ExtendedGraphemeClusterLiteralConvertible|_SwiftNSDictionaryRequiredOverridesType|_SwiftNSStringRequiredOverridesType|_SwiftNSArrayRequiredOverridesType|AutoreleasingUnsafeMutablePointer|UnicodeScalarLiteralConvertible|RangeReplaceableCollectionType|StringInterpolationConvertible|DictionaryLiteralConvertible|UnsafeBufferPointerGenerator|_SwiftNSFastEnumerationType|LazyBidirectionalCollection|ExtendedGraphemeClusterType|FloatingPointClassification|ImplicitlyUnwrappedOptional|UnsafeMutableBufferPointer|LazyRandomAccessCollection|FilterCollectionViewIndex|_ExtensibleCollectionType|IntegerLiteralConvertible|ReverseBidirectionalIndex|BooleanLiteralConvertible|StringLiteralConvertible|BidirectionalReverseView|ReverseRandomAccessIndex|ExtensibleCollectionType|FloatLiteralConvertible|RandomAccessReverseView|_BidirectionalIndexType|ArrayLiteralConvertible|_RandomAccessIndexType|_IntegerArithmeticType|BidirectionalIndexType|StrideThroughGenerator|_SwiftNSDictionaryType|_SwiftNSEnumeratorType|BitwiseOperationsType|RandomAccessIndexType|NilLiteralConvertible|_ObjectiveCBridgeable|UnicodeDecodingResult|LazyForwardCollection|IntegerArithmeticType|MutableCollectionType|PermutationGenerator|MapSequenceGenerator|_UnsignedIntegerType|FilterCollectionView|UnsafeMutablePointer|UnsafeBufferPointer|UnsignedIntegerType|_SwiftNSCopyingType|DictionaryGenerator|_SignedIntegerType|_SwiftNSStringType|IntegerLiteralType|FilterSequenceView|EnumerateGenerator|BooleanLiteralType|HeapBufferStorage|EnumerateSequence|MirrorDisposition|_SignedNumberType|UnicodeScalarType|StringLiteralType|GeneratorSequence|_ForwardIndexType|StrideToGenerator|_SwiftNSArrayType|IndexingGenerator|_RawOptionSetType|FloatingPointType|MapCollectionView|SignedIntegerType|OutputStreamType|UnicodeCodecType|HalfOpenInterval|NSObjectProtocol|SignedNumberType|ObjectIdentifier|RawOptionSetType|_CocoaStringType|ForwardIndexType|RawRepresentable|MutableSliceable|FloatLiteralType|_ArrayBufferType|AbsoluteValuable|CFunctionPointer|QuickLookObject|MapSequenceView|EmptyCollection|FilterGenerator|_CollectionType|CollectionOfOne|ContiguousArray|DictionaryIndex|GeneratorOfOne|DebugPrintable|CVaListPointer|CUnsignedShort|COpaquePointer|EmptyGenerator|_Incrementable|RangeGenerator|NSErrorPointer|_Sequence_Type|ClosedInterval|CollectionType|ZipGenerator2|UnsafePointer|GeneratorType|StrideThrough|CUnsignedLong|UnicodeScalar|_SequenceType|LazySequence|IntervalType|SequenceType|StaticString|CUnsignedInt|_IntegerType|BooleanType|CVarArgType|_Strideable|_Comparable|GeneratorOf|IntegerType|Reflectable|CSignedChar|Streamable|SequenceOf|Dictionary|Comparable|HeapBuffer|_Sliceable|MirrorType|Strideable|CLongLong|CWideChar|Printable|Equatable|Unmanaged|Sliceable|Character|AnyObject|Optional|StrideTo|SinkType|AnyClass|Hashable|Selector|Float32|CChar16|UIntMax|Float80|Float64|RawByte|Process|CDouble|CChar32|UInt16|UInt32|C_ARGC|C_ARGV|UInt64|CFloat|String|SinkOf|Repeat|OnHeap|Double|CShort|IntMax|UTF16|Slice|CLong|UTF32|UInt8|Range|UWord|Array|Float|Int64|Int32|Int16|CBool|CChar|UInt|Int8|Word|UTF8|Zip2|Void|Bool|CInt|Any|Int|Bit)\b").token(T::NameBuiltin),
            rule(r"(withUnsafeMutablePointers|withUnsafeMutablePointer|lexicographicalCompare|withExtendedLifetime|preconditionFailure|withUnsafePointers|underestimateCount|withUnsafePointer|assertionFailure|unsafeAddressOf|unsafeDowncast|countElements|unsafeBitCast|toDebugString|strideofValue|removeAtIndex|alignofValue|debugPrintln|precondition|sizeofValue|removeRange|numericCast|removeLast|debugPrint|fatalError|minElement|maxElement|startsWith|withVaList|partition|removeAll|transcode|dropFirst|enumerate|getVaList|strideof|contains|overlaps|dropLast|distance|toString|advance|println|alignof|reflect|indices|reverse|isEmpty|prefix|stride|sizeof|insert|assert|sorted|splice|filter|extend|reduce|suffix|equal|print|count|split|first|swap|dump|sort|lazy|last|join|find|abs|min|max|map)\b").token(T::NameBuiltinPseudo),
            rule(r"\$\d+").token(T::NameVariable),
            rule(r"0b[01_]+").token(T::LiteralNumberBin),
            rule(r"0o[0-7_]+").token(T::LiteralNumberOct),
            rule(r"0x[0-9a-fA-F_]+").token(T::LiteralNumberHex),
            rule(r"[0-9][0-9_]*(\.[0-9_]+[eE][+\-]?[0-9_]+|\.[0-9_]*|[eE][+\-]?[0-9_]+)").token(T::LiteralNumberFloat),
            rule(r"[0-9][0-9_]*").token(T::LiteralNumberInteger),
            rule(r#"""#).token(T::LiteralString).push(&["string"]),
            rule(r"[(){}\[\].,:;=@#`?]|->|[<&?](?=\w)|(?<=\w)[>!?]").token(T::Punctuation),
            rule(r"[/=\-+!*%<>&|^?~]+").token(T::Operator),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
        ]),
        ("comment-multi", &[
            include("comment"),
            rule(r"[^*/]").token(T::CommentMultiline),
            rule(r"/\*").token(T::CommentMultiline).push(&[]),
            rule(r"\*/").token(T::CommentMultiline).pop(1),
            rule(r"[*/]").token(T::CommentMultiline),
        ]),
        ("keywords", &[
            rule(r"(fallthrough|#selector|continue|default|repeat|switch|return|throw|catch|where|break|guard|defer|while|case|else|try|for|if|do|is|in|as)\b").token(T::Keyword),
            rule(r"@availability\([^)]+\)").token(T::KeywordReserved),
            rule(r"(@UIApplicationMain|@NSApplicationMain|@IBInspectable|@availability|@IBDesignable|associativity|@autoclosure|convenience|nonmutating|@NSManaged|@NSCopying|precedence|@IBAction|@noreturn|@IBOutlet|override|optional|mutating|indirect|Protocol|rethrows|required|willSet|dynamic|postfix|unowned|throws|prefix|didSet|final|inout|@objc|infix|right|lazy|none|weak|Type|left|get|set)\b").token(T::KeywordReserved),
            rule(r"(as|dynamicType|false|is|nil|self|Self|super|true|__COLUMN__|__FILE__|__FUNCTION__|__LINE__|_|#(?:file|line|column|function))\b").token(T::KeywordConstant),
            rule(r"import\b").token(T::KeywordDeclaration).push(&["module"]),
            rule(r"(class|enum|extension|struct|protocol)(\s+)([a-zA-Z_]\w*)").groups(&[T::KeywordDeclaration, T::Text, T::NameClass]),
            rule(r"(func)(\s+)([a-zA-Z_]\w*)").groups(&[T::KeywordDeclaration, T::Text, T::NameFunction]),
            rule(r"(var|let)(\s+)([a-zA-Z_]\w*)").groups(&[T::KeywordDeclaration, T::Text, T::NameVariable]),
            rule(r"(typealias|extension|subscript|internal|protocol|operator|private|public|struct|deinit|static|import|class|init|func|enum|let|var)\b").token(T::KeywordDeclaration),
        ]),
    ],
};

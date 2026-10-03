//! Chroma's `actionscript.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "actionscript",
    config: ConfigDef {
        name: "ActionScript",
        aliases: &["as", "actionscript"],
        filenames: &["*.as"],
        mime_types: &["application/x-actionscript", "text/x-actionscript", "text/actionscript"],
        dot_all: true,
        not_multiline: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::Text),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
            rule(r"/(\\\\|\\/|[^/\n])*/[gim]*").token(T::LiteralStringRegex),
            rule(r"[~^*!%&<>|+=:;,/?\\-]+").token(T::Operator),
            rule(r"[{}\[\]();.]+").token(T::Punctuation),
            rule(r"(instanceof|arguments|continue|default|typeof|switch|return|catch|break|while|throw|each|this|with|else|case|var|new|for|try|if|do|in)\b").token(T::Keyword),
            rule(r"(implements|protected|namespace|interface|intrinsic|override|function|internal|private|package|extends|dynamic|import|native|return|public|static|class|const|super|final|get|set)\b").token(T::KeywordDeclaration),
            rule(r"(true|false|null|NaN|Infinity|-Infinity|undefined|Void)\b").token(T::KeywordConstant),
            rule(r"(IDynamicPropertyOutputIDynamicPropertyWriter|DisplacmentMapFilterMode|AccessibilityProperties|ContextMenuBuiltInItems|SharedObjectFlushStatus|DisplayObjectContainer|IllegalOperationError|DisplacmentMapFilter|InterpolationMethod|URLLoaderDataFormat|PrintJobOrientation|ActionScriptVersion|BitmapFilterQuality|GradientBevelFilter|GradientGlowFilter|DeleteObjectSample|StackOverflowError|SoundLoaderContext|ScriptTimeoutError|SecurityErrorEvent|InteractiveObject|StageDisplayState|FileReferenceList|TextFieldAutoSize|ApplicationDomain|BitmapDataChannel|ColorMatrixFilter|ExternalInterface|IMEConversionMode|DropShadowFilter|URLRequestHeader|ContextMenuEvent|ConvultionFilter|URLRequestMethod|BitmapFilterType|IEventDispatcher|ContextMenuItem|LocalConnection|InvalidSWFError|AsyncErrorEvent|MovieClipLoader|IBitmapDrawable|PrintJobOptions|EventDispatcher|NewObjectSample|HTTPStatusEvent|TextFormatAlign|IExternalizable|FullScreenEvent|DefinitionError|TextLineMetrics|NetStatusEvent|ColorTransform|ObjectEncoding|SecurityDomain|StageScaleMode|FocusDirection|ReferenceError|SoundTransform|KeyboardEvent|DisplayObject|PixelSnapping|LoaderContext|NetConnection|SecurityPanel|SecurityError|FileReference|AsBroadcaster|LineScaleMode|AntiAliasType|Accessibility|TextFieldType|URLVariabeles|ActivityEvent|ProgressEvent|TextColorType|StageQuality|TextSnapshot|Capabilities|BitmapFilter|SpreadMethod|GradientType|TextRenderer|SoundChannel|SharedObject|IOErrorEvent|SimpleButton|ContextMenu|InvokeEvent|CSMSettings|SyntaxError|StatusEvent|KeyLocation|IDataOutput|VerifyError|XMLDocument|XMLNodeType|MemoryError|GridFitType|BevelFilter|ErrorEvent|FrameLabel|GlowFilter|LoaderInfo|Microphone|MorphShape|BlurFilter|MouseEvent|FocusEvent|SoundMixer|FileFilter|TimerEvent|JointStyle|EventPhase|StageAlign|Dictionary|URLRequest|StyleSheet|SWFVersion|IDataInput|StaticText|RangeError|BitmapData|TextFormat|StackFrame|Namespace|SyncEvent|Rectangle|URLLoader|TypeError|Responder|NetStream|BlendMode|CapsStyle|DataEvent|ByteArray|MovieClip|Transform|TextField|Selection|AVM1Movie|XMLSocket|URLStream|FontStyle|EvalError|FontType|LoadVars|Graphics|Security|IMEEvent|URIError|Keyboard|Function|EOFError|PrintJob|IOError|XMLList|Boolean|ID3Info|XMLNode|Bitmap|String|RegExp|Sample|Object|Sprite|System|Endian|Matrix|Camera|Locale|Number|Loader|Socket|QName|Class|Timer|Sound|Shape|XMLUI|Mouse|Scene|Stage|Color|Point|Video|Error|Event|Proxy|Array|Date|uint|Math|Font|int|Key|IME|XML)\b").token(T::NameBuiltin),
            rule(r"(decodeURIComponent|updateAfterEvent|clearInterval|setInterval|getVersion|parseFloat|fscommand|isXMLName|encodeURI|decodeURI|getTimer|unescape|isFinite|parseInt|getURL|escape|trace|isNaN|eval)\b").token(T::NameFunction),
            rule(r"[$a-zA-Z_]\w*").token(T::NameOther),
            rule(r"[0-9][0-9]*\.[0-9]+([eE][0-9]+)?[fd]?").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-f]+").token(T::LiteralNumberHex),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r"'(\\\\|\\'|[^'])*'").token(T::LiteralStringSingle),
        ]),
    ],
};

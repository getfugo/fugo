//! Chroma's `nsis.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "nsis",
    config: ConfigDef {
        name: "NSIS",
        aliases: &["nsis", "nsi", "nsh"],
        filenames: &["*.nsi", "*.nsh"],
        mime_types: &["text/x-nsis"],
        case_insensitive: true,
        not_multiline: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"([;#].*)(\n)").groups(&[T::Comment, T::TextWhitespace]),
            rule(r"'.*?'").token(T::LiteralStringSingle),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["str_double"]),
            rule(r"`").token(T::LiteralStringBacktick).push(&["str_backtick"]),
            include("macro"),
            include("interpol"),
            include("basic"),
            rule(r"\$\{[a-z_|][\w|]*\}").token(T::KeywordPseudo),
            rule(r"/[a-z_]\w*").token(T::NameAttribute),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"[\w.]+").token(T::Text),
        ]),
        ("basic", &[
            rule(r"(\n)(Function)(\s+)([._a-z][.\w]*)\b").groups(&[T::TextWhitespace, T::Keyword, T::TextWhitespace, T::NameFunction]),
            rule(r"\b([_a-z]\w*)(::)([a-z][a-z0-9]*)\b").groups(&[T::KeywordNamespace, T::Punctuation, T::NameFunction]),
            rule(r"\b([_a-z]\w*)(:)").groups(&[T::NameLabel, T::Punctuation]),
            rule(r"(\b[ULS]|\B)([!<>=]?=|\<\>?|\>)\B").token(T::Operator),
            rule(r"[|+-]").token(T::Operator),
            rule(r"\\").token(T::Punctuation),
            rule(r"\b(Abort|Add(?:BrandingImage|Size)|Allow(?:RootDirInstall|SkipFiles)|AutoCloseWindow|BG(?:Font|Gradient)|BrandingText|BringToFront|Call(?:InstDLL)?|(?:Sub)?Caption|ChangeUI|CheckBitmap|ClearErrors|CompletedText|ComponentText|CopyFiles|CRCCheck|Create(?:Directory|Font|Shortcut)|Delete(?:INI(?:Sec|Str)|Reg(?:Key|Value))?|DetailPrint|DetailsButtonText|Dir(?:Show|Text|Var|Verify)|(?:Disabled|Enabled)Bitmap|EnableWindow|EnumReg(?:Key|Value)|Exch|Exec(?:Shell|Wait)?|ExpandEnvStrings|File(?:BufSize|Close|ErrorText|Open|Read(?:Byte)?|Seek|Write(?:Byte)?)?|Find(?:Close|First|Next|Window)|FlushINI|Function(?:End)?|Get(?:CurInstType|CurrentAddress|DlgItem|DLLVersion(?:Local)?|ErrorLevel|FileTime(?:Local)?|FullPathName|FunctionAddress|InstDirError|LabelAddress|TempFileName)|Goto|HideWindow|Icon|If(?:Abort|Errors|FileExists|RebootFlag|Silent)|InitPluginsDir|Install(?:ButtonText|Colors|Dir(?:RegKey)?)|Inst(?:ProgressFlags|Type(?:[GS]etText)?)|Int(?:CmpU?|Fmt|Op)|IsWindow|LangString(?:UP)?|License(?:BkColor|Data|ForceSelection|LangString|Text)|LoadLanguageFile|LockWindow|Log(?:Set|Text)|MessageBox|MiscButtonText|Name|Nop|OutFile|(?:Uninst)?Page(?:Ex(?:End)?)?|PluginDir|Pop|Push|Quit|Read(?:(?:Env|INI|Reg)Str|RegDWORD)|Reboot|(?:Un)?RegDLL|Rename|RequestExecutionLevel|ReserveFile|Return|RMDir|SearchPath|Section(?:Divider|End|(?:(?:Get|Set)(?:Flags|InstTypes|Size|Text))|Group(?:End)?|In)?|SendMessage|Set(?:AutoClose|BrandingImage|Compress(?:ionLevel|or(?:DictSize)?)?|CtlColors|CurInstType|DatablockOptimize|DateSave|Details(?:Print|View)|Error(?:s|Level)|FileAttributes|Font|OutPath|Overwrite|PluginUnload|RebootFlag|ShellVarContext|Silent|StaticBkColor)|Show(?:(?:I|Uni)nstDetails|Window)|Silent(?:Un)?Install|Sleep|SpaceTexts|Str(?:CmpS?|Cpy|Len)|SubSection(?:End)?|Uninstall(?:ButtonText|(?:Sub)?Caption|EXEName|Icon|Text)|UninstPage|Var|VI(?:AddVersionKey|ProductVersion)|WindowIcon|Write(?:INIStr|Reg(:?Bin|DWORD|(?:Expand)?Str)|Uninstaller)|XPStyle)\b").token(T::Keyword),
            rule(r"\b(CUR|END|(?:FILE_ATTRIBUTE_)?(?:ARCHIVE|HIDDEN|NORMAL|OFFLINE|READONLY|SYSTEM|TEMPORARY)|HK(CC|CR|CU|DD|LM|PD|U)|HKEY_(?:CLASSES_ROOT|CURRENT_(?:CONFIG|USER)|DYN_DATA|LOCAL_MACHINE|PERFORMANCE_DATA|USERS)|ID(?:ABORT|CANCEL|IGNORE|NO|OK|RETRY|YES)|MB_(?:ABORTRETRYIGNORE|DEFBUTTON[1-4]|ICON(?:EXCLAMATION|INFORMATION|QUESTION|STOP)|OK(?:CANCEL)?|RETRYCANCEL|RIGHT|SETFOREGROUND|TOPMOST|USERICON|YESNO(?:CANCEL)?)|SET|SHCTX|SW_(?:HIDE|SHOW(?:MAXIMIZED|MINIMIZED|NORMAL))|admin|all|auto|both|bottom|bzip2|checkbox|colored|current|false|force|hide|highest|if(?:diff|newer)|lastused|leave|left|listonly|lzma|nevershow|none|normal|off|on|pop|push|radiobuttons|right|show|silent|silentlog|smooth|textonly|top|true|try|user|zlib)\b").token(T::NameConstant),
        ]),
        ("macro", &[
            rule(r"\!(addincludedir(?:dir)?|addplugindir|appendfile|cd|define|delfilefile|echo(?:message)?|else|endif|error|execute|if(?:macro)?n?(?:def)?|include|insertmacro|macro(?:end)?|packhdr|search(?:parse|replace)|system|tempfilesymbol|undef|verbose|warning)\b").token(T::CommentPreproc),
        ]),
        ("interpol", &[
            rule(r"\$(R?[0-9])").token(T::NameBuiltinPseudo),
            rule(r"\$(ADMINTOOLS|APPDATA|CDBURN_AREA|COOKIES|COMMONFILES(?:32|64)|DESKTOP|DOCUMENTS|EXE(?:DIR|FILE|PATH)|FAVORITES|FONTS|HISTORY|HWNDPARENT|INTERNET_CACHE|LOCALAPPDATA|MUSIC|NETHOOD|PICTURES|PLUGINSDIR|PRINTHOOD|PROFILE|PROGRAMFILES(?:32|64)|QUICKLAUNCH|RECENT|RESOURCES(?:_LOCALIZED)?|SENDTO|SM(?:PROGRAMS|STARTUP)|STARTMENU|SYSDIR|TEMP(?:LATES)?|VIDEOS|WINDIR|\{NSISDIR\})").token(T::NameBuiltin),
            rule(r"\$(CMDLINE|INSTDIR|OUTDIR|LANGUAGE)").token(T::NameVariableGlobal),
            rule(r"\$[a-z_]\w*").token(T::NameVariable),
        ]),
        ("str_double", &[
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            rule(r#"\$(\\[nrt"]|\$)"#).token(T::LiteralStringEscape),
            include("interpol"),
            rule(r#"[^"]+"#).token(T::LiteralStringDouble),
        ]),
        ("str_backtick", &[
            rule(r"`").token(T::LiteralStringDouble).pop(1),
            rule(r#"\$(\\[nrt"]|\$)"#).token(T::LiteralStringEscape),
            include("interpol"),
            rule(r"[^`]+").token(T::LiteralStringDouble),
        ]),
    ],
};

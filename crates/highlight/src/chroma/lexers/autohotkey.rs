//! Chroma's `autohotkey.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "autohotkey",
    config: ConfigDef {
        name: "AutoHotkey",
        aliases: &["autohotkey", "ahk"],
        filenames: &["*.ahk", "*.ahkl"],
        mime_types: &["text/x-autohotkey"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"^(\s*)(/\*)").groups(&[T::Text, T::CommentMultiline]).push(&["incomment"]),
            rule(r"^(\s*)(\()").groups(&[T::Text, T::Generic]).push(&["incontinuation"]),
            rule(r"\s+;.*?$").token(T::CommentSingle),
            rule(r"^;.*?$").token(T::CommentSingle),
            rule(r"[]{}(),;[]").token(T::Punctuation),
            rule(r"(in|is|and|or|not)\b").token(T::OperatorWord),
            rule(r"\%[a-zA-Z_#@$][\w#@$]*\%").token(T::NameVariable),
            rule(r"!=|==|:=|\.=|<<|>>|[-~+/*%=<>&^|?:!.]").token(T::Operator),
            include("commands"),
            include("labels"),
            include("builtInFunctions"),
            include("builtInVariables"),
            rule(r#"""#).token(T::LiteralString).combined(&["stringescape", "dqs"]),
            include("numbers"),
            rule(r"[a-zA-Z_#@$][\w#@$]*").token(T::Name),
            rule(r"\\|\'").token(T::Text),
            rule(r"\`([,%`abfnrtv\-+;])").token(T::LiteralStringEscape),
            include("garbage"),
        ]),
        ("incomment", &[
            rule(r"^\s*\*/").token(T::CommentMultiline).pop(1),
            rule(r"[^*]+").token(T::CommentMultiline),
            rule(r"\*").token(T::CommentMultiline),
        ]),
        ("incontinuation", &[
            rule(r"^\s*\)").token(T::Generic).pop(1),
            rule(r"[^)]").token(T::Generic),
            rule(r"[)]").token(T::Generic),
        ]),
        ("commands", &[
            rule(r"(?i)^(\s*)(global|local|static|#AllowSameLineComments|#ClipboardTimeout|#CommentFlag|#ErrorStdOut|#EscapeChar|#HotkeyInterval|#HotkeyModifierTimeout|#Hotstring|#IfWinActive|#IfWinExist|#IfWinNotActive|#IfWinNotExist|#IncludeAgain|#Include|#InstallKeybdHook|#InstallMouseHook|#KeyHistory|#LTrim|#MaxHotkeysPerInterval|#MaxMem|#MaxThreads|#MaxThreadsBuffer|#MaxThreadsPerHotkey|#NoEnv|#NoTrayIcon|#Persistent|#SingleInstance|#UseHook|#WinActivateForce|AutoTrim|BlockInput|Break|Click|ClipWait|Continue|Control|ControlClick|ControlFocus|ControlGetFocus|ControlGetPos|ControlGetText|ControlGet|ControlMove|ControlSend|ControlSendRaw|ControlSetText|CoordMode|Critical|DetectHiddenText|DetectHiddenWindows|Drive|DriveGet|DriveSpaceFree|Edit|Else|EnvAdd|EnvDiv|EnvGet|EnvMult|EnvSet|EnvSub|EnvUpdate|Exit|ExitApp|FileAppend|FileCopy|FileCopyDir|FileCreateDir|FileCreateShortcut|FileDelete|FileGetAttrib|FileGetShortcut|FileGetSize|FileGetTime|FileGetVersion|FileInstall|FileMove|FileMoveDir|FileRead|FileReadLine|FileRecycle|FileRecycleEmpty|FileRemoveDir|FileSelectFile|FileSelectFolder|FileSetAttrib|FileSetTime|FormatTime|GetKeyState|Gosub|Goto|GroupActivate|GroupAdd|GroupClose|GroupDeactivate|Gui|GuiControl|GuiControlGet|Hotkey|IfEqual|IfExist|IfGreaterOrEqual|IfGreater|IfInString|IfLess|IfLessOrEqual|IfMsgBox|IfNotEqual|IfNotExist|IfNotInString|IfWinActive|IfWinExist|IfWinNotActive|IfWinNotExist|If |ImageSearch|IniDelete|IniRead|IniWrite|InputBox|Input|KeyHistory|KeyWait|ListHotkeys|ListLines|ListVars|Loop|Menu|MouseClickDrag|MouseClick|MouseGetPos|MouseMove|MsgBox|OnExit|OutputDebug|Pause|PixelGetColor|PixelSearch|PostMessage|Process|Progress|Random|RegDelete|RegRead|RegWrite|Reload|Repeat|Return|RunAs|RunWait|Run|SendEvent|SendInput|SendMessage|SendMode|SendPlay|SendRaw|Send|SetBatchLines|SetCapslockState|SetControlDelay|SetDefaultMouseSpeed|SetEnv|SetFormat|SetKeyDelay|SetMouseDelay|SetNumlockState|SetScrollLockState|SetStoreCapslockMode|SetTimer|SetTitleMatchMode|SetWinDelay|SetWorkingDir|Shutdown|Sleep|Sort|SoundBeep|SoundGet|SoundGetWaveVolume|SoundPlay|SoundSet|SoundSetWaveVolume|SplashImage|SplashTextOff|SplashTextOn|SplitPath|StatusBarGetText|StatusBarWait|StringCaseSense|StringGetPos|StringLeft|StringLen|StringLower|StringMid|StringReplace|StringRight|StringSplit|StringTrimLeft|StringTrimRight|StringUpper|Suspend|SysGet|Thread|ToolTip|Transform|TrayTip|URLDownloadToFile|While|WinActivate|WinActivateBottom|WinClose|WinGetActiveStats|WinGetActiveTitle|WinGetClass|WinGetPos|WinGetText|WinGetTitle|WinGet|WinHide|WinKill|WinMaximize|WinMenuSelectItem|WinMinimizeAllUndo|WinMinimizeAll|WinMinimize|WinMove|WinRestore|WinSetTitle|WinSet|WinShow|WinWaitActive|WinWaitClose|WinWaitNotActive|WinWait)\b").groups(&[T::Text, T::NameBuiltin]),
        ]),
        ("builtInFunctions", &[
            rule(r"(?i)(Abs|ACos|Asc|ASin|ATan|Ceil|Chr|Cos|DllCall|Exp|FileExist|Floor|GetKeyState|IL_Add|IL_Create|IL_Destroy|InStr|IsFunc|IsLabel|Ln|Log|LV_Add|LV_Delete|LV_DeleteCol|LV_GetCount|LV_GetNext|LV_GetText|LV_Insert|LV_InsertCol|LV_Modify|LV_ModifyCol|LV_SetImageList|Mod|NumGet|NumPut|OnMessage|RegExMatch|RegExReplace|RegisterCallback|Round|SB_SetIcon|SB_SetParts|SB_SetText|Sin|Sqrt|StrLen|SubStr|Tan|TV_Add|TV_Delete|TV_GetChild|TV_GetCount|TV_GetNext|TV_Get|TV_GetParent|TV_GetPrev|TV_GetSelection|TV_GetText|TV_Modify|VarSetCapacity|WinActive|WinExist|Object|ComObjActive|ComObjArray|ComObjEnwrap|ComObjUnwrap|ComObjParameter|ComObjType|ComObjConnect|ComObjCreate|ComObjGet|ComObjError|ComObjValue|Insert|MinIndex|MaxIndex|Remove|SetCapacity|GetCapacity|GetAddress|_NewEnum|FileOpen|Read|Write|ReadLine|WriteLine|ReadNumType|WriteNumType|RawRead|RawWrite|Seek|Tell|Close|Next|IsObject|StrPut|StrGet|Trim|LTrim|RTrim)\b").token(T::NameFunction),
        ]),
        ("builtInVariables", &[
            rule(r"(?i)(A_AhkPath|A_AhkVersion|A_AppData|A_AppDataCommon|A_AutoTrim|A_BatchLines|A_CaretX|A_CaretY|A_ComputerName|A_ControlDelay|A_Cursor|A_DDDD|A_DDD|A_DD|A_DefaultMouseSpeed|A_Desktop|A_DesktopCommon|A_DetectHiddenText|A_DetectHiddenWindows|A_EndChar|A_EventInfo|A_ExitReason|A_FormatFloat|A_FormatInteger|A_Gui|A_GuiEvent|A_GuiControl|A_GuiControlEvent|A_GuiHeight|A_GuiWidth|A_GuiX|A_GuiY|A_Hour|A_IconFile|A_IconHidden|A_IconNumber|A_IconTip|A_Index|A_IPAddress1|A_IPAddress2|A_IPAddress3|A_IPAddress4|A_ISAdmin|A_IsCompiled|A_IsCritical|A_IsPaused|A_IsSuspended|A_KeyDelay|A_Language|A_LastError|A_LineFile|A_LineNumber|A_LoopField|A_LoopFileAttrib|A_LoopFileDir|A_LoopFileExt|A_LoopFileFullPath|A_LoopFileLongPath|A_LoopFileName|A_LoopFileShortName|A_LoopFileShortPath|A_LoopFileSize|A_LoopFileSizeKB|A_LoopFileSizeMB|A_LoopFileTimeAccessed|A_LoopFileTimeCreated|A_LoopFileTimeModified|A_LoopReadLine|A_LoopRegKey|A_LoopRegName|A_LoopRegSubkey|A_LoopRegTimeModified|A_LoopRegType|A_MDAY|A_Min|A_MM|A_MMM|A_MMMM|A_Mon|A_MouseDelay|A_MSec|A_MyDocuments|A_Now|A_NowUTC|A_NumBatchLines|A_OSType|A_OSVersion|A_PriorHotkey|A_ProgramFiles|A_Programs|A_ProgramsCommon|A_ScreenHeight|A_ScreenWidth|A_ScriptDir|A_ScriptFullPath|A_ScriptName|A_Sec|A_Space|A_StartMenu|A_StartMenuCommon|A_Startup|A_StartupCommon|A_StringCaseSense|A_Tab|A_Temp|A_ThisFunc|A_ThisHotkey|A_ThisLabel|A_ThisMenu|A_ThisMenuItem|A_ThisMenuItemPos|A_TickCount|A_TimeIdle|A_TimeIdlePhysical|A_TimeSincePriorHotkey|A_TimeSinceThisHotkey|A_TitleMatchMode|A_TitleMatchModeSpeed|A_UserName|A_WDay|A_WinDelay|A_WinDir|A_WorkingDir|A_YDay|A_YEAR|A_YWeek|A_YYYY|Clipboard|ClipboardAll|ComSpec|ErrorLevel|ProgramFiles|True|False|A_IsUnicode|A_FileEncoding|A_OSVersion|A_PtrSize)\b").token(T::NameVariable),
        ]),
        ("labels", &[
            rule(r#"(^\s*)([^:\s("]+?:{1,2})"#).groups(&[T::Text, T::NameLabel]),
            rule(r"(^\s*)(::[^:\s]+?::)").groups(&[T::Text, T::NameLabel]),
        ]),
        ("numbers", &[
            rule(r"(\d+\.\d*|\d*\.\d+)([eE][+-]?[0-9]+)?").token(T::LiteralNumberFloat),
            rule(r"\d+[eE][+-]?[0-9]+").token(T::LiteralNumberFloat),
            rule(r"0\d+").token(T::LiteralNumberOct),
            rule(r"0[xX][a-fA-F0-9]+").token(T::LiteralNumberHex),
            rule(r"\d+L").token(T::LiteralNumberIntegerLong),
            rule(r"\d+").token(T::LiteralNumberInteger),
        ]),
        ("stringescape", &[
            rule(r#"\"\"|\`([,%`abfnrtv])"#).token(T::LiteralStringEscape),
        ]),
        ("strings", &[
            rule(r#"[^"\n]+"#).token(T::LiteralString),
        ]),
        ("dqs", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            include("strings"),
        ]),
        ("garbage", &[
            rule(r"[^\S\n]").token(T::Text),
        ]),
    ],
};

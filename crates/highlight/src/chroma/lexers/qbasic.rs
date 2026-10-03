//! Chroma's `qbasic.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "qbasic",
    config: ConfigDef {
        name: "QBasic",
        aliases: &["qbasic", "basic"],
        filenames: &["*.BAS", "*.bas"],
        mime_types: &["text/basic"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\n+").token(T::Text),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"^(\s*)(\d*)(\s*)(REM .*)$").groups(&[T::TextWhitespace, T::NameLabel, T::TextWhitespace, T::CommentSingle]),
            rule(r"^(\s*)(\d+)(\s*)").groups(&[T::TextWhitespace, T::NameLabel, T::TextWhitespace]),
            rule(r"(?=[\s]*)(\w+)(?=[\s]*=)").token(T::NameVariableGlobal),
            rule(r#"(?=[^"]*)\'.*$"#).token(T::CommentSingle),
            rule(r#""[^\n"]*""#).token(T::LiteralStringDouble),
            rule(r"(END)(\s+)(FUNCTION|IF|SELECT|SUB)").groups(&[T::KeywordReserved, T::TextWhitespace, T::KeywordReserved]),
            rule(r"(DECLARE)(\s+)([A-Z]+)(\s+)(\S+)").groups(&[T::KeywordDeclaration, T::TextWhitespace, T::NameVariable, T::TextWhitespace, T::Name]),
            rule(r"(DIM)(\s+)(SHARED)(\s+)([^\s(]+)").groups(&[T::KeywordDeclaration, T::TextWhitespace, T::NameVariable, T::TextWhitespace, T::NameVariableGlobal]),
            rule(r"(DIM)(\s+)([^\s(]+)").groups(&[T::KeywordDeclaration, T::TextWhitespace, T::NameVariableGlobal]),
            rule(r"^(\s*)([a-zA-Z_]+)(\s*)(\=)").groups(&[T::TextWhitespace, T::NameVariableGlobal, T::TextWhitespace, T::Operator]),
            rule(r"(GOTO|GOSUB)(\s+)(\w+\:?)").groups(&[T::KeywordReserved, T::TextWhitespace, T::NameLabel]),
            rule(r"(SUB)(\s+)(\w+\:?)").groups(&[T::KeywordReserved, T::TextWhitespace, T::NameLabel]),
            include("declarations"),
            include("functions"),
            include("metacommands"),
            include("operators"),
            include("statements"),
            include("keywords"),
            rule(r"[a-zA-Z_]\w*[$@#&!]").token(T::NameVariableGlobal),
            rule(r"[a-zA-Z_]\w*\:").token(T::NameLabel),
            rule(r"\-?\d*\.\d+[@|#]?").token(T::LiteralNumberFloat),
            rule(r"\-?\d+[@|#]").token(T::LiteralNumberFloat),
            rule(r"\-?\d+#?").token(T::LiteralNumberIntegerLong),
            rule(r"\-?\d+#?").token(T::LiteralNumberInteger),
            rule(r"!=|==|:=|\.=|<<|>>|[-~+/\\*%=<>&^|?:!.]").token(T::Operator),
            rule(r"[\[\]{}(),;]").token(T::Punctuation),
            rule(r"[\w]+").token(T::NameVariableGlobal),
        ]),
        ("declarations", &[
            rule(r"\b(DATA|LET)(?=\(|\b)").token(T::KeywordDeclaration),
        ]),
        ("functions", &[
            rule(r"\b(ABS|ASC|ATN|CDBL|CHR\$|CINT|CLNG|COMMAND\$|COS|CSNG|CSRLIN|CVD|CVDMBF|CVI|CVL|CVS|CVSMBF|DATE\$|ENVIRON\$|EOF|ERDEV|ERDEV\$|ERL|ERR|EXP|FILEATTR|FIX|FRE|FREEFILE|HEX\$|INKEY\$|INP|INPUT\$|INSTR|INT|IOCTL\$|LBOUND|LCASE\$|LEFT\$|LEN|LOC|LOF|LOG|LPOS|LTRIM\$|MID\$|MKD\$|MKDMBF\$|MKI\$|MKL\$|MKS\$|MKSMBF\$|OCT\$|PEEK|PEN|PLAY|PMAP|POINT|POS|RIGHT\$|RND|RTRIM\$|SADD|SCREEN|SEEK|SETMEM|SGN|SIN|SPACE\$|SPC|SQR|STICK|STR\$|STRIG|STRING\$|TAB|TAN|TIME\$|TIMER|UBOUND|UCASE\$|VAL|VARPTR|VARPTR\$|VARSEG)(?=\(|\b)").token(T::KeywordReserved),
        ]),
        ("metacommands", &[
            rule(r"\b(\$DYNAMIC|\$INCLUDE|\$STATIC)(?=\(|\b)").token(T::KeywordConstant),
        ]),
        ("operators", &[
            rule(r"\b(AND|EQV|IMP|NOT|OR|XOR)(?=\(|\b)").token(T::OperatorWord),
        ]),
        ("statements", &[
            rule(r"\b(BEEP|BLOAD|BSAVE|CALL|CALL\ ABSOLUTE|CALL\ INTERRUPT|CALLS|CHAIN|CHDIR|CIRCLE|CLEAR|CLOSE|CLS|COLOR|COM|COMMON|CONST|DATA|DATE\$|DECLARE|DEF\ FN|DEF\ SEG|DEFDBL|DEFINT|DEFLNG|DEFSNG|DEFSTR|DEF|DIM|DO|LOOP|DRAW|END|ENVIRON|ERASE|ERROR|EXIT|FIELD|FILES|FOR|NEXT|FUNCTION|GET|GOSUB|GOTO|IF|THEN|INPUT|INPUT\ \#|IOCTL|KEY|KEY|KILL|LET|LINE|LINE\ INPUT|LINE\ INPUT\ \#|LOCATE|LOCK|UNLOCK|LPRINT|LSET|MID\$|MKDIR|NAME|ON\ COM|ON\ ERROR|ON\ KEY|ON\ PEN|ON\ PLAY|ON\ STRIG|ON\ TIMER|ON\ UEVENT|ON|OPEN|OPEN\ COM|OPTION\ BASE|OUT|PAINT|PALETTE|PCOPY|PEN|PLAY|POKE|PRESET|PRINT|PRINT\ \#|PRINT\ USING|PSET|PUT|PUT|RANDOMIZE|READ|REDIM|REM|RESET|RESTORE|RESUME|RETURN|RMDIR|RSET|RUN|SCREEN|SEEK|SELECT\ CASE|SHARED|SHELL|SLEEP|SOUND|STATIC|STOP|STRIG|SUB|SWAP|SYSTEM|TIME\$|TIMER|TROFF|TRON|TYPE|UEVENT|UNLOCK|VIEW|WAIT|WHILE|WEND|WIDTH|WINDOW|WRITE)\b").token(T::KeywordReserved),
        ]),
        ("keywords", &[
            rule(r"\b(ACCESS|ALIAS|ANY|APPEND|AS|BASE|BINARY|BYVAL|CASE|CDECL|DOUBLE|ELSE|ELSEIF|ENDIF|INTEGER|IS|LIST|LOCAL|LONG|LOOP|MOD|NEXT|OFF|ON|OUTPUT|RANDOM|SIGNAL|SINGLE|STEP|STRING|THEN|TO|UNTIL|USING|WEND)\b").token(T::Keyword),
        ]),
    ],
};

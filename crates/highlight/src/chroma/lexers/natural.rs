//! Chroma's `natural.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "natural",
    config: ConfigDef {
        name: "Natural",
        aliases: &["natural"],
        filenames: &[
            "*.NSN",
            "*.NSP",
            "*.NSS",
            "*.NSH",
            "*.NSG",
            "*.NSL",
            "*.NSA",
            "*.NSM",
            "*.NSC",
            "*.NS7",
        ],
        mime_types: &["text/x-natural"],
        case_insensitive: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("common", &[
            rule(r"\s+").token(T::Text),
            rule(r"^\*.*$").token(T::CommentSingle),
            rule(r"/\*.*$").token(T::CommentSingle),
        ]),
        ("variable-names", &[
            rule(r"[#+]?[\w\-\d]+").token(T::NameVariable),
            rule(r"\([a-zA-z]\d*\)").token(T::Other),
        ]),
        ("root", &[
            include("common"),
            rule(r"(?:END-DEFINE|END-IF|END-FOR|END-SUBROUTINE|END-ERROR|END|IGNORE)\b").token(T::Keyword),
            rule(r"(?:INIT|CONST)\s*<\b").token(T::Keyword),
            rule(r"(FORM)(\s+)(\w+)").groups(&[T::Keyword, T::Text, T::NameFunction]),
            rule(r"(DEFINE)(\s+)(SUBROUTINE)(\s+)([#+]?[\w\-\d]+)").groups(&[T::Keyword, T::Text, T::Keyword, T::Text, T::NameFunction]),
            rule(r"(PERFORM)(\s+)([#+]?[\w\-\d]+)").groups(&[T::Keyword, T::Text, T::NameFunction]),
            rule(r"(METHOD)(\s+)([\w~]+)").groups(&[T::Keyword, T::Text, T::NameFunction]),
            rule(r"(\s+)([\w\-]+)([=\-]>)([\w\-~]+)").groups(&[T::Text, T::NameVariable, T::Operator, T::NameFunction]),
            rule(r"(?<=(=|-)>)([\w\-~]+)(?=\()").token(T::NameFunction),
            rule(r"(TEXT)(-)(\d{3})").groups(&[T::Keyword, T::Punctuation, T::LiteralNumberInteger]),
            rule(r"(TEXT)(-)(\w{3})").groups(&[T::Keyword, T::Punctuation, T::NameVariable]),
            rule(r"(?i)\b(?<!-)(?<!#)(ENTIRE|BY|NAME|ARRAY|SPECIFIED|VIEW|MODULE|FUNCTION|RETURNS|AND|NUMERIC|OPTIONAL|END-PARSE|TRUE|END-RESULT|LEAVING|NOT|CONDITION|NUMBER|NO|EXP|FULL|REPLACE|INSERT|DOEND|LOG|ABS|ANY|REPEAT|SET|DLOGOFF|DOWNLOAD|BREAK|VALUES|DIVIDE|COMPRESS|UPDATE|SORTKEY|OR|END-FIND|END-ENDPAGE|REDUCE|IGNORE|MIN|WASTE|END-DEFINE|SUBSTR|END|FIND|ADD|INVESTIGATE|DNATIVE|CONST|COS|ENDHOC|SGN|COPY|REDEFINE|DEFINE|MULTIPLY|ASSIGN|LE|VALUE|COMPOSE|FALSE|POS|CALL|TAN|ERROR|CLOSE|PARSE|LT|WITH_CTE|END-SORT|EJECT|RESET|SHOW|LOCAL|PERFORM|TERMINATE|VAL|BACKOUT|END-LOOP|REJECT|SUM|CREATE|SORT|RETURN|AT|SIN|SETTIME|INT|NE|GLOBAL|END-SELECT|ELSE|DELETE|TOP|INCLUDE|END-ENDDATA|LOOP|OLD|SUSPEND|SKIP|SQRT|RULEVAR|NMIN|AVER|PROCESS|SELECT|MAP|USING|END-HISTOGRAM|MAX|NEWPAGE|ON|OFF|KEY|NAMED|CONTROL|PF1|PF2|PF3|PF4|PF5|PF6|PF7|PF8|PF9|INITIAL|WRITE|STORE|FETCH|ATN|RET|END-WORK|RESTORE|GET|LIMIT|END-ERROR|SEND|OPEN|ESCAPE|COMPUTE|COUNT|TRANSFER|RELEASE|DO|DYNAMIC|ROLLBACK|END-READ|DISPLAY|UPLOAD|END-DATA|NULL-HANDLE|NCOUNT|RESIZE|END-PROCESS|REQUEST|READ|SEPARATE|EQ|INPUT|DATA|END-START|STACK|REINPUT|INCDIC|INCCONT|END-IF|WHEN|END-BEFORE|WHILE|END-ENDFILE|END-TOPPAGE|INCDIR|PARAMETER|OBTAIN|CALLDBPROC|END-BROWSE|MOVE|SUBTRACT|DLOGON|EXAMINE|SUBSTRING|BEFORE|STOP|RUN|END-BREAK|EXPORT|END-SUBROUTINE|FOR|GE|PRINT|BROWSE|IMPORT|EXPAND|ALL|PASSW|FORMAT|GT|END-NOREC|END-DECIDE|END-FOR|CALLNAT|END-ALL|OPTIONS|RETRY|NONE|INCMAC|END-FILE|DECIDE|INIT|HISTOGRAM|NAVER|START|ACCEPT|COMMIT|TOTAL|IF|FRAC|END-REPEAT|UNTIL|TO|INTO|WITH|DELIMITER|FIRST|OF|INTO|SUBROUTINE|GIVING|POSITION)\b(?!-)").token(T::Keyword),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r"(?<=(\s|.))(AND|OR|NOT|EQUAL|NE|EQ|GT|GE|LT|LE)\b").token(T::OperatorWord),
            include("variable-names"),
            rule(r"[?*<>=\-+&]").token(T::Operator),
            rule(r"(?:(ESCAPE)(\s+)(MODULE|ROUTINE|BOTTOM|TOP))").groups(&[T::Keyword, T::Text, T::Keyword]),
            rule(r"'(''|[^'])*'").token(T::LiteralStringSingle),
            rule(r"`([^`])*`").token(T::LiteralStringSingle),
            rule(r"([|}])([^{}|]*?)([|{])").groups(&[T::Punctuation, T::LiteralStringSingle, T::Punctuation]),
            rule(r"[/;:()\[\],.]").token(T::Punctuation),
        ]),
    ],
};

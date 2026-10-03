//! Chroma's `abap.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "abap",
    config: ConfigDef {
        name: "ABAP",
        aliases: &["abap"],
        filenames: &["*.abap", "*.ABAP"],
        mime_types: &["text/x-abap"],
        case_insensitive: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("common", &[
            rule(r"\s+").token(T::Text),
            rule(r"^\*.*$").token(T::CommentSingle),
            rule(r#"\".*?\n"#).token(T::CommentSingle),
            rule(r"##\w+").token(T::CommentSpecial),
        ]),
        ("variable-names", &[
            rule(r"<\S+>").token(T::NameVariable),
            rule(r"\w[\w~]*(?:(\[\])|->\*)?").token(T::NameVariable),
        ]),
        ("root", &[
            include("common"),
            rule(r"CALL\s+(?:BADI|CUSTOMER-FUNCTION|FUNCTION)").token(T::Keyword),
            rule(r"(CALL\s+(?:DIALOG|SCREEN|SUBSCREEN|SELECTION-SCREEN|TRANSACTION|TRANSFORMATION))\b").token(T::Keyword),
            rule(r"(FORM|PERFORM)(\s+)(\w+)").groups(&[T::Keyword, T::Text, T::NameFunction]),
            rule(r"(PERFORM)(\s+)(\()(\w+)(\))").groups(&[T::Keyword, T::Text, T::Punctuation, T::NameVariable, T::Punctuation]),
            rule(r"(MODULE)(\s+)(\S+)(\s+)(INPUT|OUTPUT)").groups(&[T::Keyword, T::Text, T::NameFunction, T::Text, T::Keyword]),
            rule(r"(METHOD)(\s+)([\w~]+)").groups(&[T::Keyword, T::Text, T::NameFunction]),
            rule(r"(\s+)([\w\-]+)([=\-]>)([\w\-~]+)").groups(&[T::Text, T::NameVariable, T::Operator, T::NameFunction]),
            rule(r"(?<=(=|-)>)([\w\-~]+)(?=\()").token(T::NameFunction),
            rule(r"(TEXT)(-)(\d{3})").groups(&[T::Keyword, T::Punctuation, T::LiteralNumberInteger]),
            rule(r"(TEXT)(-)(\w{3})").groups(&[T::Keyword, T::Punctuation, T::NameVariable]),
            rule(r"(ADD-CORRESPONDING|AUTHORITY-CHECK|CLASS-DATA|CLASS-EVENTS|CLASS-METHODS|CLASS-POOL|DELETE-ADJACENT|DIVIDE-CORRESPONDING|EDITOR-CALL|ENHANCEMENT-POINT|ENHANCEMENT-SECTION|EXIT-COMMAND|FIELD-GROUPS|FIELD-SYMBOLS|FUNCTION-POOL|INTERFACE-POOL|INVERTED-DATE|LOAD-OF-PROGRAM|LOG-POINT|MESSAGE-ID|MOVE-CORRESPONDING|MULTIPLY-CORRESPONDING|NEW-LINE|NEW-PAGE|NEW-SECTION|NO-EXTENSION|OUTPUT-LENGTH|PRINT-CONTROL|SELECT-OPTIONS|START-OF-SELECTION|SUBTRACT-CORRESPONDING|SYNTAX-CHECK|SYSTEM-EXCEPTIONS|TYPE-POOL|TYPE-POOLS|NO-DISPLAY)\b").token(T::Keyword),
            rule(r"(?<![-\>])(CREATE\s+(PUBLIC|PRIVATE|DATA|OBJECT)|(PUBLIC|PRIVATE|PROTECTED)\s+SECTION|(TYPE|LIKE)\s+((LINE\s+OF|REF\s+TO|(SORTED|STANDARD|HASHED)\s+TABLE\s+OF))?|FROM\s+(DATABASE|MEMORY)|CALL\s+METHOD|(GROUP|ORDER) BY|HAVING|SEPARATED BY|GET\s+(BADI|BIT|CURSOR|DATASET|LOCALE|PARAMETER|PF-STATUS|(PROPERTY|REFERENCE)\s+OF|RUN\s+TIME|TIME\s+(STAMP)?)?|SET\s+(BIT|BLANK\s+LINES|COUNTRY|CURSOR|DATASET|EXTENDED\s+CHECK|HANDLER|HOLD\s+DATA|LANGUAGE|LEFT\s+SCROLL-BOUNDARY|LOCALE|MARGIN|PARAMETER|PF-STATUS|PROPERTY\s+OF|RUN\s+TIME\s+(ANALYZER|CLOCK\s+RESOLUTION)|SCREEN|TITLEBAR|UPADTE\s+TASK\s+LOCAL|USER-COMMAND)|CONVERT\s+((INVERTED-)?DATE|TIME|TIME\s+STAMP|TEXT)|(CLOSE|OPEN)\s+(DATASET|CURSOR)|(TO|FROM)\s+(DATA BUFFER|INTERNAL TABLE|MEMORY ID|DATABASE|SHARED\s+(MEMORY|BUFFER))|DESCRIBE\s+(DISTANCE\s+BETWEEN|FIELD|LIST|TABLE)|FREE\s(MEMORY|OBJECT)?|PROCESS\s+(BEFORE\s+OUTPUT|AFTER\s+INPUT|ON\s+(VALUE-REQUEST|HELP-REQUEST))|AT\s+(LINE-SELECTION|USER-COMMAND|END\s+OF|NEW)|AT\s+SELECTION-SCREEN(\s+(ON(\s+(BLOCK|(HELP|VALUE)-REQUEST\s+FOR|END\s+OF|RADIOBUTTON\s+GROUP))?|OUTPUT))?|SELECTION-SCREEN:?\s+((BEGIN|END)\s+OF\s+((TABBED\s+)?BLOCK|LINE|SCREEN)|COMMENT|FUNCTION\s+KEY|INCLUDE\s+BLOCKS|POSITION|PUSHBUTTON|SKIP|ULINE)|LEAVE\s+(LIST-PROCESSING|PROGRAM|SCREEN|TO LIST-PROCESSING|TO TRANSACTION)(ENDING|STARTING)\s+AT|FORMAT\s+(COLOR|INTENSIFIED|INVERSE|HOTSPOT|INPUT|FRAMES|RESET)|AS\s+(CHECKBOX|SUBSCREEN|WINDOW)|WITH\s+(((NON-)?UNIQUE)?\s+KEY|FRAME)|(BEGIN|END)\s+OF|DELETE(\s+ADJACENT\s+DUPLICATES\sFROM)?|COMPARING(\s+ALL\s+FIELDS)?|(INSERT|APPEND)(\s+INITIAL\s+LINE\s+(IN)?TO|\s+LINES\s+OF)?|IN\s+((BYTE|CHARACTER)\s+MODE|PROGRAM)|END-OF-(DEFINITION|PAGE|SELECTION)|WITH\s+FRAME(\s+TITLE)|(REPLACE|FIND)\s+((FIRST|ALL)\s+OCCURRENCES?\s+OF\s+)?(SUBSTRING|REGEX)?|MATCH\s+(LENGTH|COUNT|LINE|OFFSET)|(RESPECTING|IGNORING)\s+CASE|IN\s+UPDATE\s+TASK|(SOURCE|RESULT)\s+(XML)?|REFERENCE\s+INTO|AND\s+(MARK|RETURN)|CLIENT\s+SPECIFIED|CORRESPONDING\s+FIELDS\s+OF|IF\s+FOUND|FOR\s+EVENT|INHERITING\s+FROM|LEAVE\s+TO\s+SCREEN|LOOP\s+AT\s+(SCREEN)?|LOWER\s+CASE|MATCHCODE\s+OBJECT|MODIF\s+ID|MODIFY\s+SCREEN|NESTING\s+LEVEL|NO\s+INTERVALS|OF\s+STRUCTURE|RADIOBUTTON\s+GROUP|RANGE\s+OF|REF\s+TO|SUPPRESS DIALOG|TABLE\s+OF|UPPER\s+CASE|TRANSPORTING\s+NO\s+FIELDS|VALUE\s+CHECK|VISIBLE\s+LENGTH|HEADER\s+LINE|COMMON\s+PART)\b").token(T::Keyword),
            rule(r"(^|(?<=(\s|\.)))(ABBREVIATED|ABSTRACT|ADD|ALIASES|ALIGN|ALPHA|ASSERT|AS|ASSIGN(ING)?|AT(\s+FIRST)?|BACK|BLOCK|BREAK-POINT|CASE|CATCH|CHANGING|CHECK|CLASS|CLEAR|COLLECT|COLOR|COMMIT|CREATE|COMMUNICATION|COMPONENTS?|COMPUTE|CONCATENATE|CONDENSE|CONSTANTS|CONTEXTS|CONTINUE|CONTROLS|COUNTRY|CURRENCY|DATA|DATE|DECIMALS|DEFAULT|DEFINE|DEFINITION|DEFERRED|DEMAND|DETAIL|DIRECTORY|DIVIDE|DO|DUMMY|ELSE(IF)?|ENDAT|ENDCASE|ENDCATCH|ENDCLASS|ENDDO|ENDFORM|ENDFUNCTION|ENDIF|ENDINTERFACE|ENDLOOP|ENDMETHOD|ENDMODULE|ENDSELECT|ENDTRY|ENDWHILE|ENHANCEMENT|EVENTS|EXACT|EXCEPTIONS?|EXIT|EXPONENT|EXPORT|EXPORTING|EXTRACT|FETCH|FIELDS?|FOR|FORM|FORMAT|FREE|FROM|FUNCTION|HIDE|ID|IF|IMPORT|IMPLEMENTATION|IMPORTING|IN|INCLUDE|INCLUDING|INDEX|INFOTYPES|INITIALIZATION|INTERFACE|INTERFACES|INTO|LANGUAGE|LEAVE|LENGTH|LINES|LOAD|LOCAL|JOIN|KEY|NEXT|MAXIMUM|MESSAGE|METHOD[S]?|MINIMUM|MODULE|MODIFIER|MODIFY|MOVE|MULTIPLY|NODES|NUMBER|OBLIGATORY|OBJECT|OF|OFF|ON|OTHERS|OVERLAY|PACK|PAD|PARAMETERS|PERCENTAGE|POSITION|PROGRAM|PROVIDE|PUBLIC|PUT|PF\d\d|RAISE|RAISING|RANGES?|READ|RECEIVE|REDEFINITION|REFRESH|REJECT|REPORT|RESERVE|RESUME|RETRY|RETURN|RETURNING|RIGHT|ROLLBACK|REPLACE|SCROLL|SEARCH|SELECT|SHIFT|SIGN|SINGLE|SIZE|SKIP|SORT|SPLIT|STATICS|STOP|STYLE|SUBMATCHES|SUBMIT|SUBTRACT|SUM(?!\()|SUMMARY|SUMMING|SUPPLY|TABLE|TABLES|TIMESTAMP|TIMES?|TIMEZONE|TITLE|\??TO|TOP-OF-PAGE|TRANSFER|TRANSLATE|TRY|TYPES|ULINE|UNDER|UNPACK|UPDATE|USING|VALUE|VALUES|VIA|VARYING|VARY|WAIT|WHEN|WHERE|WIDTH|WHILE|WITH|WINDOW|WRITE|XSD|ZERO)\b").token(T::Keyword),
            rule(r"(abs|acos|asin|atan|boolc|boolx|bit_set|char_off|charlen|ceil|cmax|cmin|condense|contains|contains_any_of|contains_any_not_of|concat_lines_of|cos|cosh|count|count_any_of|count_any_not_of|dbmaxlen|distance|escape|exp|find|find_end|find_any_of|find_any_not_of|floor|frac|from_mixed|insert|lines|log|log10|match|matches|nmax|nmin|numofchar|repeat|replace|rescale|reverse|round|segment|shift_left|shift_right|sign|sin|sinh|sqrt|strlen|substring|substring_after|substring_from|substring_before|substring_to|tan|tanh|to_upper|to_lower|to_mixed|translate|trunc|xstrlen)(\()\b").groups(&[T::NameBuiltin, T::Punctuation]),
            rule(r"&[0-9]").token(T::Name),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r"(?<=(\s|.))(AND|OR|EQ|NE|GT|LT|GE|LE|CO|CN|CA|NA|CS|NOT|NS|CP|NP|BYTE-CO|BYTE-CN|BYTE-CA|BYTE-NA|BYTE-CS|BYTE-NS|IS\s+(NOT\s+)?(INITIAL|ASSIGNED|REQUESTED|BOUND))\b").token(T::OperatorWord),
            include("variable-names"),
            rule(r"[?*<>=\-+&]").token(T::Operator),
            rule(r"'(''|[^'])*'").token(T::LiteralStringSingle),
            rule(r"`([^`])*`").token(T::LiteralStringSingle),
            rule(r"([|}])([^{}|]*?)([|{])").groups(&[T::Punctuation, T::LiteralStringSingle, T::Punctuation]),
            rule(r"[/;:()\[\],.]").token(T::Punctuation),
            rule(r"(!)(\w+)").groups(&[T::Operator, T::Name]),
        ]),
    ],
};

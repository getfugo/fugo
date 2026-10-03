//! Chroma's `postscript.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "postscript",
    config: ConfigDef {
        name: "PostScript",
        aliases: &["postscript", "postscr"],
        filenames: &["*.ps", "*.eps"],
        mime_types: &["application/postscript"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"^%!.+\n").token(T::CommentPreproc),
            rule(r"%%.*\n").token(T::CommentSpecial),
            rule(r"(^%.*\n){2,}").token(T::CommentMultiline),
            rule(r"%.*\n").token(T::CommentSingle),
            rule(r"\(").token(T::LiteralString).push(&["stringliteral"]),
            rule(r"[{}<>\[\]]").token(T::Punctuation),
            rule(r"<[0-9A-Fa-f]+>(?=[()<>\[\]{}/%\s])").token(T::LiteralNumberHex),
            rule(r"[0-9]+\#(\-|\+)?([0-9]+\.?|[0-9]*\.[0-9]+|[0-9]+\.[0-9]*)((e|E)[0-9]+)?(?=[()<>\[\]{}/%\s])").token(T::LiteralNumberOct),
            rule(r"(\-|\+)?([0-9]+\.?|[0-9]*\.[0-9]+|[0-9]+\.[0-9]*)((e|E)[0-9]+)?(?=[()<>\[\]{}/%\s])").token(T::LiteralNumberFloat),
            rule(r"(\-|\+)?[0-9]+(?=[()<>\[\]{}/%\s])").token(T::LiteralNumberInteger),
            rule(r"\/[^()<>\[\]{}/%\s]+(?=[()<>\[\]{}/%\s])").token(T::NameVariable),
            rule(r"[^()<>\[\]{}/%\s]+(?=[()<>\[\]{}/%\s])").token(T::NameFunction),
            rule(r"(false|true)(?=[()<>\[\]{}/%\s])").token(T::KeywordConstant),
            rule(r"(eq|ne|g[et]|l[et]|and|or|not|if(?:else)?|for(?:all)?)(?=[()<>\[\]{}/%\s])").token(T::KeywordReserved),
            rule(r"(dictstackoverflow|undefinedfilename|currentlinewidth|undefinedresult|currentmatrix|defaultmatrix|invertmatrix|concatmatrix|currentpoint|setlinewidth|syntaxerror|idtransform|identmatrix|setrgbcolor|stringwidth|setlinejoin|getinterval|itransform|strokepath|pathforall|rangecheck|setlinecap|dtransform|transform|translate|setmatrix|typecheck|undefined|scalefont|closepath|findfont|showpage|rcurveto|grestore|truncate|pathbbox|charpath|rlineto|rmoveto|ceiling|newpath|setdash|setfont|restore|curveto|setgray|stroke|pstack|matrix|length|lineto|repeat|rotate|moveto|shfill|concat|gsave|aload|scale|array|round|stack|index|begin|print|floor|exch|quit|clip|copy|bind|loop|idiv|fill|show|roll|exit|load|dict|save|arcn|sqrt|exec|rand|atan|end|div|abs|run|def|cvs|exp|cvi|sin|cos|get|dup|mod|put|sub|pop|add|neg|mul|arc|log|ln|gt)(?=[()<>\[\]{}/%\s])").token(T::NameBuiltin),
            rule(r"\s+").token(T::Text),
        ]),
        ("stringliteral", &[
            rule(r"[^()\\]+").token(T::LiteralString),
            rule(r"\\").token(T::LiteralStringEscape).push(&["escape"]),
            rule(r"\(").token(T::LiteralString).push(&[]),
            rule(r"\)").token(T::LiteralString).pop(1),
        ]),
        ("escape", &[
            rule(r"[0-8]{3}|n|r|t|b|f|\\|\(|\)").token(T::LiteralStringEscape).pop(1),
            rule("").pop(1),
        ]),
    ],
};

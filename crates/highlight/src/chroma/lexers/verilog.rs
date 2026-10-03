//! Chroma's `verilog.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "verilog",
    config: ConfigDef {
        name: "verilog",
        aliases: &["verilog", "v"],
        filenames: &["*.v"],
        mime_types: &["text/x-verilog"],
        ensure_nl: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"^\s*`define").token(T::CommentPreproc).push(&["macro"]),
            rule(r"\n").token(T::Text),
            rule(r"\s+").token(T::Text),
            rule(r"\\\n").token(T::Text),
            rule(r"/(\\\n)?/(\n|(.|\n)*?[^\\]\n)").token(T::CommentSingle),
            rule(r"/(\\\n)?[*](.|\n)*?[*](\\\n)?/").token(T::CommentMultiline),
            rule(r"[{}#@]").token(T::Punctuation),
            rule(r#"L?""#).token(T::LiteralString).push(&["string"]),
            rule(r"L?'(\\.|\\[0-7]{1,3}|\\x[a-fA-F0-9]{1,2}|[^\\\'\n])'").token(T::LiteralStringChar),
            rule(r"(\d+\.\d*|\.\d+|\d+)[eE][+-]?\d+[lL]?").token(T::LiteralNumberFloat),
            rule(r"(\d+\.\d*|\.\d+|\d+[fF])[fF]?").token(T::LiteralNumberFloat),
            rule(r"([0-9]+)|(\'h)[0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"([0-9]+)|(\'b)[01]+").token(T::LiteralNumberBin),
            rule(r"([0-9]+)|(\'d)[0-9]+").token(T::LiteralNumberInteger),
            rule(r"([0-9]+)|(\'o)[0-7]+").token(T::LiteralNumberOct),
            rule(r"\'[01xz]").token(T::LiteralNumber),
            rule(r"\d+[Ll]?").token(T::LiteralNumberInteger),
            rule(r"\*/").token(T::Error),
            rule(r"[~!%^&*+=|?:<>/-]").token(T::Operator),
            rule(r"[()\[\],.;\']").token(T::Punctuation),
            rule(r"`[a-zA-Z_]\w*").token(T::NameConstant),
            rule(r"^(\s*)(package)(\s+)").groups(&[T::Text, T::KeywordNamespace, T::Text]),
            rule(r"^(\s*)(import)(\s+)").groups(&[T::Text, T::KeywordNamespace, T::Text]).push(&["import"]),
            rule(r"(endprimitive|always_latch|macromodule|always_comb|endgenerate|endfunction|endpackage|endspecify|localparam|parameter|primitive|always_ff|automatic|specparam|endmodule|rtranif1|scalared|continue|deassign|endtable|defparam|function|strength|generate|pulldown|vectored|rtranif0|unsigned|specify|endcase|negedge|strong0|disable|default|endtask|posedge|strong1|typedef|tranif1|integer|forever|release|initial|tranif0|highz0|genvar|highz1|pullup|notif0|bufif1|bufif0|repeat|medium|return|struct|assign|signed|module|packed|string|output|notif1|always|final|casex|while|table|const|large|break|begin|input|pull0|pull1|inout|weak1|rcmos|weak0|casez|force|small|rnmos|rpmos|rtran|event|type|void|enum|wait|fork|join|else|edge|pmos|nand|cmos|nmos|task|xnor|case|tran|buf|ref|end|var|and|xor|for|nor|not|do|if|or)\b").token(T::Keyword),
            rule(r"`(autoexpand_vectornets|nounconnected_drive|noexpand_vectornets|noremove_gatenames|unconnected_drive|noremove_netnames|expand_vectornets|remove_gatenames|default_nettype|remove_netnames|endcelldefine|noaccelerate|endprotected|accelerate|celldefine|endprotect|protected|timescale|resetall|protect|include|ifndef|ifdef|endif|elsif|undef|else)\b").token(T::CommentPreproc),
            rule(r"\$(shortrealtobits|bitstoshortreal|printtimescale|showvariables|countdrivers|reset_value|reset_count|getpattern|showscopes|realtobits|bitstoreal|monitoroff|timeformat|sreadmemh|monitoron|sreadmemb|fmonitor|showvars|fdisplay|realtime|readmemb|readmemh|monitor|history|fstrobe|display|restart|incsave|strobe|fwrite|finish|random|fclose|stime|nokey|fopen|floor|nolog|scale|scope|input|reset|write|rtoi|bits|list|stop|itor|time|save|key|log)\b").token(T::NameBuiltin),
            rule(r"(woshortreal|shortint|realtime|longint|integer|supply0|supply1|triand|trireg|uwire|logic|trior|byte|wand|tri0|tri1|time|real|wire|reg|bit|int|tri)\b").token(T::KeywordType),
            rule(r"[a-zA-Z_]\w*:(?!:)").token(T::NameLabel),
            rule(r"\$?[a-zA-Z_]\w*").token(T::Name),
        ]),
        ("string", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"\\([\\abfnrtv"\']|x[a-fA-F0-9]{2,4}|[0-7]{1,3})"#).token(T::LiteralStringEscape),
            rule(r#"[^\\"\n]+"#).token(T::LiteralString),
            rule(r"\\\n").token(T::LiteralString),
            rule(r"\\").token(T::LiteralString),
        ]),
        ("macro", &[
            rule(r"[^/\n]+").token(T::CommentPreproc),
            rule(r"/[*](.|\n)*?[*]/").token(T::CommentMultiline),
            rule(r"//.*?\n").token(T::CommentSingle).pop(1),
            rule(r"/").token(T::CommentPreproc),
            rule(r"(?<=\\)\n").token(T::CommentPreproc),
            rule(r"\n").token(T::CommentPreproc).pop(1),
        ]),
        ("import", &[
            rule(r"[\w:]+\*?").token(T::NameNamespace).pop(1),
        ]),
    ],
};

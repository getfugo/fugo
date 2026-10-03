//! Chroma's `systemverilog.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "systemverilog",
    config: ConfigDef {
        name: "systemverilog",
        aliases: &["systemverilog", "sv"],
        filenames: &["*.sv", "*.svh"],
        mime_types: &["text/x-systemverilog"],
        ensure_nl: true,
        ..ConfigDef::EMPTY
    },
    states: &[
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
        ("root", &[
            rule(r"^\s*`define").token(T::CommentPreproc).push(&["macro"]),
            rule(r"^(\s*)(package)(\s+)").groups(&[T::Text, T::KeywordNamespace, T::Text]),
            rule(r#"^(\s*)(import)(\s+)("DPI(?:-C)?")(\s+)"#).groups(&[T::Text, T::KeywordNamespace, T::Text, T::LiteralString, T::Text]),
            rule(r"^(\s*)(import)(\s+)").groups(&[T::Text, T::KeywordNamespace, T::Text]).push(&["import"]),
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
            rule(r"(pulsestyle_ondetect|pulsestyle_onevent|noshowcancelled|sync_accept_on|sync_reject_on|showcancelled|timeprecision|endprimitive|randsequence|s_until_with|s_eventually|always_latch|endinterface|illegal_bins|macromodule|always_comb|endfunction|endproperty|first_match|endsequence|endgenerate|ignore_bins|endclocking|until_with|localparam|coverpoint|eventually|throughout|s_nexttime|endprogram|endspecify|endchecker|wait_order|constraint|covergroup|endpackage|endconfig|interface|accept_on|shortreal|parameter|primitive|intersect|protected|join_none|automatic|reject_on|always_ff|specparam|endmodule|shortint|join_any|endclass|sequence|defparam|scalared|deassign|endgroup|timeunit|instance|continue|restrict|clocking|nexttime|s_always|rtranif1|endtable|rtranif0|unsigned|priority|vectored|property|pulldown|wildcard|generate|function|realtime|forkjoin|randcase|context|forever|release|virtual|strong0|program|untyped|posedge|package|foreach|extends|specify|unique0|typedef|chandle|implies|checker|negedge|tranif1|initial|modport|strong1|matches|tranif0|endtask|integer|supply0|endcase|supply1|longint|disable|s_until|default|liblist|library|include|bufif0|design|tagged|struct|inside|medium|signed|config|highz1|incdir|import|expect|triand|trireg|export|unique|notif0|notif1|return|ifnone|output|highz0|packed|bufif1|repeat|global|genvar|binsof|extern|string|before|static|assume|assign|pullup|assert|always|within|strong|module|final|union|rcmos|casex|casez|trior|alias|pull1|pull0|break|uwire|randc|rnmos|rpmos|rtran|class|const|cover|weak1|until|logic|local|weak0|large|table|force|input|inout|small|solve|begin|super|event|while|cross|void|fork|enum|wait|cmos|bind|else|edge|join|nand|task|this|dist|time|cell|nmos|tran|wand|wire|bins|with|tri1|pmos|xnor|pure|type|real|rand|case|byte|weak|tri0|null|int|use|ref|var|tri|end|for|wor|iff|xor|bit|let|new|nor|and|not|reg|buf|or|if|do)\b").token(T::Keyword),
            rule(r"(`nounconnected_drive|`unconnected_drive|`default_nettype|`begin_keywords|`endcelldefine|`end_keywords|`undefineall|`celldefine|`timescale|`__LINE__|`resetall|`__FILE__|`include|`ifndef|`pragma|`define|`undef|`endif|`elsif|`ifdef|`else|`line)\b").token(T::CommentPreproc),
            rule(r"(\$dumpportsflush|\$dumpportslimit|\$value\$plusargs|\$dumpportsoff|\$dumpportsall|\$dumpportson|\$monitoroff|\$writememb|\$fdisplayo|\$fdisplayh|\$dumpports|\$dumplimit|\$dumpflush|\$fmonitorb|\$fmonitoro|\$monitoron|\$fdisplayb|\$writememh|\$fmonitorh|\$readmemb|\$fdisplay|\$monitorh|\$dumpfile|\$sformatf|\$monitorb|\$monitoro|\$displayb|\$plusargs|\$fmonitor|\$displayo|\$fstrobeo|\$displayh|\$fstrobeh|\$fstrobeb|\$readmemh|\$dumpvars|\$fstrobe|\$sformat|\$strobeb|\$swriteh|\$strobeh|\$strobeo|\$swriteb|\$fwriteh|\$fwriteo|\$monitor|\$dumpall|\$dumpoff|\$fwriteb|\$display|\$swriteo|\$fflush|\$random|\$dumpon|\$fscanf|\$rewind|\$writeh|\$writeo|\$sscanf|\$strobe|\$writeb|\$finish|\$ungetc|\$fclose|\$ferror|\$swrite|\$fwrite|\$fgetc|\$fseek|\$fgets|\$write|\$fopen|\$fread|\$ftell|\$test|\$feof)\b").token(T::NameBuiltin),
            rule(r"(class)(\s+)").groups(&[T::Keyword, T::Text]).push(&["classname"]),
            rule(r"(woshortreal|shortint|realtime|longint|integer|supply0|supply1|triand|trireg|uwire|logic|trior|byte|wand|tri0|tri1|time|real|wire|reg|bit|int|tri)\b").token(T::KeywordType),
            rule(r"[a-zA-Z_]\w*:(?!:)").token(T::NameLabel),
            rule(r"\$?[a-zA-Z_]\w*").token(T::Name),
        ]),
        ("classname", &[
            rule(r"[a-zA-Z_]\w*").token(T::NameClass).pop(1),
        ]),
        ("string", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"\\([\\abfnrtv"\']|x[a-fA-F0-9]{2,4}|[0-7]{1,3})"#).token(T::LiteralStringEscape),
            rule(r#"[^\\"\n]+"#).token(T::LiteralString),
            rule(r"\\\n").token(T::LiteralString),
            rule(r"\\").token(T::LiteralString),
        ]),
    ],
};

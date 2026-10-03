//! Chroma's `sas.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "sas",
    config: ConfigDef {
        name: "SAS",
        aliases: &["sas"],
        filenames: &["*.SAS", "*.sas"],
        mime_types: &["text/x-sas", "text/sas", "application/x-sas"],
        case_insensitive: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("validvar", &[
            rule(r"[a-z_]\w{0,31}\.?").token(T::NameVariable).pop(1),
        ]),
        ("cards-datalines", &[
            rule(r"^\s*(datalines|cards)\s*;\s*$").token(T::Keyword).push(&["data"]),
        ]),
        ("proc-data", &[
            rule(r"(^|;)\s*(proc \w+|data|run|quit)[\s;]").token(T::KeywordReserved),
        ]),
        ("string_dquote", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"\\\\|\\"|\\\n"#).token(T::LiteralStringEscape),
            rule(r"&").token(T::NameVariable).push(&["validvar"]),
            rule(r#"[^$&"\\]+"#).token(T::LiteralString),
            rule(r#"[$"\\]"#).token(T::LiteralString),
        ]),
        ("general", &[
            include("keywords"),
            include("vars-strings"),
            include("special"),
            include("numbers"),
        ]),
        ("vars-strings", &[
            rule(r"&[a-z_]\w{0,31}\.?").token(T::NameVariable),
            rule(r"%[a-z_]\w{0,31}").token(T::NameFunction),
            rule(r"\'").token(T::LiteralString).push(&["string_squote"]),
            rule(r#"""#).token(T::LiteralString).push(&["string_dquote"]),
        ]),
        ("root", &[
            include("comments"),
            include("proc-data"),
            include("cards-datalines"),
            include("logs"),
            include("general"),
            rule(r".").token(T::Text),
            rule(r"\\\n").token(T::Text),
            rule(r"\n").token(T::Text),
        ]),
        ("data", &[
            rule(r"(.|\n)*^\s*;\s*$").token(T::Other).pop(1),
        ]),
        ("logs", &[
            rule(r"\n?^\s*%?put ").token(T::Keyword).push(&["log-messages"]),
        ]),
        ("keywords", &[
            rule(r"\b(datalines4|datalines|delimiter|startsas|redirect|lostcard|continue|informat|filename|footnote|catname|options|libname|systask|display|waitsas|missing|replace|delete|window|endsas|update|format|attrib|length|infile|select|return|retain|rename|remove|output|cards4|modify|leave|title|merge|delim|input|cards|abort|where|label|array|error|call|page|stop|keep|file|drop|link|skip|list|goto|put|out|set|by|dm|in|x)\b").token(T::Keyword),
            rule(r"\b(references|distinct|describe|validate|restrict|cascade|msgtype|message|primary|foreign|delete|update|create|unique|having|modify|insert|select|group|check|table|alter|order|reset|index|where|into|from|view|null|like|drop|add|not|key|and|set|on|in|or|as)\b").token(T::Keyword),
            rule(r"\b(while|until|then|else|end|if|do)\b").token(T::Keyword),
            rule(r"%(sysevalf|nrbquote|qsysfunc|qlowcase|compstor|nrquote|display|qupcase|datatyp|qcmpres|unquote|syscall|sysfunc|sysrput|sysprod|syslput|sysexec|lowcase|qsubstr|sysget|length|keydef|global|superq|substr|verify|bquote|cmpres|upcase|window|label|qleft|while|qtrim|quote|nrstr|until|sysrc|input|macro|local|qscan|index|else|scan|mend|eval|trim|then|goto|left|put|let|end|str|do|to|if)\b").token(T::NameBuiltin),
            rule(r"\b(vinformatnx|vinformatwx|vinformatdx|vinformatw|vinformatd|vinformatx|vinformatn|vinformat|translate|vinarrayx|vformatwx|vformatnx|vformatdx|getoption|fileexist|fetchobs|vlengthx|filename|fipstate|kurtosis|vinarray|vformatx|pathname|foptname|compound|compress|vformatw|hosthelp|vformatn|zipnamel|vformatd|probbeta|daccdbsl|zipstate|trigamma|probbnml|probhypr|probnegb|probnorm|datepart|datetime|varlabel|varinfmt|dropnote|skewness|doptname|timepart|fipnamel|dequote|tranwrd|sysprod|digamma|stnamel|soundex|depdbsl|reverse|daccsyd|doptnum|resolve|uniform|datejul|varname|varrayx|probgam|probchi|fappend|dacctab|vformat|poisson|collate|brshift|ordinal|fdelete|blshift|betainv|fileref|lowcase|libname|fipname|vlabelx|vlength|weekday|juldate|jbessel|ibessel|zipfips|foptnum|zipname|getvarn|getvarc|frewind|vartype|depsyd|stderr|stfips|fwrite|gaminv|second|substr|vtypex|symget|hbound|vnamex|fpoint|saving|fnonct|rewind|indexc|indexw|repeat|inputc|inputn|ranuni|stname|rantbl|ranpoi|rannor|sysget|rangam|ranexp|vlabel|lbound|cexist|length|lgamma|rancau|libref|cnonct|ranbin|compbl|logpdf|logpmf|logsdf|sysmsg|curobs|daccdb|verify|daccsl|minute|system|tnonct|dsname|varray|varnum|probit|spedis|normal|varlen|dclose|varfmt|fexist|deptab|upcase|rantri|fclose|nmiss|point|trimn|depsl|trunc|peekc|depdb|probf|exist|fetch|netpv|today|mopen|probt|month|dairy|sysrc|finfo|quote|log10|close|floor|dinfo|range|fnote|attrn|intrr|intnx|intck|attrc|input|dread|dopen|index|right|round|vname|vtype|fread|gamma|arsin|arcos|fopen|frlen|fget|sinh|sqrt|addr|airy|sign|fsep|year|fuzz|dnum|scan|rank|fput|fpos|putn|putc|hour|tanh|atan|dhms|tinv|band|bnot|erfc|fcol|poke|trim|byte|ceil|peek|left|cinv|finv|open|log2|mean|note|date|cosh|mort|time|bxor|bor|mdy|std|max|css|sin|cos|npv|log|var|uss|pdf|pmf|cdf|abs|hms|day|erf|put|lag|irr|int|sum|tan|min|dif|qtr|sdf|dim|yyq|exp|mod|cv|n)\(").token(T::NameBuiltin),
        ]),
        ("numbers", &[
            rule(r"\b[+-]?([0-9]+(\.[0-9]+)?|\.[0-9]+|\.)(E[+-]?[0-9]+)?i?\b").token(T::LiteralNumber),
        ]),
        ("special", &[
            rule(r"(null|missing|_all_|_automatic_|_character_|_n_|_infile_|_name_|_null_|_numeric_|_user_|_webout_)").token(T::KeywordConstant),
        ]),
        ("log-messages", &[
            rule(r"NOTE(:|-).*").token(T::Generic).pop(1),
            rule(r"WARNING(:|-).*").token(T::GenericEmph).pop(1),
            rule(r"ERROR(:|-).*").token(T::GenericError).pop(1),
            include("general"),
        ]),
        ("comments", &[
            rule(r"^\s*\*.*?;").token(T::Comment),
            rule(r"/\*.*?\*/").token(T::Comment),
            rule(r"^\s*\*(.|\n)*?;").token(T::CommentMultiline),
            rule(r"/[*](.|\n)*?[*]/").token(T::CommentMultiline),
        ]),
        ("string_squote", &[
            rule(r"'").token(T::LiteralString).pop(1),
            rule(r#"\\\\|\\"|\\\n"#).token(T::LiteralStringEscape),
            rule(r"[^$\'\\]+").token(T::LiteralString),
            rule(r"[$\'\\]").token(T::LiteralString),
        ]),
    ],
};

//! Chroma's `gnuplot.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "gnuplot",
    config: ConfigDef {
        name: "Gnuplot",
        aliases: &["gnuplot"],
        filenames: &["*.plot", "*.plt"],
        mime_types: &["text/x-gnuplot"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("whitespace", &[
            rule(r"#").token(T::Comment).push(&["comment"]),
            rule(r"[ \t\v\f]+").token(T::Text),
        ]),
        ("save", &[
            rule(r"functions\b|function\b|functio\b|functi\b|funct\b|func\b|fun\b|fu\b|f\b|set\b|se\b|s\b|terminal\b|termina\b|termin\b|termi\b|term\b|ter\b|te\b|t\b|variables\b|variable\b|variabl\b|variab\b|varia\b|vari\b|var\b|va\b|v\b").token(T::NameBuiltin),
            include("genericargs"),
        ]),
        ("pause", &[
            rule(r"(mouse|any|button1|button2|button3)\b").token(T::NameBuiltin),
            rule(r"keypress\b|keypres\b|keypre\b|keypr\b|keyp\b|key\b").token(T::NameBuiltin),
            include("genericargs"),
        ]),
        ("plot", &[
            rule(r"axes\b|axe\b|ax\b|axis\b|axi\b|binary\b|binar\b|bina\b|bin\b|every\b|ever\b|eve\b|ev\b|index\b|inde\b|ind\b|in\b|i\b|matrix\b|matri\b|matr\b|mat\b|smooth\b|smoot\b|smoo\b|smo\b|sm\b|s\b|thru\b|title\b|titl\b|tit\b|ti\b|t\b|notitle\b|notitl\b|notit\b|noti\b|not\b|using\b|usin\b|usi\b|us\b|u\b|with\b|wit\b|wi\b|w\b").token(T::NameBuiltin),
            include("genericargs"),
        ]),
        ("if", &[
            rule(r"\)").token(T::Punctuation).pop(1),
            include("genericargs"),
        ]),
        ("genericargs", &[
            include("noargs"),
            rule(r#"""#).token(T::LiteralString).push(&["dqstring"]),
            rule(r"'").token(T::LiteralString).push(&["sqstring"]),
            rule(r"(\d+\.\d*|\.\d+|\d+)[eE][+-]?\d+").token(T::LiteralNumberFloat),
            rule(r"(\d+\.\d*|\.\d+)").token(T::LiteralNumberFloat),
            rule(r"-?\d+").token(T::LiteralNumberInteger),
            rule(r"[,.~!%^&*+=|?:<>/-]").token(T::Operator),
            rule(r"[{}()\[\]]").token(T::Punctuation),
            rule(r"(eq|ne)\b").token(T::OperatorWord),
            rule(r"([a-zA-Z_]\w*)(\s*)(\()").groups(&[T::NameFunction, T::Text, T::Punctuation]),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
            rule(r"@[a-zA-Z_]\w*").token(T::NameConstant),
            rule(r"\\\n").token(T::Text),
        ]),
        ("comment", &[
            rule(r"[^\\\n]").token(T::Comment),
            rule(r"\\\n").token(T::Comment),
            rule(r"\\").token(T::Comment),
            rule("").pop(1),
        ]),
        ("root", &[
            include("whitespace"),
            rule(r"bind\b|bin\b|bi\b").token(T::Keyword).push(&["bind"]),
            rule(r"exit\b|exi\b|ex\b|quit\b|qui\b|qu\b|q\b").token(T::Keyword).push(&["quit"]),
            rule(r"fit\b|fi\b|f\b").token(T::Keyword).push(&["fit"]),
            rule(r"(if)(\s*)(\()").groups(&[T::Keyword, T::Text, T::Punctuation]).push(&["if"]),
            rule(r"else\b").token(T::Keyword),
            rule(r"pause\b|paus\b|pau\b|pa\b").token(T::Keyword).push(&["pause"]),
            rule(r"plot\b|plo\b|pl\b|p\b|replot\b|replo\b|repl\b|rep\b|splot\b|splo\b|spl\b|sp\b").token(T::Keyword).push(&["plot"]),
            rule(r"save\b|sav\b|sa\b").token(T::Keyword).push(&["save"]),
            rule(r"set\b|se\b").token(T::Keyword).push(&["genericargs", "optionarg"]),
            rule(r"show\b|sho\b|sh\b|unset\b|unse\b|uns\b").token(T::Keyword).push(&["noargs", "optionarg"]),
            rule(r"lower\b|lowe\b|low\b|raise\b|rais\b|rai\b|ra\b|call\b|cal\b|ca\b|cd\b|clear\b|clea\b|cle\b|cl\b|help\b|hel\b|he\b|h\b|\?\b|history\b|histor\b|histo\b|hist\b|his\b|hi\b|load\b|loa\b|lo\b|l\b|print\b|prin\b|pri\b|pr\b|pwd\b|reread\b|rerea\b|rere\b|rer\b|re\b|reset\b|rese\b|res\b|screendump\b|screendum\b|screendu\b|screend\b|screen\b|scree\b|scre\b|scr\b|shell\b|shel\b|she\b|system\b|syste\b|syst\b|sys\b|sy\b|update\b|updat\b|upda\b|upd\b|up\b").token(T::Keyword).push(&["genericargs"]),
            rule(r"pwd\b|reread\b|rerea\b|rere\b|rer\b|re\b|reset\b|rese\b|res\b|screendump\b|screendum\b|screendu\b|screend\b|screen\b|scree\b|scre\b|scr\b|shell\b|shel\b|she\b|test\b").token(T::Keyword).push(&["noargs"]),
            rule(r"([a-zA-Z_]\w*)(\s*)(=)").groups(&[T::NameVariable, T::Text, T::Operator]).push(&["genericargs"]),
            rule(r"([a-zA-Z_]\w*)(\s*\(.*?\)\s*)(=)").groups(&[T::NameFunction, T::Text, T::Operator]).push(&["genericargs"]),
            rule(r"@[a-zA-Z_]\w*").token(T::NameConstant),
            rule(r";").token(T::Keyword),
        ]),
        ("dqstring", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"\\([\\abfnrtv"\']|x[a-fA-F0-9]{2,4}|[0-7]{1,3})"#).token(T::LiteralStringEscape),
            rule(r#"[^\\"\n]+"#).token(T::LiteralString),
            rule(r"\\\n").token(T::LiteralString),
            rule(r"\\").token(T::LiteralString),
            rule(r"\n").token(T::LiteralString).pop(1),
        ]),
        ("optionarg", &[
            include("whitespace"),
            rule(r"all\b|al\b|a\b|angles\b|angle\b|angl\b|ang\b|an\b|arrow\b|arro\b|arr\b|ar\b|autoscale\b|autoscal\b|autosca\b|autosc\b|autos\b|auto\b|aut\b|au\b|bars\b|bar\b|ba\b|b\b|border\b|borde\b|bord\b|bor\b|boxwidth\b|boxwidt\b|boxwid\b|boxwi\b|boxw\b|box\b|clabel\b|clabe\b|clab\b|cla\b|cl\b|clip\b|cli\b|cl\b|c\b|cntrparam\b|cntrpara\b|cntrpar\b|cntrpa\b|cntrp\b|cntr\b|cnt\b|cn\b|contour\b|contou\b|conto\b|cont\b|con\b|co\b|data\b|dat\b|da\b|datafile\b|datafil\b|datafi\b|dataf\b|data\b|dgrid3d\b|dgrid3\b|dgrid\b|dgri\b|dgr\b|dg\b|dummy\b|dumm\b|dum\b|du\b|encoding\b|encodin\b|encodi\b|encod\b|enco\b|enc\b|decimalsign\b|decimalsig\b|decimalsi\b|decimals\b|decimal\b|decima\b|decim\b|deci\b|dec\b|fit\b|fontpath\b|fontpat\b|fontpa\b|fontp\b|font\b|format\b|forma\b|form\b|for\b|fo\b|function\b|functio\b|functi\b|funct\b|func\b|fun\b|fu\b|functions\b|function\b|functio\b|functi\b|funct\b|func\b|fun\b|fu\b|grid\b|gri\b|gr\b|g\b|hidden3d\b|hidden3\b|hidden\b|hidde\b|hidd\b|hid\b|historysize\b|historysiz\b|historysi\b|historys\b|history\b|histor\b|histo\b|hist\b|his\b|isosamples\b|isosample\b|isosampl\b|isosamp\b|isosam\b|isosa\b|isos\b|iso\b|is\b|key\b|ke\b|k\b|keytitle\b|keytitl\b|keytit\b|keyti\b|keyt\b|label\b|labe\b|lab\b|la\b|linestyle\b|linestyl\b|linesty\b|linest\b|lines\b|line\b|lin\b|li\b|ls\b|loadpath\b|loadpat\b|loadpa\b|loadp\b|load\b|loa\b|locale\b|local\b|loca\b|loc\b|logscale\b|logscal\b|logsca\b|logsc\b|logs\b|log\b|macros\b|macro\b|macr\b|mac\b|mapping\b|mappin\b|mappi\b|mapp\b|map\b|mapping3d\b|mapping3\b|mapping\b|mappin\b|mappi\b|mapp\b|map\b|margin\b|margi\b|marg\b|mar\b|lmargin\b|lmargi\b|lmarg\b|lmar\b|rmargin\b|rmargi\b|rmarg\b|rmar\b|tmargin\b|tmargi\b|tmarg\b|tmar\b|bmargin\b|bmargi\b|bmarg\b|bmar\b|mouse\b|mous\b|mou\b|mo\b|multiplot\b|multiplo\b|multipl\b|multip\b|multi\b|mxtics\b|mxtic\b|mxti\b|mxt\b|nomxtics\b|nomxtic\b|nomxti\b|nomxt\b|mx2tics\b|mx2tic\b|mx2ti\b|mx2t\b|nomx2tics\b|nomx2tic\b|nomx2ti\b|nomx2t\b|mytics\b|mytic\b|myti\b|myt\b|nomytics\b|nomytic\b|nomyti\b|nomyt\b|my2tics\b|my2tic\b|my2ti\b|my2t\b|nomy2tics\b|nomy2tic\b|nomy2ti\b|nomy2t\b|mztics\b|mztic\b|mzti\b|mzt\b|nomztics\b|nomztic\b|nomzti\b|nomzt\b|mcbtics\b|mcbtic\b|mcbti\b|mcbt\b|nomcbtics\b|nomcbtic\b|nomcbti\b|nomcbt\b|offsets\b|offset\b|offse\b|offs\b|off\b|of\b|origin\b|origi\b|orig\b|ori\b|or\b|output\b|outpu\b|outp\b|out\b|ou\b|o\b|parametric\b|parametri\b|parametr\b|paramet\b|parame\b|param\b|para\b|par\b|pa\b|pm3d\b|pm3\b|pm\b|palette\b|palett\b|palet\b|pale\b|pal\b|colorbox\b|colorbo\b|colorb\b|plot\b|plo\b|pl\b|p\b|pointsize\b|pointsiz\b|pointsi\b|points\b|point\b|poin\b|poi\b|polar\b|pola\b|pol\b|print\b|prin\b|pri\b|pr\b|object\b|objec\b|obje\b|obj\b|samples\b|sample\b|sampl\b|samp\b|sam\b|sa\b|size\b|siz\b|si\b|style\b|styl\b|sty\b|st\b|surface\b|surfac\b|surfa\b|surf\b|sur\b|su\b|table\b|terminal\b|termina\b|termin\b|termi\b|term\b|ter\b|te\b|t\b|termoptions\b|termoption\b|termoptio\b|termopti\b|termopt\b|termop\b|termo\b|tics\b|tic\b|ti\b|ticscale\b|ticscal\b|ticsca\b|ticsc\b|ticslevel\b|ticsleve\b|ticslev\b|ticsle\b|ticsl\b|timefmt\b|timefm\b|timef\b|timestamp\b|timestam\b|timesta\b|timest\b|times\b|time\b|tim\b|title\b|titl\b|tit\b|variables\b|variable\b|variabl\b|variab\b|varia\b|vari\b|var\b|va\b|v\b|version\b|versio\b|versi\b|vers\b|ver\b|ve\b|view\b|vie\b|vi\b|xyplane\b|xyplan\b|xypla\b|xypl\b|xyp\b|xdata\b|xdat\b|xda\b|x2data\b|x2dat\b|x2da\b|ydata\b|ydat\b|yda\b|y2data\b|y2dat\b|y2da\b|zdata\b|zdat\b|zda\b|cbdata\b|cbdat\b|cbda\b|xlabel\b|xlabe\b|xlab\b|xla\b|xl\b|x2label\b|x2labe\b|x2lab\b|x2la\b|x2l\b|ylabel\b|ylabe\b|ylab\b|yla\b|yl\b|y2label\b|y2labe\b|y2lab\b|y2la\b|y2l\b|zlabel\b|zlabe\b|zlab\b|zla\b|zl\b|cblabel\b|cblabe\b|cblab\b|cbla\b|cbl\b|xtics\b|xtic\b|xti\b|noxtics\b|noxtic\b|noxti\b|x2tics\b|x2tic\b|x2ti\b|nox2tics\b|nox2tic\b|nox2ti\b|ytics\b|ytic\b|yti\b|noytics\b|noytic\b|noyti\b|y2tics\b|y2tic\b|y2ti\b|noy2tics\b|noy2tic\b|noy2ti\b|ztics\b|ztic\b|zti\b|noztics\b|noztic\b|nozti\b|cbtics\b|cbtic\b|cbti\b|nocbtics\b|nocbtic\b|nocbti\b|xdtics\b|xdtic\b|xdti\b|noxdtics\b|noxdtic\b|noxdti\b|x2dtics\b|x2dtic\b|x2dti\b|nox2dtics\b|nox2dtic\b|nox2dti\b|ydtics\b|ydtic\b|ydti\b|noydtics\b|noydtic\b|noydti\b|y2dtics\b|y2dtic\b|y2dti\b|noy2dtics\b|noy2dtic\b|noy2dti\b|zdtics\b|zdtic\b|zdti\b|nozdtics\b|nozdtic\b|nozdti\b|cbdtics\b|cbdtic\b|cbdti\b|nocbdtics\b|nocbdtic\b|nocbdti\b|xmtics\b|xmtic\b|xmti\b|noxmtics\b|noxmtic\b|noxmti\b|x2mtics\b|x2mtic\b|x2mti\b|nox2mtics\b|nox2mtic\b|nox2mti\b|ymtics\b|ymtic\b|ymti\b|noymtics\b|noymtic\b|noymti\b|y2mtics\b|y2mtic\b|y2mti\b|noy2mtics\b|noy2mtic\b|noy2mti\b|zmtics\b|zmtic\b|zmti\b|nozmtics\b|nozmtic\b|nozmti\b|cbmtics\b|cbmtic\b|cbmti\b|nocbmtics\b|nocbmtic\b|nocbmti\b|xrange\b|xrang\b|xran\b|xra\b|xr\b|x2range\b|x2rang\b|x2ran\b|x2ra\b|x2r\b|yrange\b|yrang\b|yran\b|yra\b|yr\b|y2range\b|y2rang\b|y2ran\b|y2ra\b|y2r\b|zrange\b|zrang\b|zran\b|zra\b|zr\b|cbrange\b|cbrang\b|cbran\b|cbra\b|cbr\b|rrange\b|rrang\b|rran\b|rra\b|rr\b|trange\b|trang\b|tran\b|tra\b|tr\b|urange\b|urang\b|uran\b|ura\b|ur\b|vrange\b|vrang\b|vran\b|vra\b|vr\b|xzeroaxis\b|xzeroaxi\b|xzeroax\b|xzeroa\b|x2zeroaxis\b|x2zeroaxi\b|x2zeroax\b|x2zeroa\b|yzeroaxis\b|yzeroaxi\b|yzeroax\b|yzeroa\b|y2zeroaxis\b|y2zeroaxi\b|y2zeroax\b|y2zeroa\b|zzeroaxis\b|zzeroaxi\b|zzeroax\b|zzeroa\b|zeroaxis\b|zeroaxi\b|zeroax\b|zeroa\b|zero\b|zer\b|ze\b|z\b").token(T::NameBuiltin).pop(1),
        ]),
        ("bind", &[
            rule(r"!").token(T::Keyword).pop(1),
            rule(r"allwindows\b|allwindow\b|allwindo\b|allwind\b|allwin\b|allwi\b|allw\b|all\b").token(T::NameBuiltin),
            include("genericargs"),
        ]),
        ("fit", &[
            rule(r"via\b").token(T::NameBuiltin),
            include("plot"),
        ]),
        ("noargs", &[
            include("whitespace"),
            rule(r";").token(T::Punctuation).pop(1),
            rule(r"\n").token(T::Text).pop(1),
        ]),
        ("sqstring", &[
            rule(r"''").token(T::LiteralString),
            rule(r"'").token(T::LiteralString).pop(1),
            rule(r"[^\\'\n]+").token(T::LiteralString),
            rule(r"\\\n").token(T::LiteralString),
            rule(r"\\").token(T::LiteralString),
            rule(r"\n").token(T::LiteralString).pop(1),
        ]),
        ("quit", &[
            rule(r"gnuplot\b").token(T::Keyword),
            include("noargs"),
        ]),
    ],
};

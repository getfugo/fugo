//! Chroma's `terraform.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "terraform",
    config: ConfigDef {
        name: "Terraform",
        aliases: &["terraform", "tf", "hcl"],
        filenames: &["*.tf", "*.hcl"],
        mime_types: &["application/x-tf", "application/x-terraform"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("string", &[
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            rule(r#"\\\\""#).token(T::LiteralStringDouble),
            rule(r#"[^"\\\\$]+"#).token(T::LiteralStringDouble),
            rule(r#"[^\\\\"$]+"#).token(T::LiteralStringDouble),
            rule(r"\$\{").token(T::LiteralStringInterpol).push(&["interp-inside"]),
        ]),
        ("interp-inside", &[
            rule(r"\}").token(T::LiteralStringInterpol).pop(1),
            include("root"),
        ]),
        ("root", &[
            rule(r"[\[\](),.{}]").token(T::Punctuation),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["string"]),
            rule(r"-?[0-9]+").token(T::LiteralNumber),
            rule(r"=>").token(T::Punctuation),
            rule(r"(false|true)\b").token(T::KeywordConstant),
            rule(r"/(?s)\*(((?!\*/).)*)\*/").token(T::CommentMultiline),
            rule(r"\s*(#|//).*\n").token(T::CommentSingle),
            rule(r"(?!\s*)(variable)(\s*)").groups(&[T::Name, T::Text]),
            rule(r"^(provisioner|variable|resource|provider|module|output|data)(?!\.)\b").token(T::KeywordReserved),
            rule(r"(for|in)\b").token(T::Keyword),
            rule(r"(module|count|data|each|var)\b").token(T::NameBuiltin),
            rule(r"(parseint|signum|floor|ceil|log|max|min|abs|pow)\b").token(T::NameBuiltin),
            rule(r"(trimsuffix|formatlist|trimprefix|trimspace|regexall|replace|indent|strrev|format|substr|chomp|split|title|regex|lower|upper|trim|join)\b").token(T::NameBuiltin),
            rule(r"[^.](setintersection|coalescelist|setsubtract|setproduct|matchkeys|chunklist|transpose|contains|distinct|coalesce|setunion|reverse|flatten|element|compact|lookup|length|concat|values|zipmap|range|merge|slice|index|list|sort|keys|map)\b").token(T::NameBuiltin),
            rule(r"[^.](base64decode|base64encode|base64gzip|jsondecode|jsonencode|yamldecode|yamlencode|csvdecode|urlencode)\b").token(T::NameBuiltin),
            rule(r"(templatefile|filebase64|fileexists|pathexpand|basename|abspath|fileset|dirname|file)\b").token(T::NameBuiltin),
            rule(r"(formatdate|timestamp|timeadd)\b").token(T::NameBuiltin),
            rule(r"(filebase64sha256|filebase64sha512|base64sha512|base64sha256|filesha256|rsadecrypt|filesha512|filesha1|filemd5|uuidv5|bcrypt|sha256|sha512|sha1|uuid|md5)\b").token(T::NameBuiltin),
            rule(r"(cidrnetmask|cidrsubnet|cidrhost)\b").token(T::NameBuiltin),
            rule(r"(tostring|tonumber|tobool|tolist|tomap|toset|can|try)\b").token(T::NameBuiltin),
            rule(r"(^|[^.\w])(name|x|default|type|description|value)(_[a-zA-Z]\w*)*").token(T::NameAttribute),
            rule(r"=(?!>)|\+|-|\*|\/|:|!|%|>|<(?!<)|>=|<=|==|!=|&&|\||\?").token(T::Operator),
            rule(r"\n|\s+|\\\n").token(T::Text),
            rule(r"[a-zA-Z]\w*").token(T::NameOther),
            rule(r"(?s)(<<-?)(\w+)(\n\s*(?:(?!\2).)*\s*\n\s*)(\2)").groups(&[T::Operator, T::Operator, T::LiteralString, T::Operator]),
        ]),
        ("declaration", &[
            rule(r#"(\s*)("(?:\\\\|\\"|[^"])*")(\s*)"#).groups(&[T::Text, T::NameAttribute, T::Text]),
            rule(r"\{").token(T::Punctuation).pop(1),
        ]),
    ],
};

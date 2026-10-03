//! Chroma's `erlang.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "erlang",
    config: ConfigDef {
        name: "Erlang",
        aliases: &["erlang"],
        filenames: &["*.erl", "*.hrl", "*.es", "*.escript"],
        mime_types: &["text/x-erlang"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::Text),
            rule(r"%.*\n").token(T::Comment),
            rule(r"(receive|after|begin|catch|query|case|cond|when|let|fun|end|try|of|if)\b").token(T::Keyword),
            rule(r"(localtime_to_universaltime|universaltime_to_localtime|list_to_existing_atom|check_process_code|bitstring_to_list|list_to_bitstring|function_exported|is_process_alive|iolist_to_binary|bump_reductions|garbage_collect|process_display|suspend_process|list_to_integer|disconnect_node|integer_to_list|trace_delivered|send_nosuspend|list_to_binary|system_profile|binary_to_term|binary_to_list|resume_process|append_element|term_to_binary|system_monitor|list_to_tuple|spawn_monitor|delete_module|trace_pattern|tuple_to_list|list_to_float|float_to_list|module_loaded|port_connect|is_bitstring|port_to_list|monitor_node|process_info|port_control|split_binary|cancel_timer|purge_module|group_leader|list_to_atom|atom_to_list|port_command|is_reference|process_flag|pid_to_list|system_info|start_timer|iolist_size|fun_to_list|load_module|is_function|ref_to_list|list_to_pid|system_flag|make_tuple|is_builtin|unregister|is_boolean|set_cookie|md5_update|spawn_link|setelement|trace_info|read_timer|statistics|send_after|port_close|is_integer|tuple_size|spawn_opt|open_port|is_record|is_binary|md5_final|port_call|port_info|is_number|byte_size|demonitor|register|is_float|bit_size|fun_info|get_keys|is_tuple|is_atom|element|is_list|is_port|monitor|display|whereis|is_pid|memory|unlink|phash2|length|spawn|nodes|trace|round|apply|erase|phash|trunc|float|size|link|node|exit|hash|send|get|md5|put|abs|hd|tl)\b").token(T::NameBuiltin),
            rule(r"(andalso|orelse|bxor|band|bnot|and|bsr|bsl|div|not|rem|bor|xor|or)\b").token(T::OperatorWord),
            rule(r"^-").token(T::Punctuation).push(&["directive"]),
            rule(r"(\+\+?|--?|\*|/|<|>|/=|=:=|=/=|=<|>=|==?|<-|!|\?)").token(T::Operator),
            rule(r#"""#).token(T::LiteralString).push(&["string"]),
            rule(r"<<").token(T::NameLabel),
            rule(r">>").token(T::NameLabel),
            rule(r"((?:[a-z]\w*|'[^\n']*[^\\]'))(:)").groups(&[T::NameNamespace, T::Punctuation]),
            rule(r"(?:^|(?<=:))((?:[a-z]\w*|'[^\n']*[^\\]'))(\s*)(\()").groups(&[T::NameFunction, T::Text, T::Punctuation]),
            rule(r"[+-]?(?:[2-9]|[12][0-9]|3[0-6])#[0-9a-zA-Z]+").token(T::LiteralNumberInteger),
            rule(r"[+-]?\d+").token(T::LiteralNumberInteger),
            rule(r"[+-]?\d+.\d+").token(T::LiteralNumberFloat),
            rule(r#"[]\[:_@\".{}()|;,]"#).token(T::Punctuation),
            rule(r"(?:[A-Z_]\w*)").token(T::NameVariable),
            rule(r"(?:[a-z]\w*|'[^\n']*[^\\]')").token(T::Name),
            rule(r"\?(?:(?:[A-Z_]\w*)|(?:[a-z]\w*|'[^\n']*[^\\]'))").token(T::NameConstant),
            rule(r#"\$(?:(?:\\(?:[bdefnrstv\'"\\]|[0-7][0-7]?[0-7]?|(?:x[0-9a-fA-F]{2}|x\{[0-9a-fA-F]+\})|\^[a-zA-Z]))|\\[ %]|[^\\])"#).token(T::LiteralStringChar),
            rule(r"#(?:[a-z]\w*|'[^\n']*[^\\]')(:?\.(?:[a-z]\w*|'[^\n']*[^\\]'))?").token(T::NameLabel),
            rule(r"\A#!.+\n").token(T::CommentHashbang),
            rule(r"#\{").token(T::Punctuation).push(&["map_key"]),
        ]),
        ("string", &[
            rule(r#"(?:\\(?:[bdefnrstv\'"\\]|[0-7][0-7]?[0-7]?|(?:x[0-9a-fA-F]{2}|x\{[0-9a-fA-F]+\})|\^[a-zA-Z]))"#).token(T::LiteralStringEscape),
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r"~[0-9.*]*[~#+BPWXb-ginpswx]").token(T::LiteralStringInterpol),
            rule(r#"[^"\\~]+"#).token(T::LiteralString),
            rule(r"~").token(T::LiteralString),
        ]),
        ("directive", &[
            rule(r"(define)(\s*)(\()((?:(?:[A-Z_]\w*)|(?:[a-z]\w*|'[^\n']*[^\\]')))").groups(&[T::NameEntity, T::Text, T::Punctuation, T::NameConstant]).pop(1),
            rule(r"(record)(\s*)(\()((?:(?:[A-Z_]\w*)|(?:[a-z]\w*|'[^\n']*[^\\]')))").groups(&[T::NameEntity, T::Text, T::Punctuation, T::NameLabel]).pop(1),
            rule(r"(?:[a-z]\w*|'[^\n']*[^\\]')").token(T::NameEntity).pop(1),
        ]),
        ("map_key", &[
            include("root"),
            rule(r"=>").token(T::Punctuation).push(&["map_val"]),
            rule(r":=").token(T::Punctuation).push(&["map_val"]),
            rule(r"\}").token(T::Punctuation).pop(1),
        ]),
        ("map_val", &[
            include("root"),
            rule(r",").token(T::Punctuation).pop(1),
            rule(r"(?=\})").token(T::Punctuation).pop(1),
        ]),
    ],
};

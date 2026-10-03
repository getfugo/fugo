//! Study aid for comparing with Chroma: `FUGO_HL_TOKENS=<in>:<out>` reads JSON lines
//! `{"lang": …, "code": …}` and writes, per line, the coalesced tokens as `[[type, text], …]`
//! (`null` when Chroma has no lexer for the language) — the format of the Go oracle's `tokens`
//! command (crate README).

use std::io::{BufRead, Write};

use ssg_config::markup::HighlightConfig;
use ssg_highlight::Highlight;

#[test]
fn write_tokens() {
    let Ok(arg) = std::env::var("FUGO_HL_TOKENS") else {
        return;
    };
    let (input, output) = arg.split_once(':').expect("<in>:<out>");
    let hl = Highlight::new(&HighlightConfig::default());
    let input = std::io::BufReader::new(std::fs::File::open(input).expect("input"));
    let mut out = std::io::BufWriter::new(std::fs::File::create(output).expect("output"));
    for line in input.lines() {
        let line = line.expect("line");
        let req: serde_json::Value = serde_json::from_str(&line).expect("json");
        let lang = req["lang"].as_str().unwrap_or_default();
        let code = req["code"].as_str().unwrap_or_default();
        let tokens = hl.tokens(code, lang).map(|ts| {
            ts.into_iter()
                .map(|(t, v)| serde_json::json!([t.name(), v]))
                .collect::<Vec<_>>()
        });
        serde_json::to_writer(&mut out, &tokens).expect("write");
        out.write_all(b"\n").expect("write");
    }
}

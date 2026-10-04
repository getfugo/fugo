use super::*;

fn delim(s: &str) -> Option<Vec<TableAlignment>> {
    parse_delimiter(s.as_bytes())
}

#[test]
fn delimiters() {
    use TableAlignment::{Center, Left, None as N, Right};
    assert_eq!(delim(":--|:--|:--"), Some(vec![Left, Left, Left]));
    assert_eq!(delim("| --- | :-: | --: |"), Some(vec![N, Center, Right]));
    assert_eq!(delim("|-"), Some(vec![N]));
    assert_eq!(delim("    | --- |"), None);
    assert_eq!(delim("| |"), None);
    assert_eq!(delim("|"), None);
    assert_eq!(delim("| : -- |"), None);
    assert_eq!(delim("| -- | x |"), None);
}

fn row(s: &str, n: usize, header: bool) -> Vec<String> {
    let a = vec![TableAlignment::None; n];
    parse_row(s.as_bytes(), 0..s.len(), &a, header)
        .cells
        .iter()
        .map(|c| match c {
            Cell::Text(r) => s[r.clone()].to_owned(),
            Cell::Pad => "<pad>".to_owned(),
        })
        .collect()
}

#[test]
fn rows() {
    assert_eq!(row("a|b", 3, true), ["a", "b", "<pad>"]);
    assert_eq!(row("| a | b | c |", 2, false), ["a", "b"]);
    assert_eq!(row("| a | b | c |", 2, true), ["a", "b", "c"]);
    assert_eq!(row(r"| a \| b | c", 2, false), [r"a \| b", "c"]);
    assert_eq!(row("a||", 2, false), ["a", "<pad>"]);
    assert_eq!(row("|", 2, false), ["<pad>", "<pad>"]);
    assert_eq!(row("plain", 2, false), ["plain", "<pad>"]);
    assert_eq!(row("| |", 1, false), [""]);
}

#[test]
fn code_pipes() {
    assert_eq!(unescape_code_pipes(r"a\\|b").as_deref(), Some(r"a\|b"));
    assert_eq!(unescape_code_pipes(r"a\\\\|b").as_deref(), Some(r"a\\\|b"));
    assert_eq!(unescape_code_pipes(r"a\|b"), None);
    assert_eq!(unescape_code_pipes("a|b"), None);
}

#[test]
fn transforms() {
    let t = "intro\n  Field | Value\n  | --- | --- |\n  | a | b |";
    let mut lines = Vec::new();
    let mut at = 0;
    for l in t.split('\n') {
        lines.push(at..at + l.len());
        at += l.len() + 1;
    }
    let f = transform(t.as_bytes(), &lines).expect("a table");
    assert_eq!(f.before.len(), 1);
    assert_eq!(f.body.len(), 1);
    // A header with more cells than the delimiter: no table, and no later one either.
    let t = "a|b|c\n-|-\nd|e\n-|-";
    let mut lines = Vec::new();
    let mut at = 0;
    for l in t.split('\n') {
        lines.push(at..at + l.len());
        at += l.len() + 1;
    }
    assert!(transform(t.as_bytes(), &lines).is_none());
}

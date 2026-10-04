//! `fnmatch.fnmatchcase` of CPython: the keys of the changes files are shell patterns
//! (`*` any text, `/` included; `?` one character; `[…]`, `[!…]` sets), translated the way
//! `fnmatch.translate` does, so a pattern means what it meant to the harness's first version.
//! Translated from CPython 3.14.7's `Lib/fnmatch.py` (PSF-2.0, THIRD_PARTY/cpython/LICENSE;
//! PROVENANCE.md).

use regex::Regex;

/// Whether `name` matches the shell pattern `pat`, case-sensitively.
#[must_use]
pub fn fnmatchcase(name: &str, pat: &str) -> bool {
    Regex::new(&translate(pat)).is_ok_and(|re| re.is_match(name))
}

/// The regular expression of a shell pattern (anchored at both ends).
#[must_use]
pub fn translate(pat: &str) -> String {
    let p: Vec<char> = pat.chars().collect();
    let n = p.len();
    let mut out = String::from(r"(?s)\A(?:");
    let mut i = 0;
    while i < n {
        let c = p[i];
        i += 1;
        match c {
            '*' => {
                out.push_str(".*");
                while i < n && p[i] == '*' {
                    i += 1;
                }
            }
            '?' => out.push('.'),
            '[' => {
                let mut j = i;
                if j < n && p[j] == '!' {
                    j += 1;
                }
                if j < n && p[j] == ']' {
                    j += 1;
                }
                while j < n && p[j] != ']' {
                    j += 1;
                }
                if j >= n {
                    out.push_str(r"\[");
                } else {
                    out.push_str(&set(&p, i, j));
                    i = j + 1;
                }
            }
            c => out.push_str(&regex::escape(&c.to_string())),
        }
    }
    out.push_str(r")\z");
    out
}

/// The set `pat[i..j]` (between `[` and `]`) as a character class: a `-` between two characters
/// is a range unless it is the set's first character (after a leading `!`) or follows a range;
/// a reversed range is dropped; a leading `!` negates.
fn set(p: &[char], start: usize, j: usize) -> String {
    let stuff = &p[start..j];
    // The pieces between the range hyphens; every other character is literal.
    let mut chunks: Vec<Vec<char>> = Vec::new();
    if stuff.contains(&'-') {
        let mut i = start;
        let mut k = if p[i] == '!' { i + 2 } else { i + 1 };
        while let Some(h) = (k..j).find(|&x| p[x] == '-') {
            chunks.push(p[i..h].to_vec());
            i = h + 1;
            k = h + 3;
        }
        let chunk = p[i..j].to_vec();
        if chunk.is_empty() {
            chunks
                .last_mut()
                .expect("a chunk before the trailing hyphen")
                .push('-');
        } else {
            chunks.push(chunk);
        }
        for k in (1..chunks.len()).rev() {
            if chunks[k - 1].last() > chunks[k].first() {
                let mut merged = chunks[k - 1][..chunks[k - 1].len() - 1].to_vec();
                merged.extend_from_slice(&chunks[k][1..]);
                chunks[k - 1] = merged;
                chunks.remove(k);
            }
        }
    } else {
        chunks.push(stuff.to_vec());
    }
    let total: usize = chunks.iter().map(Vec::len).sum::<usize>() + chunks.len() - 1;
    if total == 0 {
        return r"[^\s\S]".to_owned(); // an empty set: never matches
    }
    let negated = chunks[0].first() == Some(&'!');
    if negated && total == 1 {
        return ".".to_owned(); // `[!]`: any character
    }
    if negated {
        chunks[0].remove(0);
    }
    let mut class = String::from(if negated { "[^" } else { "[" });
    for (k, chunk) in chunks.iter().enumerate() {
        if k > 0 {
            class.push('-');
        }
        for &c in chunk {
            class.push_str(&regex::escape(&c.to_string()));
        }
    }
    class.push(']');
    class
}

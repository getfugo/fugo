//! Go's `path/filepath.Match` (Unix): `*` (not across `/`), `?`, `[...]` with ranges and `^`
//! negation, `\` escapes. Chroma matches lexer file name patterns with it.

/// Whether `name` matches `pattern`; `None` for a malformed pattern (Go's `ErrBadPattern`).
pub(crate) fn matches(pattern: &str, name: &str) -> Option<bool> {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    match_chars(&pattern, &name)
}

fn match_chars(mut pattern: &[char], mut name: &[char]) -> Option<bool> {
    'outer: while !pattern.is_empty() {
        let (star, chunk, rest) = scan_chunk(pattern);
        pattern = rest;
        if star && chunk.is_empty() {
            return Some(!name.contains(&'/'));
        }
        let (t, ok, bad) = match_chunk(chunk, name);
        if ok && (t.is_empty() || !pattern.is_empty()) {
            name = t;
            continue;
        }
        if bad {
            return None;
        }
        if star {
            let mut i = 0;
            while i < name.len() && name[i] != '/' {
                let (t, ok, bad) = match_chunk(chunk, &name[i + 1..]);
                if ok {
                    if pattern.is_empty() && !t.is_empty() {
                        i += 1;
                        continue;
                    }
                    name = t;
                    continue 'outer;
                }
                if bad {
                    return None;
                }
                i += 1;
            }
        }
        // Check that the rest of the pattern is well formed before failing.
        while !pattern.is_empty() {
            let (_, chunk, rest) = scan_chunk(pattern);
            pattern = rest;
            if match_chunk(chunk, &[]).2 {
                return None;
            }
        }
        return Some(false);
    }
    Some(name.is_empty())
}

/// The next chunk: leading stars, then up to the next unbracketed `*`.
fn scan_chunk(pattern: &[char]) -> (bool, &[char], &[char]) {
    let mut p = pattern;
    let mut star = false;
    while p.first() == Some(&'*') {
        p = &p[1..];
        star = true;
    }
    let mut in_range = false;
    let mut i = 0;
    while i < p.len() {
        match p[i] {
            '\\' => {
                if i + 1 < p.len() {
                    i += 1;
                }
            }
            '[' => in_range = true,
            ']' => in_range = false,
            '*' if !in_range => break,
            _ => {}
        }
        i += 1;
    }
    (star, &p[..i], &p[i..])
}

/// Matches `chunk` at the start of `s`: (rest of `s`, matched, malformed pattern).
fn match_chunk<'s>(chunk: &[char], s: &'s [char]) -> (&'s [char], bool, bool) {
    let mut chunk = chunk;
    let mut s = s;
    let mut failed = false;
    while !chunk.is_empty() {
        if !failed && s.is_empty() {
            failed = true;
        }
        match chunk[0] {
            '[' => {
                let mut r = '\0';
                if !failed {
                    r = s[0];
                    s = &s[1..];
                }
                chunk = &chunk[1..];
                let mut negated = false;
                if chunk.first() == Some(&'^') {
                    negated = true;
                    chunk = &chunk[1..];
                }
                let mut matched = false;
                let mut n = 0;
                loop {
                    if !chunk.is_empty() && chunk[0] == ']' && n > 0 {
                        chunk = &chunk[1..];
                        break;
                    }
                    let Some((lo, rest)) = get_esc(chunk) else {
                        return (&[], false, true);
                    };
                    chunk = rest;
                    let mut hi = lo;
                    if chunk.first() == Some(&'-') {
                        let Some((h, rest)) = get_esc(&chunk[1..]) else {
                            return (&[], false, true);
                        };
                        hi = h;
                        chunk = rest;
                    }
                    if lo <= r && r <= hi {
                        matched = true;
                    }
                    n += 1;
                }
                if matched == negated {
                    failed = true;
                }
            }
            '?' => {
                if !failed {
                    if s[0] == '/' {
                        failed = true;
                    }
                    s = &s[1..];
                }
                chunk = &chunk[1..];
            }
            '\\' => {
                chunk = &chunk[1..];
                if chunk.is_empty() {
                    return (&[], false, true);
                }
                if !failed {
                    if chunk[0] != s[0] {
                        failed = true;
                    }
                    s = &s[1..];
                }
                chunk = &chunk[1..];
            }
            c => {
                if !failed {
                    if c != s[0] {
                        failed = true;
                    }
                    s = &s[1..];
                }
                chunk = &chunk[1..];
            }
        }
    }
    if failed {
        return (&[], false, false);
    }
    (s, true, false)
}

/// A possibly escaped character of a `[...]` class.
fn get_esc(chunk: &[char]) -> Option<(char, &[char])> {
    if chunk.is_empty() || chunk[0] == '-' || chunk[0] == ']' {
        return None;
    }
    let mut chunk = chunk;
    if chunk[0] == '\\' {
        chunk = &chunk[1..];
        if chunk.is_empty() {
            return None;
        }
    }
    let r = chunk[0];
    let rest = &chunk[1..];
    if rest.is_empty() {
        return None;
    }
    Some((r, rest))
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn like_go() {
        assert_eq!(matches("*.go", "main.go"), Some(true));
        assert_eq!(matches("*.go", "main.go.bak"), Some(false));
        assert_eq!(matches("*.[1-9]", "ls.1"), Some(true));
        assert_eq!(matches("*.[^1-9]", "ls.1"), Some(false));
        assert_eq!(matches("Caddyfile*", "Caddyfile.prod"), Some(true));
        assert_eq!(matches("*", "a/b"), Some(false));
        assert_eq!(matches("a?c", "abc"), Some(true));
        assert_eq!(matches("*.php[345]", "x.php5"), Some(true));
        assert_eq!(matches("[", "a"), None);
        assert_eq!(matches("*.md", "filename.md"), Some(true));
    }
}

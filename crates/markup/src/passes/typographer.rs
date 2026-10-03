//! The typographer with goldmark's heuristics: `---` `--` `...` `<<` `>>` and smart quotes,
//! decided from the characters around each quote (CommonMark flanking rules plus goldmark's
//! apostrophe cases). Unclosed-quote counters are per block.

use ssg_base::text;
use unicode_properties::{GeneralCategoryGroup, UnicodeGeneralCategory};

use crate::Typographer;

fn is_space(c: char) -> bool {
    c.is_whitespace()
}

/// goldmark's `IsPunctRune`: Unicode punctuation or symbol.
fn is_punct_or_symbol(c: char) -> bool {
    matches!(
        c.general_category_group(),
        GeneralCategoryGroup::Punctuation | GeneralCategoryGroup::Symbol
    )
}

/// Whether a quote run can open and/or close (CommonMark flanking).
fn flanking(before: char, after: char) -> (bool, bool) {
    let (bp, bs) = (is_punct_or_symbol(before), is_space(before));
    let (ap, asp) = (is_punct_or_symbol(after), is_space(after));
    let left = !asp && (!ap || bs || bp);
    let right = !bs && (!bp || asp || ap);
    (left, right)
}

/// Quotes opened and not yet closed in the current block.
#[derive(Default)]
pub(crate) struct Counters {
    single: u32,
    double: u32,
}

/// The characters a replacement can start with.
pub(crate) const TRIGGERS: &[char] = &['\'', '"', '-', '.', '<', '>'];

/// Which replacement applies at the start of `line` (the rest of the source line including
/// its newline), preceded by `before`: `(consumed bytes, replacement)`.
pub(crate) fn replacement<'t>(
    t: &'t Typographer,
    line: &str,
    before: char,
    counters: &mut Counters,
) -> Option<(usize, &'t str)> {
    let b = line.as_bytes();
    let c = *b.first()?;
    if b.len() > 2 {
        if c == b'-' && b[1] == b'-' && b[2] == b'-' {
            return Some((3, &t.em_dash));
        }
        if c == b'.' {
            return (b[1] == b'.' && b[2] == b'.').then_some((3, t.ellipsis.as_str()));
        }
    }
    if b.len() > 1 {
        match c {
            b'<' => return (b[1] == b'<').then_some((2, t.left_angle_quote.as_str())),
            b'>' => return (b[1] == b'>').then_some((2, t.right_angle_quote.as_str())),
            b'-' if b[1] == b'-' => return Some((2, &t.en_dash)),
            _ => {}
        }
    }
    if c != b'\'' && c != b'"' {
        return None;
    }
    let run = b.iter().take_while(|&&x| x == c).count();
    let after = line[run..].chars().next().unwrap_or(' ');
    let (can_open, can_close) = flanking(before, after);
    let second = line[1..].chars().next();
    let punct_at = |i: usize| b.get(i).is_some_and(u8::is_ascii_punctuation);
    let space_at = |i: usize| b.get(i).is_some_and(u8::is_ascii_whitespace);
    let maybe_close = can_close
        && can_open
        && second.is_some_and(text::is_punct)
        && (b.len() == 2 || space_at(2));
    if c == b'\'' {
        // A decade: '90s.
        if can_open
            && !can_close
            && b.len() > 3
            && b[1].is_ascii_digit()
            && b[2].is_ascii_digit()
            && b[3] == b's'
        {
            let after = line.get(4..).and_then(|s| s.chars().next()).unwrap_or(' ');
            if is_space(after) || is_punct_or_symbol(after) {
                return Some((1, &t.apostrophe));
            }
        }
        // A word-initial elision: 'twas, 'em, 'n', 'll.
        if b.len() > 1
            && (text::is_punct(before) || is_space(before))
            && matches!(b[1], b't' | b'e' | b'n' | b'l')
        {
            return Some((1, &t.apostrophe));
        }
        // Between a letter or digit and a letter: it's, Lay's.
        if (text::is_digit(before) || text::is_letter(before))
            && second.is_some_and(text::is_letter)
        {
            return Some((1, &t.apostrophe));
        }
        if can_open && !can_close {
            let contraction = (b.len() > 1
                && matches!(b[1], b's' | b'm' | b't' | b'd')
                && (b.len() < 3 || punct_at(2) || space_at(2)))
                || (b.len() > 2
                    && matches!(&b[1..3], b"ve" | b"ll" | b"re")
                    && (b.len() < 4 || punct_at(3) || space_at(3)));
            if contraction {
                return Some((1, &t.right_single_quote));
            }
            counters.single += 1;
            return Some((1, &t.left_single_quote));
        }
        // goldmark's plural-possessive rule as written (operator precedence included): the
        // quote itself is punctuation, so any quote with two more bytes on its line (the
        // newline counts) and no digit next is a right quote.
        if b.len() > 2 && !second.is_some_and(text::is_digit) {
            return Some((1, &t.right_single_quote));
        }
        if counters.single > 0 && ((can_close && !can_open) || maybe_close) {
            counters.single -= 1;
            return Some((1, &t.right_single_quote));
        }
        return None;
    }
    if can_open && !can_close {
        counters.double += 1;
        return Some((1, &t.left_double_quote));
    }
    if counters.double > 0 && ((can_close && !can_open) || maybe_close) {
        // "Monitor 21"" keeps its inch mark.
        if b.get(1) == Some(&b'"') && text::is_digit(before) {
            return None;
        }
        counters.double -= 1;
        return Some((1, &t.right_double_quote));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Typesets one source line (with its newline, as goldmark sees it).
    fn run_line(s: &str) -> String {
        let s = &format!("{s}\n");
        let t = Typographer::default();
        let mut c = Counters::default();
        let mut out = String::new();
        let mut i = 0;
        while i < s.len() {
            let before = s[..i].chars().next_back().unwrap_or('\n');
            if let Some((len, w)) = replacement(&t, &s[i..], before, &mut c) {
                out.push_str(w);
                i += len;
            } else {
                let ch = s[i..].chars().next().unwrap_or(' ');
                out.push(ch);
                i += ch.len_utf8();
            }
        }
        out.trim_end_matches('\n').to_owned()
    }

    #[test]
    fn quote_before_newline() {
        assert_eq!(
            run_line("[mediaTypes.'text/html']"),
            "[mediaTypes.&rsquo;text/html&rsquo;]"
        );
        assert_eq!(
            run_line("mediaType = 'text/html'"),
            "mediaType = &rsquo;text/html'"
        );
        assert_eq!(
            run_line("excludes = ['**']"),
            "excludes = [&rsquo;**&rsquo;]"
        );
    }

    #[test]
    fn goldmark_cases() {
        assert_eq!(
            run_line(r#""Double quotes" and 'single quotes' and it's and the '90s and 'twas."#),
            "&ldquo;Double quotes&rdquo; and &lsquo;single quotes&rsquo; and it&rsquo;s and the &rsquo;90s and &rsquo;twas."
        );
        assert_eq!(
            run_line(r#"5'10" tall and "Monitor 21"" and "unbalanced."#),
            "5'10\" tall and &ldquo;Monitor 21\"&rdquo; and &ldquo;unbalanced."
        );
        assert_eq!(
            run_line("don't, won't, y'all'd've."),
            "don&rsquo;t, won&rsquo;t, y&rsquo;all&rsquo;d&rsquo;ve."
        );
        assert_eq!(
            run_line("Dashes -- and --- and ellipsis... and <<angle>> quotes."),
            "Dashes &ndash; and &mdash; and ellipsis&hellip; and &laquo;angle&raquo; quotes."
        );
    }
}

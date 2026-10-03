//! Character classes: ranges, Unicode categories, negation and subtraction, with .NET's
//! (regexp2's) membership test and case folding.
//!
//! Ported from `syntax/charclass.go` of github.com/dlclark/regexp2 v1.11.5 (MIT), itself a port
//! of .NET's `RegexCharClass`. Only the parts the default (non-ECMAScript, non-RE2) dialect
//! reaches are kept; Unicode scripts and properties (`\p{Greek}`) are not supported (no Chroma
//! lexer uses them).

use unicode_properties::{GeneralCategory as G, UnicodeGeneralCategory};

/// The highest code point (Go's `utf8.MaxRune`).
const MAX_RUNE: u32 = 0x10_FFFF;

/// A range of code points, inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Range {
    pub first: u32,
    pub last: u32,
}

/// What a category entry stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cat {
    /// `\s` (Go's `unicode.IsSpace`).
    Space,
    /// `\w` (regexp2's `IsWordChar`).
    Word,
    /// A Unicode general category (`Lu`) or category group (`L`).
    General(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Category {
    negate: bool,
    cat: Cat,
}

/// A character class (regexp2's `CharSet`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CharSet {
    ranges: Vec<Range>,
    categories: Vec<Category>,
    sub: Option<Box<CharSet>>,
    pub negate: bool,
    anything: bool,
}

/// The general category names Go's `unicode.Categories` knows.
const CATEGORY_NAMES: &[&str] = &[
    "C", "Cc", "Cf", "Co", "Cs", "L", "Ll", "Lm", "Lo", "Lt", "Lu", "M", "Mc", "Me", "Mn", "N",
    "Nd", "Nl", "No", "P", "Pc", "Pd", "Pe", "Pf", "Pi", "Po", "Ps", "S", "Sc", "Sk", "Sm", "So",
    "Z", "Zl", "Zp", "Zs",
];

/// The interned name of a supported category (`None`: unknown or unsupported).
pub(crate) fn category_name(name: &str) -> Option<&'static str> {
    CATEGORY_NAMES.iter().copied().find(|n| *n == name)
}

/// The two-letter code of `c`'s general category.
fn general(c: char) -> &'static str {
    match c.general_category() {
        G::UppercaseLetter => "Lu",
        G::LowercaseLetter => "Ll",
        G::TitlecaseLetter => "Lt",
        G::ModifierLetter => "Lm",
        G::OtherLetter => "Lo",
        G::NonspacingMark => "Mn",
        G::SpacingMark => "Mc",
        G::EnclosingMark => "Me",
        G::DecimalNumber => "Nd",
        G::LetterNumber => "Nl",
        G::OtherNumber => "No",
        G::ConnectorPunctuation => "Pc",
        G::DashPunctuation => "Pd",
        G::OpenPunctuation => "Ps",
        G::ClosePunctuation => "Pe",
        G::InitialPunctuation => "Pi",
        G::FinalPunctuation => "Pf",
        G::OtherPunctuation => "Po",
        G::MathSymbol => "Sm",
        G::CurrencySymbol => "Sc",
        G::ModifierSymbol => "Sk",
        G::OtherSymbol => "So",
        G::SpaceSeparator => "Zs",
        G::LineSeparator => "Zl",
        G::ParagraphSeparator => "Zp",
        G::Control => "Cc",
        G::Format => "Cf",
        G::Surrogate => "Cs",
        G::PrivateUse => "Co",
        G::Unassigned => "Cn",
    }
}

/// Whether `c` is in the category (or category group) `name` (Go's `unicode.Is`; Go's `C`
/// does not include unassigned code points).
fn in_category(name: &str, c: char) -> bool {
    let g = general(c);
    if name.len() == 1 {
        g.starts_with(name) && g != "Cn"
    } else {
        g == name
    }
}

/// regexp2's `IsWordChar`: letters, non-spacing marks, decimal digits, connector punctuation,
/// ZWJ and ZWNJ.
pub(crate) fn is_word_char(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric() || c == '_';
    }
    matches!(
        general(c),
        "Lu" | "Ll" | "Lt" | "Lm" | "Lo" | "Mn" | "Nd" | "Pc"
    ) || c == '\u{200D}'
        || c == '\u{200C}'
}

/// Go's `unicode.IsSpace`.
pub(crate) fn is_space(c: char) -> bool {
    match c {
        '\t' | '\n' | '\u{B}' | '\u{C}' | '\r' | ' ' | '\u{85}' | '\u{A0}' => true,
        c if (c as u32) <= 0xFF => false,
        c => c.is_whitespace(),
    }
}

/// Go's `unicode.ToLower` (the simple lower-case mapping).
pub(crate) fn to_lower(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    if c == '\u{130}' {
        return 'i';
    }
    let mut it = c.to_lowercase();
    match (it.next(), it.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

impl CharSet {
    /// `.` with `s`: every character.
    pub fn any() -> Self {
        Self {
            ranges: vec![Range {
                first: 0,
                last: MAX_RUNE,
            }],
            ..Self::default()
        }
    }

    fn with_category(negate_set: bool, negate_cat: bool, cat: Cat) -> Self {
        Self {
            negate: negate_set,
            categories: vec![Category {
                negate: negate_cat,
                cat,
            }],
            ..Self::default()
        }
    }

    /// `\w` / `\W`.
    pub fn word(negate: bool) -> Self {
        Self::with_category(negate, false, Cat::Word)
    }

    /// `\s` / `\S`.
    pub fn space(negate: bool) -> Self {
        Self::with_category(negate, false, Cat::Space)
    }

    /// `\d` / `\D` (regexp2 negates the category, not the set).
    pub fn digit(negate: bool) -> Self {
        Self::with_category(false, negate, Cat::General("Nd"))
    }

    /// regexp2's `CharIn`, including its first-match-wins walk over the categories.
    pub fn contains(&self, ch: char) -> bool {
        let c = ch as u32;
        let mut val = self.ranges.iter().any(|r| r.first <= c && c <= r.last);
        if !val {
            for ct in &self.categories {
                let is = match ct.cat {
                    Cat::Space => is_space(ch),
                    Cat::Word => is_word_char(ch),
                    Cat::General(name) => in_category(name, ch),
                };
                if is {
                    val = !ct.negate;
                    break;
                } else if ct.negate {
                    val = true;
                    break;
                }
            }
        }
        if self.negate {
            val = !val;
        }
        if val && let Some(sub) = &self.sub {
            val = !sub.contains(ch);
        }
        val
    }

    /// A single character (`IsSingleton`).
    pub fn singleton(&self) -> Option<char> {
        (!self.negate
            && self.categories.is_empty()
            && self.sub.is_none()
            && self.ranges.len() == 1
            && self.ranges[0].first == self.ranges[0].last)
            .then(|| char::from_u32(self.ranges[0].first))
            .flatten()
    }

    /// Everything but a single character (`IsSingletonInverse`).
    pub fn singleton_inverse(&self) -> Option<char> {
        (self.negate
            && self.categories.is_empty()
            && self.sub.is_none()
            && self.ranges.len() == 1
            && self.ranges[0].first == self.ranges[0].last)
            .then(|| char::from_u32(self.ranges[0].first))
            .flatten()
    }

    pub fn add_digit(&mut self, negate: bool) {
        self.add_categories(&[Category {
            negate,
            cat: Cat::General("Nd"),
        }]);
    }

    pub fn add_space(&mut self, negate: bool) {
        self.add_categories(&[Category {
            negate,
            cat: Cat::Space,
        }]);
    }

    pub fn add_word(&mut self, negate: bool) {
        self.add_categories(&[Category {
            negate,
            cat: Cat::Word,
        }]);
    }

    fn make_anything(&mut self) {
        self.anything = true;
        self.categories.clear();
        self.ranges = vec![Range {
            first: 0,
            last: MAX_RUNE,
        }];
    }

    fn add_categories(&mut self, cats: &[Category]) {
        if self.anything {
            return;
        }
        for ct in cats {
            let mut found = false;
            for ct2 in &self.categories {
                if ct.cat == ct2.cat {
                    if ct.negate != ct2.negate {
                        self.make_anything();
                        return;
                    }
                    found = true;
                    break;
                }
            }
            if !found {
                self.categories.push(*ct);
            }
        }
    }

    /// `\p{name}` / `\P{name}` (`addCategory`): with `ignore_case`, `Ll`, `Lu` and `Lt` all
    /// match.
    pub fn add_category(&mut self, name: &'static str, negate: bool, ignore_case: bool) {
        if ignore_case && matches!(name, "Ll" | "Lu" | "Lt") {
            self.add_categories(&[
                Category {
                    negate,
                    cat: Cat::General("Ll"),
                },
                Category {
                    negate,
                    cat: Cat::General("Lu"),
                },
                Category {
                    negate,
                    cat: Cat::General("Lt"),
                },
            ]);
        }
        self.add_categories(&[Category {
            negate,
            cat: Cat::General(name),
        }]);
    }

    pub fn add_subtraction(&mut self, sub: CharSet) {
        self.sub = Some(Box::new(sub));
    }

    pub fn add_char(&mut self, c: char) {
        self.add_range(c as u32, c as u32);
    }

    pub fn add_range(&mut self, first: u32, last: u32) {
        self.ranges.push(Range { first, last });
        self.canonicalize();
    }

    /// Merges a set into this one (`addSet`, used when merging alternations).
    #[cfg(test)]
    fn add_set(&mut self, set: &CharSet) {
        if self.anything {
            return;
        }
        if set.anything {
            self.make_anything();
            return;
        }
        self.ranges.extend_from_slice(&set.ranges);
        self.add_categories(&set.categories);
        self.canonicalize();
    }

    /// Sorts the ranges and merges overlapping or abutting ones (`canonicalize`).
    fn canonicalize(&mut self) {
        if self.ranges.len() <= 1 {
            return;
        }
        self.ranges.sort_by_key(|r| r.first);
        let mut out: Vec<Range> = Vec::with_capacity(self.ranges.len());
        for r in &self.ranges {
            match out.last_mut() {
                Some(last) if last.last == MAX_RUNE || r.first <= last.last.saturating_add(1) => {
                    if last.last < r.last {
                        last.last = r.last;
                    }
                }
                _ => out.push(*r),
            }
        }
        self.ranges = out;
    }

    /// Adds the lower-case versions of the class's characters (`addLowercase`): a single
    /// character is replaced by its lower case, a range gains the lower-case ranges.
    pub fn add_lowercase(&mut self) {
        if self.anything {
            return;
        }
        let mut to_add = Vec::new();
        for r in &mut self.ranges {
            if r.first == r.last {
                let lower = char::from_u32(r.first).map_or(r.first, |c| to_lower(c) as u32);
                *r = Range {
                    first: lower,
                    last: lower,
                };
            } else {
                to_add.push(*r);
            }
        }
        for r in to_add {
            self.add_lowercase_range(r.first, r.last);
        }
        self.canonicalize();
    }

    fn add_lowercase_range(&mut self, ch_min: u32, ch_max: u32) {
        let mut i = LC_TABLE.partition_point(|lc| lc.max < ch_min);
        while i < LC_TABLE.len() {
            let lc = &LC_TABLE[i];
            i += 1;
            if lc.min > ch_max {
                return;
            }
            let mut min_t = lc.min.max(ch_min);
            let mut max_t = lc.max.min(ch_max);
            match lc.op {
                LcOp::Set(v) => {
                    min_t = v;
                    max_t = v;
                }
                LcOp::Add(d) => {
                    min_t = min_t.wrapping_add_signed(d);
                    max_t = max_t.wrapping_add_signed(d);
                }
                LcOp::Bor => {
                    min_t |= 1;
                    max_t |= 1;
                }
                LcOp::Bad => {
                    min_t += min_t & 1;
                    max_t += max_t & 1;
                }
            }
            if min_t < ch_min || max_t > ch_max {
                self.add_range(min_t, max_t);
            }
        }
    }
}

/// How the lower case of a run of code points is computed (`lcMap.op`).
#[derive(Clone, Copy)]
enum LcOp {
    Set(u32),
    Add(i32),
    Bor,
    Bad,
}

struct LcMap {
    min: u32,
    max: u32,
    op: LcOp,
}

const fn lc(min: u32, max: u32, op: LcOp) -> LcMap {
    LcMap { min, max, op }
}

/// regexp2's `lcTable`: intervals on which the lower-case function is non-decreasing.
const LC_TABLE: &[LcMap] = &[
    lc(0x0041, 0x005A, LcOp::Add(32)),
    lc(0x00C0, 0x00DE, LcOp::Add(32)),
    lc(0x0100, 0x012E, LcOp::Bor),
    lc(0x0130, 0x0130, LcOp::Set(0x0069)),
    lc(0x0132, 0x0136, LcOp::Bor),
    lc(0x0139, 0x0147, LcOp::Bad),
    lc(0x014A, 0x0176, LcOp::Bor),
    lc(0x0178, 0x0178, LcOp::Set(0x00FF)),
    lc(0x0179, 0x017D, LcOp::Bad),
    lc(0x0181, 0x0181, LcOp::Set(0x0253)),
    lc(0x0182, 0x0184, LcOp::Bor),
    lc(0x0186, 0x0186, LcOp::Set(0x0254)),
    lc(0x0187, 0x0187, LcOp::Set(0x0188)),
    lc(0x0189, 0x018A, LcOp::Add(205)),
    lc(0x018B, 0x018B, LcOp::Set(0x018C)),
    lc(0x018E, 0x018E, LcOp::Set(0x01DD)),
    lc(0x018F, 0x018F, LcOp::Set(0x0259)),
    lc(0x0190, 0x0190, LcOp::Set(0x025B)),
    lc(0x0191, 0x0191, LcOp::Set(0x0192)),
    lc(0x0193, 0x0193, LcOp::Set(0x0260)),
    lc(0x0194, 0x0194, LcOp::Set(0x0263)),
    lc(0x0196, 0x0196, LcOp::Set(0x0269)),
    lc(0x0197, 0x0197, LcOp::Set(0x0268)),
    lc(0x0198, 0x0198, LcOp::Set(0x0199)),
    lc(0x019C, 0x019C, LcOp::Set(0x026F)),
    lc(0x019D, 0x019D, LcOp::Set(0x0272)),
    lc(0x019F, 0x019F, LcOp::Set(0x0275)),
    lc(0x01A0, 0x01A4, LcOp::Bor),
    lc(0x01A7, 0x01A7, LcOp::Set(0x01A8)),
    lc(0x01A9, 0x01A9, LcOp::Set(0x0283)),
    lc(0x01AC, 0x01AC, LcOp::Set(0x01AD)),
    lc(0x01AE, 0x01AE, LcOp::Set(0x0288)),
    lc(0x01AF, 0x01AF, LcOp::Set(0x01B0)),
    lc(0x01B1, 0x01B2, LcOp::Add(217)),
    lc(0x01B3, 0x01B5, LcOp::Bad),
    lc(0x01B7, 0x01B7, LcOp::Set(0x0292)),
    lc(0x01B8, 0x01B8, LcOp::Set(0x01B9)),
    lc(0x01BC, 0x01BC, LcOp::Set(0x01BD)),
    lc(0x01C4, 0x01C5, LcOp::Set(0x01C6)),
    lc(0x01C7, 0x01C8, LcOp::Set(0x01C9)),
    lc(0x01CA, 0x01CB, LcOp::Set(0x01CC)),
    lc(0x01CD, 0x01DB, LcOp::Bad),
    lc(0x01DE, 0x01EE, LcOp::Bor),
    lc(0x01F1, 0x01F2, LcOp::Set(0x01F3)),
    lc(0x01F4, 0x01F4, LcOp::Set(0x01F5)),
    lc(0x01FA, 0x0216, LcOp::Bor),
    lc(0x0386, 0x0386, LcOp::Set(0x03AC)),
    lc(0x0388, 0x038A, LcOp::Add(37)),
    lc(0x038C, 0x038C, LcOp::Set(0x03CC)),
    lc(0x038E, 0x038F, LcOp::Add(63)),
    lc(0x0391, 0x03AB, LcOp::Add(32)),
    lc(0x03E2, 0x03EE, LcOp::Bor),
    lc(0x0401, 0x040F, LcOp::Add(80)),
    lc(0x0410, 0x042F, LcOp::Add(32)),
    lc(0x0460, 0x0480, LcOp::Bor),
    lc(0x0490, 0x04BE, LcOp::Bor),
    lc(0x04C1, 0x04C3, LcOp::Bad),
    lc(0x04C7, 0x04C7, LcOp::Set(0x04C8)),
    lc(0x04CB, 0x04CB, LcOp::Set(0x04CC)),
    lc(0x04D0, 0x04EA, LcOp::Bor),
    lc(0x04EE, 0x04F4, LcOp::Bor),
    lc(0x04F8, 0x04F8, LcOp::Set(0x04F9)),
    lc(0x0531, 0x0556, LcOp::Add(48)),
    lc(0x10A0, 0x10C5, LcOp::Add(48)),
    lc(0x1E00, 0x1EF8, LcOp::Bor),
    lc(0x1F08, 0x1F0F, LcOp::Add(-8)),
    lc(0x1F18, 0x1F1F, LcOp::Add(-8)),
    lc(0x1F28, 0x1F2F, LcOp::Add(-8)),
    lc(0x1F38, 0x1F3F, LcOp::Add(-8)),
    lc(0x1F48, 0x1F4D, LcOp::Add(-8)),
    lc(0x1F59, 0x1F59, LcOp::Set(0x1F51)),
    lc(0x1F5B, 0x1F5B, LcOp::Set(0x1F53)),
    lc(0x1F5D, 0x1F5D, LcOp::Set(0x1F55)),
    lc(0x1F5F, 0x1F5F, LcOp::Set(0x1F57)),
    lc(0x1F68, 0x1F6F, LcOp::Add(-8)),
    lc(0x1F88, 0x1F8F, LcOp::Add(-8)),
    lc(0x1F98, 0x1F9F, LcOp::Add(-8)),
    lc(0x1FA8, 0x1FAF, LcOp::Add(-8)),
    lc(0x1FB8, 0x1FB9, LcOp::Add(-8)),
    lc(0x1FBA, 0x1FBB, LcOp::Add(-74)),
    lc(0x1FBC, 0x1FBC, LcOp::Set(0x1FB3)),
    lc(0x1FC8, 0x1FCB, LcOp::Add(-86)),
    lc(0x1FCC, 0x1FCC, LcOp::Set(0x1FC3)),
    lc(0x1FD8, 0x1FD9, LcOp::Add(-8)),
    lc(0x1FDA, 0x1FDB, LcOp::Add(-100)),
    lc(0x1FE8, 0x1FE9, LcOp::Add(-8)),
    lc(0x1FEA, 0x1FEB, LcOp::Add(-112)),
    lc(0x1FEC, 0x1FEC, LcOp::Set(0x1FE5)),
    lc(0x1FF8, 0x1FF9, LcOp::Add(-128)),
    lc(0x1FFA, 0x1FFB, LcOp::Add(-126)),
    lc(0x1FFC, 0x1FFC, LcOp::Set(0x1FF3)),
    lc(0x2160, 0x216F, LcOp::Add(16)),
    lc(0x24B6, 0x24D0, LcOp::Add(26)),
    lc(0xFF21, 0xFF3A, LcOp::Add(32)),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn membership_follows_regexp2() {
        let mut s = CharSet::default();
        s.add_range('a' as u32, 'z' as u32);
        assert!(s.contains('q'));
        assert!(!s.contains('Q'));
        s.add_lowercase();
        assert!(s.contains('q'));
        // `[\W\d]`: regexp2 stops at the first category that decides, so a digit is out.
        let mut w = CharSet::default();
        w.add_word(true);
        w.add_digit(false);
        assert!(!w.contains('5'));
        assert!(w.contains('-'));
        let mut sub = CharSet::default();
        sub.add_char('e');
        s.add_subtraction(sub);
        assert!(!s.contains('e'));
        assert!(CharSet::word(false).contains('é'));
        assert!(CharSet::space(false).contains('\u{A0}'));
        assert!(CharSet::digit(true).contains('x'));
        let mut m = CharSet::default();
        m.add_char('a');
        m.add_set(&CharSet::any());
        assert!(m.contains('\n'));
    }

    #[test]
    fn lower_case_of_ranges() {
        let mut s = CharSet::default();
        s.add_range('A' as u32, 'F' as u32);
        s.add_lowercase();
        assert!(s.contains('c') && s.contains('C'));
        assert_eq!(to_lower('İ'), 'i');
        assert_eq!(to_lower('Σ'), 'σ');
    }
}

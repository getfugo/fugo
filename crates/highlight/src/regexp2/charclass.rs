//! Character classes: ranges, Unicode categories, negation and subtraction, with .NET's
//! (regexp2's) membership test and case folding.
//!
//! Ported from `syntax/charclass.go` of github.com/dlclark/regexp2 v1.11.5 (MIT), itself a port
//! of .NET's `RegexCharClass`. Only the parts the default (non-ECMAScript, non-RE2) dialect
//! reaches are kept; Unicode scripts and properties (`\p{Greek}`) are not supported (no Chroma
//! lexer uses them).

use unicode_properties::{GeneralCategory as G, UnicodeGeneralCategory};

mod lowercase;

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
}

#[cfg(test)]
mod tests;

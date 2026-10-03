//! English inflection: `pluralize`, `singularize`, `humanize`, `ordinalize`, `titleize`.
//!
//! The rules and word lists are those of gobuffalo/flect v1.0.3 (MIT), which Go uses for
//! section titles and the template functions of the same names. No inflection crate matches
//! them (see the crate README), so the data lives here.
//!
//! A string is first split into words: white space and `_ : - /` separate words, an upper-case
//! letter after a non-upper-case one starts a word, and characters other than letters, digits
//! and punctuation are dropped. Words that are acronyms (`API`, `HTML`, …) are upper-cased.
//! [`pluralize`] and [`singularize`] change only the last word: a dictionary of irregular and
//! uncountable words, then suffix rules, then `s`.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use crate::text;

/// Inflection data that could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum InflectError {
    #[error("custom inflections: {0}")]
    Json(#[from] serde_json::Error),
    #[error("custom inflections: {singular:?} -> {plural:?} is not a single word")]
    MultiWord { singular: String, plural: String },
    #[error("custom inflection {0:?} repeats a built-in word")]
    Duplicate(String),
}

/// User additions (`inflections.json`: singular → plural; `acronyms.json`: a list).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CustomInflections {
    pub inflections: Vec<(String, String)>,
    pub acronyms: Vec<String>,
}

impl CustomInflections {
    /// Decodes the first JSON value of each file's contents: an object of strings and an array
    /// of strings; `null` values count as empty strings.
    ///
    /// # Errors
    /// Invalid JSON, a value of another type, or an inflection that is not a single word.
    pub fn from_json(
        inflections: Option<&str>,
        acronyms: Option<&str>,
    ) -> Result<Self, InflectError> {
        fn first<T: serde::de::DeserializeOwned>(s: &str) -> Result<Option<T>, serde_json::Error> {
            serde_json::Deserializer::from_str(s)
                .into_iter::<Option<T>>()
                .next()
                .transpose()
                .map(Option::flatten)
                .and_then(|v| match v {
                    Some(v) => Ok(Some(v)),
                    None if s.trim().is_empty() => Err(serde::de::Error::custom("empty input")),
                    None => Ok(None),
                })
        }
        let mut out = Self::default();
        if let Some(s) = inflections {
            let m: Option<std::collections::BTreeMap<String, Option<String>>> = first(s)?;
            for (singular, plural) in m.unwrap_or_default() {
                let plural = plural.unwrap_or_default();
                if singular.contains(' ') || plural.contains(' ') {
                    return Err(InflectError::MultiWord { singular, plural });
                }
                out.inflections.push((singular, plural));
            }
        }
        if let Some(s) = acronyms {
            let a: Option<Vec<Option<String>>> = first(s)?;
            out.acronyms = a
                .unwrap_or_default()
                .into_iter()
                .map(Option::unwrap_or_default)
                .collect();
        }
        Ok(out)
    }
}

/// A suffix rule: words ending in `suffix` get it replaced by `replacement` (`None`: the word
/// is already inflected and stays as it is).
#[derive(Clone, Debug)]
struct Rule {
    suffix: &'static str,
    replacement: Option<&'static str>,
}

#[derive(Clone, Debug)]
enum Owned {
    Static(&'static str),
    Custom(String),
}

impl Owned {
    fn as_str(&self) -> &str {
        match self {
            Self::Static(s) => s,
            Self::Custom(s) => s,
        }
    }
}

/// The inflection tables: the built-in ones, optionally extended by [`CustomInflections`].
#[derive(Clone, Debug)]
pub struct Inflector {
    acronyms: HashSet<String>,
    single_to_plural: HashMap<String, Owned>,
    plural_to_single: HashMap<String, Owned>,
    plural_rules: Vec<Rule>,
    singular_rules: Vec<Rule>,
}

static STANDARD: LazyLock<Inflector> = LazyLock::new(|| {
    Inflector::with_custom(&CustomInflections::default())
        .expect("the built-in tables are consistent")
});

impl Inflector {
    /// The built-in tables.
    #[must_use]
    pub fn standard() -> &'static Self {
        &STANDARD
    }

    /// The built-in tables with user additions.
    ///
    /// # Errors
    /// When a custom word is also a built-in dictionary word (as singular or plural).
    pub fn with_custom(custom: &CustomInflections) -> Result<Self, InflectError> {
        let mut single_to_plural: HashMap<String, Owned> = HashMap::new();
        let mut plural_to_single: HashMap<String, Owned> = HashMap::new();
        for (s, p) in &custom.inflections {
            single_to_plural.insert(s.clone(), Owned::Custom(p.clone()));
            plural_to_single.insert(p.clone(), Owned::Custom(s.clone()));
        }
        let taken =
            |m: &HashMap<String, Owned>, k: &str| m.get(k).is_some_and(|v| !v.as_str().is_empty());
        for w in DICTIONARY {
            if taken(&single_to_plural, w.singular) {
                return Err(InflectError::Duplicate(w.singular.to_owned()));
            }
            let plural = w.plural_or_self();
            single_to_plural.insert(w.singular.to_owned(), Owned::Static(plural));
            if w.kind != Kind::Unidirectional {
                for p in [Some(plural), w.alternative].into_iter().flatten() {
                    if taken(&plural_to_single, p) {
                        return Err(InflectError::Duplicate(p.to_owned()));
                    }
                    plural_to_single.insert(p.to_owned(), Owned::Static(w.singular));
                }
            }
        }

        // Rules are prepended as they are added (the last added is tried first), each rule
        // preceded by one that keeps words already ending in its replacement.
        let mut plural_rules = Vec::new();
        let mut singular_rules = Vec::new();
        let add = |rules: &mut Vec<Rule>, suffix: &'static str, replacement: &'static str| {
            rules.push(Rule {
                suffix,
                replacement: Some(replacement),
            });
            rules.push(Rule {
                suffix: replacement,
                replacement: None,
            });
        };
        for &(singular, plural) in SUFFIXES.iter().rev() {
            add(&mut plural_rules, singular, plural);
            add(&mut singular_rules, plural, singular);
        }
        for w in DICTIONARY.iter().filter(|w| w.kind != Kind::Exact) {
            let plural = w.plural_or_self();
            add(&mut plural_rules, w.singular, plural);
            if w.kind != Kind::Unidirectional {
                add(&mut singular_rules, plural, w.singular);
                if let Some(alt) = w.alternative {
                    add(&mut singular_rules, alt, w.singular);
                }
            }
        }
        plural_rules.reverse();
        singular_rules.reverse();

        let mut acronyms: HashSet<String> = ACRONYMS.iter().map(|a| (*a).to_owned()).collect();
        acronyms.extend(custom.acronyms.iter().cloned());
        Ok(Self {
            acronyms,
            single_to_plural,
            plural_to_single,
            plural_rules,
            singular_rules,
        })
    }

    fn is_acronym(&self, s: &str) -> bool {
        self.acronyms.contains(&text::to_upper(s))
    }

    /// Adds `word` to `words` after trimming white space and the separators `_ : - /`
    /// (upper-cased when it is an acronym; dropped when empty).
    fn push_word(&self, words: &mut Vec<String>, word: &str) {
        let mut w = word.trim();
        for sep in SEPARATORS {
            w = w.trim_matches(sep);
        }
        if w.is_empty() {
            return;
        }
        let upper = text::to_upper(w);
        if self.acronyms.contains(&upper) {
            words.push(upper);
        } else {
            words.push(w.to_owned());
        }
    }

    /// The words of `s`.
    #[must_use]
    pub fn words(&self, s: &str) -> Vec<String> {
        let s = s.trim();
        let mut words = Vec::new();
        if s.is_empty() {
            return words;
        }
        let upper = text::to_upper(s);
        if self.acronyms.contains(&upper) {
            return vec![upper];
        }
        let mut prev = '\0';
        let mut word = String::new();
        for c in s.chars() {
            let starts_word = is_separator(c)
                || (text::is_upper(c) && (!text::is_upper(prev) || self.is_acronym(&word)));
            if starts_word {
                self.push_word(&mut words, &std::mem::take(&mut word));
                word.push(c);
            } else if text::is_letter(c) || text::is_digit(c) || text::is_punct(c) || c == '`' {
                word.push(c);
            } else {
                self.push_word(&mut words, &std::mem::take(&mut word));
            }
            prev = c;
        }
        self.push_word(&mut words, &word);
        words
    }

    /// `s` with the trailing `old` replaced by `new` (appended when `s` does not end in `old`).
    fn replace_suffix(s: &str, old: &str, new: &str) -> String {
        format!("{}{new}", s.strip_suffix(old).unwrap_or(s))
    }

    /// The plural of the last word of `s` (`box` → `boxes`, `child` → `children`).
    #[must_use]
    pub fn pluralize(&self, s: &str) -> String {
        let words = self.words(s);
        let Some(last) = words.last() else {
            return String::new();
        };
        if let Some(p) = self.single_to_plural.get(s) {
            return p.as_str().to_owned();
        }
        if self.plural_to_single.contains_key(s) {
            return s.to_owned();
        }
        let lower = text::to_lower(last);
        if self.plural_to_single.contains_key(&lower) {
            return s.to_owned();
        }
        if let Some(p) = self.single_to_plural.get(&lower) {
            let p = self.match_capital(last, p.as_str());
            return Self::replace_suffix(s, last, &p);
        }
        if let Some(r) = self.plural_rules.iter().find(|r| last.ends_with(r.suffix)) {
            return Self::replace_suffix(s, last, &r.apply(last));
        }
        if lower.ends_with('s') {
            return s.to_owned();
        }
        format!("{s}s")
    }

    /// The singular of the last word of `s` (`boxes` → `box`, `people` → `person`).
    #[must_use]
    pub fn singularize(&self, s: &str) -> String {
        let words = self.words(s);
        let Some(last) = words.last() else {
            return s.to_owned();
        };
        if let Some(p) = self.plural_to_single.get(s) {
            return p.as_str().to_owned();
        }
        if self.single_to_plural.contains_key(s) {
            return s.to_owned();
        }
        let lower = text::to_lower(last);
        if let Some(p) = self.plural_to_single.get(&lower) {
            let p = self.match_capital(last, p.as_str());
            return Self::replace_suffix(s, last, &p);
        }
        if self.single_to_plural.contains_key(&lower) {
            return s.to_owned();
        }
        if let Some(r) = self
            .singular_rules
            .iter()
            .find(|r| last.ends_with(r.suffix))
        {
            return Self::replace_suffix(s, last, &r.apply(last));
        }
        if last.ends_with('s') {
            return Self::replace_suffix(s, "s", "");
        }
        s.to_owned()
    }

    /// `replacement`, capitalised when `word` is.
    fn match_capital(&self, word: &str, replacement: &str) -> String {
        if word == self.capitalize(word) {
            self.capitalize(replacement)
        } else {
            replacement.to_owned()
        }
    }

    /// `s` with its first character in title case (empty when `s` has no words).
    #[must_use]
    pub fn capitalize(&self, s: &str) -> String {
        if self.words(s).is_empty() {
            return String::new();
        }
        title_first(s)
    }

    /// The words of `s`, each with its first character in title case, joined by spaces.
    #[must_use]
    pub fn titleize(&self, s: &str) -> String {
        self.words(s)
            .iter()
            .map(|w| title_first(w))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The words of `s` joined by spaces, the first titleized (`potato-chips` →
    /// `Potato chips`). Blank input is returned as it is; so is input without words (`+`),
    /// where Go fails.
    #[must_use]
    pub fn humanize(&self, s: &str) -> String {
        if s.trim().is_empty() {
            return s.to_owned();
        }
        let words = self.words(s);
        let Some((first, rest)) = words.split_first() else {
            return s.to_owned();
        };
        let mut out = Vec::with_capacity(words.len());
        self.push_word(&mut out, &self.titleize(first));
        for w in rest {
            self.push_word(&mut out, w);
        }
        out.join(" ")
    }
}

impl Rule {
    fn apply(&self, word: &str) -> String {
        match self.replacement {
            Some(r) => Inflector::replace_suffix(word, self.suffix, r),
            None => word.to_owned(),
        }
    }
}

/// `s` with its first character mapped to title case.
fn title_first(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |c| {
        let mut out = String::with_capacity(s.len());
        out.push(text::title_char(c));
        out.push_str(chars.as_str());
        out
    })
}

/// The characters that separate words besides white space.
const SEPARATORS: [char; 5] = ['_', ' ', ':', '-', '/'];

fn is_separator(c: char) -> bool {
    SEPARATORS.contains(&c) || c.is_whitespace()
}

/// [`Inflector::pluralize`] with the built-in tables.
#[must_use]
pub fn pluralize(s: &str) -> String {
    Inflector::standard().pluralize(s)
}

/// [`Inflector::singularize`] with the built-in tables.
#[must_use]
pub fn singularize(s: &str) -> String {
    Inflector::standard().singularize(s)
}

/// [`Inflector::humanize`] with the built-in tables.
#[must_use]
pub fn humanize(s: &str) -> String {
    Inflector::standard().humanize(s)
}

/// [`Inflector::titleize`] with the built-in tables.
#[must_use]
pub fn titleize(s: &str) -> String {
    Inflector::standard().titleize(s)
}

/// The English ordinal of `n` (`1st`, `12th`, `-22nd`).
#[must_use]
pub fn ordinalize(n: i64) -> String {
    let suffix = match n.unsigned_abs() % 100 {
        11..=13 => "th",
        m => match m % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    };
    format!("{n}{suffix}")
}

/// [`ordinalize`] of `s` read as a decimal integer; other strings are returned unchanged.
#[must_use]
pub fn ordinalize_str(s: &str) -> String {
    s.parse::<i64>().map_or_else(|_| s.to_owned(), ordinalize)
}

/// Template `humanize`: an integer becomes its ordinal, anything else is humanized, lower-cased
/// and humanized again (`"Returns a relative URL."` → `"Returns a relative url."`).
#[must_use]
pub fn humanize_text(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    if let Ok(n) = s.parse::<i64>() {
        return ordinalize(n);
    }
    humanize(&text::to_lower(&humanize(s)))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Regular,
    /// The plural cannot be turned back into this singular.
    Unidirectional,
    /// Uncountable: the plural is the singular.
    Uncountable,
    /// Only the whole word, never as a suffix rule.
    Exact,
}

struct Word {
    singular: &'static str,
    plural: &'static str,
    alternative: Option<&'static str>,
    kind: Kind,
}

impl Word {
    fn plural_or_self(&self) -> &'static str {
        if self.kind == Kind::Uncountable && self.plural.is_empty() {
            self.singular
        } else {
            self.plural
        }
    }
}

const fn w(singular: &'static str, plural: &'static str) -> Word {
    Word {
        singular,
        plural,
        alternative: None,
        kind: Kind::Regular,
    }
}

const fn alt(singular: &'static str, plural: &'static str, alternative: &'static str) -> Word {
    Word {
        alternative: Some(alternative),
        ..w(singular, plural)
    }
}

const fn uni(singular: &'static str, plural: &'static str) -> Word {
    Word {
        kind: Kind::Unidirectional,
        ..w(singular, plural)
    }
}

const fn exact(singular: &'static str, plural: &'static str) -> Word {
    Word {
        kind: Kind::Exact,
        ..w(singular, plural)
    }
}

const fn unc(singular: &'static str) -> Word {
    Word {
        kind: Kind::Uncountable,
        ..w(singular, "")
    }
}

/// Acronyms kept upper case. Lookups upper-case the word first, so the mixed-case entries
/// (`gbps`, `WiFi`, …) never match; they are kept as in flect.
const ACRONYMS: &[&str] = &[
    "OK", "UTF8", "HTML", "JSON", "JWT", "ID", "UUID", "SQL", "ACK", "ACL", "ADSL", "AES", "ANSI",
    "API", "ARP", "ATM", "BGP", "BSS", "CCITT", "CHAP", "CIDR", "CIR", "CLI", "CPE", "CPU", "CRC",
    "CRT", "CSMA", "CMOS", "DCE", "DEC", "DES", "DHCP", "DNS", "DRAM", "DSL", "DSLAM", "DTE",
    "DMI", "EHA", "EIA", "EIGRP", "EOF", "ESS", "FCC", "FCS", "FDDI", "FTP", "GBIC", "gbps",
    "GEPOF", "HDLC", "HTTP", "HTTPS", "IANA", "ICMP", "IDF", "IDS", "IEEE", "IETF", "IMAP", "IP",
    "IPS", "ISDN", "ISP", "kbps", "LACP", "LAN", "LAPB", "LAPF", "LLC", "MAC", "Mbps", "MC", "MDF",
    "MIB", "MoCA", "MPLS", "MTU", "NAC", "NAT", "NBMA", "NIC", "NRZ", "NRZI", "NVRAM", "OSI",
    "OSPF", "OUI", "PAP", "PAT", "PC", "PIM", "PCM", "PDU", "POP3", "POTS", "PPP", "PPTP", "PTT",
    "PVST", "RAM", "RARP", "RFC", "RIP", "RLL", "ROM", "RSTP", "RTP", "RCP", "SDLC", "SFD", "SFP",
    "SLARP", "SLIP", "SMTP", "SNA", "SNAP", "SNMP", "SOF", "SRAM", "SSH", "SSID", "STP", "SYN",
    "TDM", "TFTP", "TIA", "TOFU", "UDP", "URL", "URI", "USB", "UTP", "VC", "VLAN", "VLSM", "VPN",
    "W3C", "WAN", "WEP", "WiFi", "WPA", "WWW",
];

/// Irregular, uncountable and exception words (flect's `dictionary`), in flect's order.
const DICTIONARY: &[Word] = &[
    w("aircraft", "aircraft"),
    alt("beef", "beef", "beefs"),
    w("bison", "bison"),
    uni("blues", "blues"),
    w("chassis", "chassis"),
    w("deer", "deer"),
    alt("fish", "fish", "fishes"),
    w("moose", "moose"),
    w("police", "police"),
    alt("salmon", "salmon", "salmons"),
    w("series", "series"),
    w("sheep", "sheep"),
    alt("shrimp", "shrimp", "shrimps"),
    w("species", "species"),
    alt("swine", "swine", "swines"),
    alt("trout", "trout", "trouts"),
    alt("tuna", "tuna", "tunas"),
    w("you", "you"),
    w("child", "children"),
    exact("ox", "oxen"),
    w("foot", "feet"),
    w("goose", "geese"),
    w("man", "men"),
    w("human", "humans"),
    exact("louse", "lice"),
    w("mouse", "mice"),
    w("tooth", "teeth"),
    w("woman", "women"),
    exact("die", "dice"),
    w("person", "people"),
    alt("adieu", "adieux", "adieus"),
    w("fabliau", "fabliaux"),
    alt("bureau", "bureaus", "bureaux"),
    w("criterion", "criteria"),
    alt("ganglion", "ganglia", "ganglions"),
    alt("lexicon", "lexica", "lexicons"),
    alt("mitochondrion", "mitochondria", "mitochondrions"),
    w("noumenon", "noumena"),
    w("phenomenon", "phenomena"),
    w("taxon", "taxa"),
    w("media", "media"),
    Word {
        alternative: Some("mediums"),
        ..uni("medium", "media")
    },
    alt("stadium", "stadiums", "stadia"),
    alt("aquarium", "aquaria", "aquariums"),
    alt("auditorium", "auditoria", "auditoriums"),
    alt("symposium", "symposia", "symposiums"),
    alt("curriculum", "curriculums", "curricula"),
    w("quota", "quotas"),
    alt("alumnus", "alumni", "alumnuses"),
    w("bacillus", "bacilli"),
    alt("cactus", "cacti", "cactuses"),
    w("coccus", "cocci"),
    alt("focus", "foci", "focuses"),
    alt("locus", "loci", "locuses"),
    alt("nucleus", "nuclei", "nucleuses"),
    alt("octopus", "octupuses", "octopi"),
    alt("radius", "radii", "radiuses"),
    w("syllabus", "syllabi"),
    alt("corpus", "corpora", "corpuses"),
    w("genus", "genera"),
    w("alumna", "alumnae"),
    w("vertebra", "vertebrae"),
    w("differentia", "differentiae"),
    w("minutia", "minutiae"),
    w("vita", "vitae"),
    w("larva", "larvae"),
    w("postcava", "postcavae"),
    w("praecava", "praecavae"),
    w("uva", "uvae"),
    alt("apex", "apices", "apexes"),
    alt("codex", "codices", "codexes"),
    alt("index", "indices", "indexes"),
    alt("latex", "latices", "latexes"),
    alt("vertex", "vertices", "vertexes"),
    alt("vortex", "vortices", "vortexes"),
    alt("appendix", "appendices", "appendixes"),
    alt("radix", "radices", "radixes"),
    alt("helix", "helices", "helixes"),
    exact("axis", "axes"),
    w("crisis", "crises"),
    uni("ellipsis", "ellipses"),
    w("genesis", "geneses"),
    w("oasis", "oases"),
    w("thesis", "theses"),
    w("testis", "testes"),
    w("base", "bases"),
    uni("basis", "bases"),
    exact("alias", "aliases"),
    w("vedalia", "vedalias"),
    exact("use", "uses"),
    w("abuse", "abuses"),
    w("cause", "causes"),
    w("clause", "clauses"),
    w("cruse", "cruses"),
    w("excuse", "excuses"),
    w("fuse", "fuses"),
    w("house", "houses"),
    w("misuse", "misuses"),
    w("muse", "muses"),
    w("pause", "pauses"),
    w("ache", "aches"),
    w("topaz", "topazes"),
    alt("buffalo", "buffaloes", "buffalos"),
    w("potato", "potatoes"),
    w("tomato", "tomatoes"),
    unc("equipment"),
    unc("information"),
    unc("jeans"),
    unc("money"),
    unc("news"),
    unc("rice"),
    alt("dwarf", "dwarfs", "dwarves"),
    alt("hoof", "hoofs", "hooves"),
    w("thief", "thieves"),
    w("chive", "chives"),
    w("hive", "hives"),
    w("move", "moves"),
    w("movie", "movies"),
    w("cookie", "cookies"),
    w("pretorium", "pretoriums"),
    w("agenda", "agendas"),
    alt("formula", "formulas", "formulae"),
    w("shoe", "shoes"),
    exact("toe", "toes"),
    w("graffiti", "graffiti"),
    exact("ID", "IDs"),
];

/// Suffix rules (singular suffix, plural suffix); earlier entries take priority.
const SUFFIXES: &[(&str, &str)] = &[
    ("tive", "tives"),
    ("eaf", "eaves"),
    ("oaf", "oaves"),
    ("afe", "aves"),
    ("arf", "arves"),
    ("rfe", "rves"),
    ("rf", "rves"),
    ("lf", "lves"),
    ("fe", "ves"),
    ("ay", "ays"),
    ("ey", "eys"),
    ("oy", "oys"),
    ("quy", "quies"),
    ("uy", "uys"),
    ("y", "ies"),
    ("eau", "eaux"),
    ("bula", "bulae"),
    ("dula", "bulae"),
    ("lula", "bulae"),
    ("nula", "bulae"),
    ("vula", "bulae"),
    ("hedron", "hedra"),
    ("ium", "ia"),
    ("seum", "seums"),
    ("eum", "ea"),
    ("oum", "oa"),
    ("stracum", "straca"),
    ("dum", "da"),
    ("elum", "ela"),
    ("ilum", "ila"),
    ("olum", "ola"),
    ("ulum", "ula"),
    ("llum", "lla"),
    ("ylum", "yla"),
    ("imum", "ima"),
    ("ernum", "erna"),
    ("gnum", "gna"),
    ("brum", "bra"),
    ("crum", "cra"),
    ("terum", "tera"),
    ("serum", "sera"),
    ("trum", "tra"),
    ("antum", "anta"),
    ("atum", "ata"),
    ("entum", "enta"),
    ("etum", "eta"),
    ("itum", "ita"),
    ("otum", "ota"),
    ("utum", "uta"),
    ("ctum", "cta"),
    ("ovum", "ova"),
    ("trix", "trices"),
    ("iasis", "iases"),
    ("mesis", "meses"),
    ("kinesis", "kineses"),
    ("resis", "reses"),
    ("gnosis", "gnoses"),
    ("opsis", "opses"),
    ("ysis", "yses"),
    ("ouse", "ouses"),
    ("lause", "lauses"),
    ("us", "uses"),
    ("ch", "ches"),
    ("io", "ios"),
    ("sh", "shes"),
    ("ss", "sses"),
    ("ez", "ezzes"),
    ("iz", "izzes"),
    ("tz", "tzes"),
    ("zz", "zzes"),
    ("ano", "anos"),
    ("lo", "los"),
    ("to", "tos"),
    ("oo", "oos"),
    ("o", "oes"),
    ("x", "xes"),
    ("S", "Ses"),
];

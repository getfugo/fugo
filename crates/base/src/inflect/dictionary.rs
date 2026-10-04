//! The dictionary of the inflector: acronyms, words with their plural and singular forms,
//! uncountable words and suffix rules.

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Regular,
    /// The plural cannot be turned back into this singular.
    Unidirectional,
    /// Uncountable: the plural is the singular.
    Uncountable,
    /// Only the whole word, never as a suffix rule.
    Exact,
}

pub(super) struct Word {
    pub(super) singular: &'static str,
    pub(super) plural: &'static str,
    pub(super) alternative: Option<&'static str>,
    pub(super) kind: Kind,
}

impl Word {
    pub(super) fn plural_or_self(&self) -> &'static str {
        if self.kind == Kind::Uncountable && self.plural.is_empty() {
            self.singular
        } else {
            self.plural
        }
    }
}

pub(super) const fn w(singular: &'static str, plural: &'static str) -> Word {
    Word {
        singular,
        plural,
        alternative: None,
        kind: Kind::Regular,
    }
}

pub(super) const fn alt(
    singular: &'static str,
    plural: &'static str,
    alternative: &'static str,
) -> Word {
    Word {
        alternative: Some(alternative),
        ..w(singular, plural)
    }
}

pub(super) const fn uni(singular: &'static str, plural: &'static str) -> Word {
    Word {
        kind: Kind::Unidirectional,
        ..w(singular, plural)
    }
}

pub(super) const fn exact(singular: &'static str, plural: &'static str) -> Word {
    Word {
        kind: Kind::Exact,
        ..w(singular, plural)
    }
}

pub(super) const fn unc(singular: &'static str) -> Word {
    Word {
        kind: Kind::Uncountable,
        ..w(singular, "")
    }
}

/// Acronyms kept upper case. Lookups upper-case the word first, so the mixed-case entries
/// (`gbps`, `WiFi`, …) never match; they are kept as in flect.
pub(super) const ACRONYMS: &[&str] = &[
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
pub(super) const DICTIONARY: &[Word] = &[
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
pub(super) const SUFFIXES: &[(&str, &str)] = &[
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

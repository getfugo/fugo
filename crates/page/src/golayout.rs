//! Go time layouts (`2006-01-02`, `Jan 2`, `15:04 MST`): the date syntax of permalink
//! attributes (`:2006`) and of date-valued related-content indices. A layout is translated
//! once into `strftime` formats, zone abbreviations and UTC offsets.

use jiff::Zoned;

/// A Go time layout, translated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoLayout {
    chunks: Vec<Chunk>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Chunk {
    Strftime(String),
    /// Go's `MST`: the abbreviation of a named zone (`UTC`, `ICT`, `+07`), the offset
    /// (`-0700`) of a fixed-offset date.
    Zone,
    /// A numeric UTC offset (`-07:00`, `Z0700`, …).
    Offset(Offset),
}

/// How a numeric UTC offset is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Offset {
    /// `Z…` elements write `Z` for UTC.
    z: bool,
    /// Hours only (`-07`), hours and minutes (`-0700`), or with seconds (`-070000`).
    precision: Precision,
    colons: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Precision {
    Hours,
    Minutes,
    Seconds,
}

/// What a layout element becomes.
#[derive(Clone, Copy)]
enum Element {
    Strftime(&'static str),
    Zone,
    Offset(Offset),
}

const fn offset(z: bool, precision: Precision, colons: bool) -> Element {
    Element::Offset(Offset {
        z,
        precision,
        colons,
    })
}

/// (Go element, translation), longest first where one is a prefix of another.
const ELEMENTS: &[(&str, Element)] = &[
    ("January", Element::Strftime("%B")),
    ("Jan", Element::Strftime("%b")),
    ("Monday", Element::Strftime("%A")),
    ("Mon", Element::Strftime("%a")),
    ("MST", Element::Zone),
    ("2006", Element::Strftime("%Y")),
    ("002", Element::Strftime("%j")),
    ("Z07:00:00", offset(true, Precision::Seconds, true)),
    ("-07:00:00", offset(false, Precision::Seconds, true)),
    ("Z070000", offset(true, Precision::Seconds, false)),
    ("-070000", offset(false, Precision::Seconds, false)),
    ("Z07:00", offset(true, Precision::Minutes, true)),
    ("-07:00", offset(false, Precision::Minutes, true)),
    ("Z0700", offset(true, Precision::Minutes, false)),
    ("-0700", offset(false, Precision::Minutes, false)),
    ("Z07", offset(true, Precision::Hours, false)),
    ("-07", offset(false, Precision::Hours, false)),
    ("01", Element::Strftime("%m")),
    ("02", Element::Strftime("%d")),
    ("03", Element::Strftime("%I")),
    ("04", Element::Strftime("%M")),
    ("05", Element::Strftime("%S")),
    ("06", Element::Strftime("%y")),
    ("15", Element::Strftime("%H")),
    ("__2", Element::Strftime("%_j")),
    ("_2", Element::Strftime("%e")),
    ("1", Element::Strftime("%-m")),
    ("2", Element::Strftime("%-d")),
    ("3", Element::Strftime("%-I")),
    ("4", Element::Strftime("%-M")),
    ("5", Element::Strftime("%-S")),
    ("PM", Element::Strftime("%p")),
    ("pm", Element::Strftime("%P")),
];

impl GoLayout {
    /// Translates `layout`; `None` when it holds no layout element (it would format as
    /// itself). Other text is kept.
    #[must_use]
    pub fn parse(layout: &str) -> Option<Self> {
        let mut chunks = Vec::new();
        let mut out = String::new();
        let mut found = false;
        let mut rest = layout;
        'outer: while !rest.is_empty() {
            for (go, element) in ELEMENTS {
                let Some(after) = rest.strip_prefix(go) else {
                    continue;
                };
                // Go reads `Mon`/`Jan` only when no lower-case letter follows, and `_2006` as
                // `_` and the year.
                if (*go == "Jan" || *go == "Mon")
                    && after.starts_with(|c: char| c.is_ascii_lowercase())
                {
                    continue;
                }
                if *go == "_2" && after.starts_with("006") {
                    continue;
                }
                match element {
                    Element::Strftime(f) => out.push_str(f),
                    Element::Zone | Element::Offset(_) => {
                        if !out.is_empty() {
                            chunks.push(Chunk::Strftime(std::mem::take(&mut out)));
                        }
                        chunks.push(match element {
                            Element::Offset(o) => Chunk::Offset(*o),
                            _ => Chunk::Zone,
                        });
                    }
                }
                found = true;
                rest = after;
                continue 'outer;
            }
            let c = rest.chars().next().expect("not empty");
            if c == '%' {
                out.push_str("%%");
            } else {
                out.push(c);
            }
            rest = &rest[c.len_utf8()..];
        }
        if !out.is_empty() {
            chunks.push(Chunk::Strftime(out));
        }
        found.then_some(Self { chunks })
    }

    /// `date` in this layout.
    #[must_use]
    pub fn format(&self, date: &Zoned) -> String {
        self.chunks
            .iter()
            .map(|c| match c {
                Chunk::Strftime(f) => date.strftime(f.as_str()).to_string(),
                Chunk::Zone => {
                    let named = date.time_zone().iana_name().is_some();
                    let abbr = date.strftime("%Z").to_string();
                    if named && !abbr.is_empty() {
                        abbr
                    } else {
                        date.strftime("%z").to_string()
                    }
                }
                Chunk::Offset(o) => o.format(date.offset().seconds()),
            })
            .collect()
    }
}

impl Offset {
    fn format(self, secs: i32) -> String {
        if self.z && secs == 0 {
            return "Z".to_owned();
        }
        let sign = if secs < 0 { '-' } else { '+' };
        let abs = secs.unsigned_abs();
        let (h, m, s) = (abs / 3600, abs % 3600 / 60, abs % 60);
        let sep = if self.colons { ":" } else { "" };
        match self.precision {
            Precision::Hours => format!("{sign}{h:02}"),
            Precision::Minutes => format!("{sign}{h:02}{sep}{m:02}"),
            Precision::Seconds => format!("{sign}{h:02}{sep}{m:02}{sep}{s:02}"),
        }
    }
}

/// `date` formatted with the Go layout `layout`; a layout without elements is itself.
#[must_use]
pub fn format_go_layout(date: &Zoned, layout: &str) -> String {
    GoLayout::parse(layout).map_or_else(|| layout.to_owned(), |l| l.format(date))
}

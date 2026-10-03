//! A page's four dates, from front matter keys, the file name, the file's modification time
//! or Git (`[frontmatter]` chains).

use jiff::tz::{Offset, TimeZone};
use jiff::{Timestamp, Zoned};
use ssg_base::{Date, Params, Value};
use ssg_config::{DateField, DateSource, SiteConfig};

/// The dates of a page; `None` is Go's zero date.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dates {
    pub date: Option<Zoned>,
    pub lastmod: Option<Zoned>,
    pub publish_date: Option<Zoned>,
    pub expiry_date: Option<Zoned>,
}

impl Dates {
    /// The date of `field`.
    #[must_use]
    pub fn get(&self, field: DateField) -> Option<&Zoned> {
        match field {
            DateField::Date => self.date.as_ref(),
            DateField::Lastmod => self.lastmod.as_ref(),
            DateField::PublishDate => self.publish_date.as_ref(),
            DateField::ExpiryDate => self.expiry_date.as_ref(),
        }
    }

    pub(crate) fn set(&mut self, field: DateField, z: Zoned) {
        let slot = match field {
            DateField::Date => &mut self.date,
            DateField::Lastmod => &mut self.lastmod,
            DateField::PublishDate => &mut self.publish_date,
            DateField::ExpiryDate => &mut self.expiry_date,
        };
        *slot = Some(z);
    }

    /// Whether every date is unset (Go's `IsAllDatesZero`: such nodes take their dates from
    /// their descendants).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        DateField::ALL.iter().all(|&f| self.get(f).is_none())
    }
}

/// What the file-based date sources read.
#[derive(Clone, Copy, Debug, Default)]
pub struct FileCtx<'a> {
    /// The file name without extension, or the bundle directory's name for a bundle index
    /// (`2024-01-02-my-post`); `:filename` reads it.
    pub base_filename: &'a str,
    /// `:fileModTime`.
    pub mod_time: Option<Timestamp>,
    /// `:git` (the last commit's author date).
    pub git_author_date: Option<Timestamp>,
}

/// The result of [`DateResolver::resolve`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DateOutcome {
    pub dates: Dates,
    /// The slug `:filename` found after the date (`2024-01-02-my-post` → `my-post`), when the
    /// front matter has no `slug`.
    pub slug: Option<String>,
    /// Front matter keys whose values are not dates (skipped; the caller reports them).
    pub unparsable: Vec<String>,
}

/// The configured source chains of the four dates (`[frontmatter]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateResolver {
    chains: Vec<(DateField, Vec<DateSource>)>,
}

impl DateResolver {
    /// The resolver for `chains` (one per field; a field without a chain is never set).
    #[must_use]
    pub fn new(chains: &[(DateField, Vec<DateSource>)]) -> Self {
        let chains = DateField::ALL
            .into_iter()
            .map(|f| {
                let sources = chains
                    .iter()
                    .find(|(g, _)| *g == f)
                    .map(|(_, s)| s.clone())
                    .unwrap_or_default();
                (f, sources)
            })
            .collect();
        Self { chains }
    }

    /// The source chain of each date field, in the order date, lastmod, publishDate,
    /// expiryDate.
    pub(crate) fn chains(&self) -> &[(DateField, Vec<DateSource>)] {
        &self.chains
    }

    /// The resolver of a site's `[frontmatter]` configuration.
    #[must_use]
    pub fn from_site(site: &SiteConfig) -> Self {
        Self::new(&site.front_matter)
    }

    /// Whether `key` is one of the configured front matter date keys.
    #[must_use]
    pub fn is_date_key(&self, key: &str) -> bool {
        self.chains
            .iter()
            .flat_map(|(_, s)| s)
            .any(|s| matches!(s, DateSource::Field(k) if k == key))
    }

    /// Resolves the dates of a page, in the order date, lastmod, publishDate, expiryDate; for
    /// each the first source that yields a date wins. Every date found is written back to the
    /// params as a date: under the key it was read from, and under the field's own key
    /// (`date`, `lastmod`, `publishdate`, `expirydate`) unless that is already set. Values
    /// without a zone are placed in `tz`; zoned values keep their offset (to the second).
    #[must_use]
    pub fn resolve(
        &self,
        p: &mut Params,
        file: Option<&FileCtx<'_>>,
        tz: &TimeZone,
    ) -> DateOutcome {
        let mut out = DateOutcome::default();
        for (field, sources) in &self.chains {
            for source in sources {
                let found = match source {
                    DateSource::Field(key) => match p.get(key) {
                        None | Some(Value::Null) => None,
                        Some(Value::String(s)) if s.is_empty() => None,
                        Some(v) => match to_date(v, tz) {
                            Some(z) => {
                                p.insert(key, &Value::Date(Date::Zoned(z.clone())));
                                Some(z)
                            }
                            None => {
                                out.unparsable.push(key.clone());
                                None
                            }
                        },
                    },
                    DateSource::Filename => file.and_then(|f| {
                        let (z, slug) = date_and_slug(f.base_filename, tz)?;
                        if p.get("slug").is_none() {
                            out.slug = Some(slug);
                        }
                        Some(z)
                    }),
                    DateSource::FileModTime => file.and_then(|f| f.mod_time).map(utc),
                    DateSource::Git => file.and_then(|f| f.git_author_date).map(utc),
                };
                if let Some(z) = found {
                    let key = field.key();
                    if p.get(key).is_none() {
                        p.insert(key, &Value::Date(Date::Zoned(z.clone())));
                    }
                    out.dates.set(*field, z);
                    break;
                }
            }
        }
        out.unparsable.dedup();
        out
    }
}

fn utc(t: Timestamp) -> Zoned {
    t.to_zoned(TimeZone::UTC)
}

/// A front matter value as a date: date strings in Go's layouts, TOML dates, Unix seconds.
pub(crate) fn to_date(v: &Value, tz: &TimeZone) -> Option<Zoned> {
    match v {
        Value::String(s) => ssg_base::parse_date(s, tz).ok(),
        Value::Date(Date::Local(dt)) => dt.to_zoned(tz.clone()).ok(),
        Value::Date(Date::Zoned(z)) if same_zone(z.time_zone(), tz) => Some(z.clone()),
        Value::Date(Date::Zoned(z)) => Some(to_second_fixed(z)),
        Value::Int(n) => Timestamp::from_second(*n).ok().map(utc),
        _ => None,
    }
}

/// Whether a date is already in the page's (named) time zone: dates resolved earlier, and
/// UTC dates of a UTC site, are used as they are.
fn same_zone(a: &TimeZone, b: &TimeZone) -> bool {
    a.iana_name().is_some() && a.iana_name() == b.iana_name()
}

/// A zoned date in another zone keeps its offset but not its zone rules or sub-seconds (Go
/// re-reads such dates from their RFC 3339 form); offset zero is UTC.
fn to_second_fixed(z: &Zoned) -> Zoned {
    let ts = Timestamp::from_second(z.timestamp().as_second()).unwrap_or(z.timestamp());
    let offset = z.offset();
    let tz = if offset == Offset::UTC {
        TimeZone::UTC
    } else {
        TimeZone::fixed(offset)
    };
    ts.to_zoned(tz)
}

/// `:filename`: a `YYYY-MM-DD` or `YYYY-MM-DD-HH-MM-SS` prefix (any one character between the
/// date and the time) and the slug after it, trimmed of ` `, `-` and `_`.
fn date_and_slug(base_filename: &str, tz: &TimeZone) -> Option<(Zoned, String)> {
    let name = ssg_base::paths::file_and_ext(base_filename).0;
    let trim = |s: &str| s.trim_matches([' ', '-', '_']).to_owned();
    if let (Some(date), Some(time), Some(rest)) = (name.get(..10), name.get(11..19), name.get(19..))
    {
        let s = format!("{date}T{}", time.replace('-', ":"));
        if let Ok(z) = ssg_base::parse_date(&s, tz) {
            return Some((z, trim(rest)));
        }
    }
    let (date, rest) = (name.get(..10)?, name.get(10..)?);
    let z = ssg_base::parse_date(date, tz).ok()?;
    Some((z, trim(rest)))
}

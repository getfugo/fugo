//! Strings: title case, trimming, `regex_find`, `substr`, padding, inflection, `urlize`,
//! `anchorize`, and the `version_at_least` test.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::{Arc, PoisonError, RwLock};

use ssg_base::url::SiteUrls;
use ssg_base::{anchor, inflect, title};
use tera::{Kwargs, TeraResult, Value};

use super::value::{chain, same_safety, text};
use super::{PureEnv, Registrar};

pub(super) fn register(r: &mut Registrar<'_>, env: &Arc<PureEnv>) {
    let default_title = env.title_style;
    r.filter("title_case", move |v, kw, _| {
        let style = kw
            .get::<&str>("style")?
            .map_or(default_title, title::Style::parse);
        Ok(Value::from(title::title_case(
            &text(&v, "title_case")?,
            style,
        )))
    });
    r.filter("trim_chars", |v, kw, _| {
        let chars: Vec<char> = kw.must_get::<&str>("chars")?.chars().collect();
        let s = text(&v, "trim_chars")?;
        Ok(same_safety(&v, s.trim_matches(chars.as_slice()).to_owned()))
    });
    r.filter("trim_start_chars", |v, kw, _| {
        let chars: Vec<char> = kw.must_get::<&str>("chars")?.chars().collect();
        let s = text(&v, "trim_start_chars")?;
        Ok(same_safety(
            &v,
            s.trim_start_matches(chars.as_slice()).to_owned(),
        ))
    });
    r.filter("trim_end_chars", |v, kw, _| {
        let chars: Vec<char> = kw.must_get::<&str>("chars")?.chars().collect();
        let s = text(&v, "trim_end_chars")?;
        Ok(same_safety(
            &v,
            s.trim_end_matches(chars.as_slice()).to_owned(),
        ))
    });
    r.filter("strip_prefix", |v, kw, _| {
        let prefix = kw.must_get::<&str>("prefix")?;
        let s = text(&v, "strip_prefix")?;
        Ok(same_safety(
            &v,
            s.strip_prefix(prefix).unwrap_or(&s).to_owned(),
        ))
    });
    r.filter("strip_suffix", |v, kw, _| {
        let suffix = kw.must_get::<&str>("suffix")?;
        let s = text(&v, "strip_suffix")?;
        Ok(same_safety(
            &v,
            s.strip_suffix(suffix).unwrap_or(&s).to_owned(),
        ))
    });
    let cache = RegexCache::default();
    r.filter("regex_find", move |v, kw, _| {
        let re = cache.get(kw.must_get::<&str>("pattern")?)?;
        let limit = kw.get::<i64>("limit")?.unwrap_or(-1);
        let s = text(&v, "regex_find")?;
        let found = re.find_iter(&s).map(|m| Value::from(m.as_str()));
        let found: Vec<Value> = match usize::try_from(limit) {
            Ok(n) => found.take(n).collect(),
            Err(_) => found.collect(),
        };
        Ok(Value::from(found))
    });
    r.filter("substr", |v, kw, _| {
        let start = kw.must_get::<i64>("start")?;
        let length = kw.get::<i64>("length")?;
        let s = text(&v, "substr")?;
        Ok(same_safety(&v, substr(&s, start, length)))
    });
    r.filter("pad_start", |v, kw, _| pad(&v, kw, Side::Start));
    r.filter("pad_end", |v, kw, _| pad(&v, kw, Side::End));
    r.filter("pluralize_word", |v, _, _| {
        Ok(Value::from(inflect::pluralize(&text(
            &v,
            "pluralize_word",
        )?)))
    });
    r.filter("singularize_word", |v, _, _| {
        Ok(Value::from(inflect::singularize(&text(
            &v,
            "singularize_word",
        )?)))
    });
    r.filter("humanize", |v, _, _| {
        if let Some(n) = v.as_i64() {
            return Ok(Value::from(inflect::ordinalize(n)));
        }
        Ok(Value::from(inflect::humanize_text(&text(&v, "humanize")?)))
    });
    r.filter("ordinalize", |v, _, _| {
        if let Some(n) = v.as_i64() {
            return Ok(Value::from(inflect::ordinalize(n)));
        }
        Ok(Value::from(inflect::ordinalize_str(&text(
            &v,
            "ordinalize",
        )?)))
    });
    let urls = SiteUrls {
        path_case: env.path_case,
        accents: env.accents,
        ..SiteUrls::default()
    };
    r.filter("urlize", move |v, _, _| {
        Ok(Value::from(urls.urlize(&text(&v, "urlize")?)))
    });
    let default_anchor = env.anchor_style;
    r.filter("anchorize", move |v, kw, _| {
        let style = match kw.get::<&str>("style")? {
            None => default_anchor,
            Some(s) => anchor_style(s)?,
        };
        Ok(Value::from(anchor::anchorize(
            &text(&v, "anchorize")?,
            style,
        )))
    });
    r.test("version_at_least", |v, kw, _| {
        let want = kw.must_get::<&str>("version")?;
        let have = text(&v, "version_at_least")?;
        Ok(Version::parse(&have)? >= Version::parse(want)?)
    });
}

fn anchor_style(s: &str) -> TeraResult<anchor::Style> {
    match s.to_ascii_lowercase().as_str() {
        "github" => Ok(anchor::Style::Github),
        "github-ascii" => Ok(anchor::Style::GithubAscii),
        "blackfriday" => Ok(anchor::Style::Blackfriday),
        other => Err(tera::Error::message(format!(
            "anchorize(style=): unknown style `{other}`; expected github, github-ascii or blackfriday"
        ))),
    }
}

/// Go's `substr`: `length` characters from `start`; a negative `start` counts from the end, a
/// negative `length` stops that many characters before the end.
pub(super) fn substr(s: &str, start: i64, length: Option<i64>) -> String {
    let chars: Vec<char> = s.chars().collect();
    let len = i64::try_from(chars.len()).unwrap_or(i64::MAX);
    if len == 0 {
        return String::new();
    }
    let mut start = if start < 0 { start + len } else { start };
    start = start.max(0);
    if start > len - 1 {
        return String::new();
    }
    let end = match length {
        None => len,
        Some(0) => return String::new(),
        Some(l) if l < 0 => len + l,
        Some(l) => start.saturating_add(l),
    };
    if start >= end || end < 0 {
        return String::new();
    }
    let end = end.min(len);
    let (start, end) = (
        usize::try_from(start).unwrap_or(0),
        usize::try_from(end).unwrap_or(0),
    );
    chars[start..end].iter().collect()
}

#[derive(Clone, Copy)]
enum Side {
    Start,
    End,
}

fn pad(v: &Value, kw: &Kwargs, side: Side) -> TeraResult<Value> {
    let width = kw.must_get::<i64>("width")?;
    let s = text(v, "pad_start/pad_end")?;
    let have = s.chars().count();
    let need = usize::try_from(width).unwrap_or(0).saturating_sub(have);
    let fill = " ".repeat(need);
    let out = match side {
        Side::Start => format!("{fill}{s}"),
        Side::End => format!("{s}{fill}"),
    };
    Ok(same_safety(v, out))
}

/// Compiled patterns, per filter instance.
#[derive(Default)]
struct RegexCache(RwLock<HashMap<String, regex::Regex>>);

impl RegexCache {
    fn get(&self, pattern: &str) -> TeraResult<regex::Regex> {
        if let Some(re) = self
            .0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(pattern)
        {
            return Ok(re.clone());
        }
        let re = regex::Regex::new(pattern)
            .map_err(|e| chain(format!("invalid regex `{pattern}`"), e))?;
        self.0
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(pattern.to_owned(), re.clone());
        Ok(re)
    }
}

/// A version such as `build.version`: `major.minor.patch`, where a `-DEV` (any pre-release) build
/// ranks below its release.
#[derive(PartialEq, Eq)]
struct Version {
    numbers: [u64; 3],
    release: bool,
}

impl Version {
    fn parse(s: &str) -> TeraResult<Self> {
        let s = s.trim().trim_start_matches('v');
        let (core, pre) = s.split_once('-').map_or((s, None), |(c, p)| (c, Some(p)));
        let mut numbers = [0; 3];
        for (slot, part) in numbers.iter_mut().zip(core.split('.')) {
            *slot = part
                .parse()
                .map_err(|_| tera::Error::message(format!("`{s}` is not a version")))?;
        }
        if core.split('.').count() > 3 {
            return Err(tera::Error::message(format!("`{s}` is not a version")));
        }
        Ok(Self {
            numbers,
            release: pre.is_none(),
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.numbers, self.release).cmp(&(other.numbers, other.release))
    }
}

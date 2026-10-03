//! Shared helpers of the oracle tests: fixture access, Go values, a [`NavModel`] over the page
//! dumps of the fixtures, configurations written to temporary projects, and pass-rate tallies.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use jiff::Zoned;
use jiff::tz::{Offset, TimeZone};
use serde_json::Value as J;
use ssg_base::url::{Accents, BaseUrl, LinkStyle, PathCase, SiteUrls};
use ssg_base::{FormatId, Idx, LangIdx, Map, MediaTypeId, PageId, PageKind, Params, Value};
use ssg_config::output::{Escaping, LinkPolicy, Listing, Placement, UglyPolicy};
use ssg_config::{Config, LoadOptions, OutputFormat, SitemapConfig};
use ssg_nav::{NavModel, PageFacts, Rendering};
use ssg_page::{
    Cjk, DateResolver, Dates, ListMode, MetaCtx, PageMenuEntry, PathShape, SourcePath, TargetPaths,
    meta_from_params,
};
use ssg_testkit::fixture::{Tag, oracle, repo_dir, repo_file};

/// A fixture under `testdata/oracle/`.
pub fn fixture(rel: &str) -> J {
    oracle(&format!("oracle/{rel}"))
}

/// The files of a fixture family (`page/menus`), without the ones in `skip`.
pub fn family(dir: &str, skip: &[&str]) -> Vec<String> {
    let path = ssg_testkit::fixture::testdata(&format!("oracle/{dir}"));
    let mut names: Vec<String> = fs::read_dir(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .map(|e| {
            e.expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|n| n.ends_with(".json.gz") && !skip.contains(&n.as_str()))
        .collect();
    names.sort();
    names
}

pub fn s(v: &J) -> &str {
    v.as_str().unwrap_or_else(|| panic!("not a string: {v}"))
}

pub fn idx(v: &J) -> usize {
    usize::try_from(v.as_u64().unwrap_or_else(|| panic!("not an index: {v}"))).expect("index")
}

pub fn page_id(i: usize) -> PageId {
    PageId::from_index(i)
}

pub fn strings(v: &J) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(|x| s(x).to_owned()).collect())
        .unwrap_or_default()
}

fn time_zone(name: &str) -> TimeZone {
    if name == "UTC" || name.is_empty() {
        TimeZone::UTC
    } else {
        TimeZone::get(name).unwrap_or_else(|e| panic!("time zone {name}: {e}"))
    }
}

/// Parses an oracle time (`2020-02-03T00:00:00+07:00[Asia/Bangkok]`, `…Z`, `…+07:00`).
pub fn parse_time(t: &str) -> Zoned {
    if let Some((stamp, zone)) = t.strip_suffix(']').and_then(|t| t.split_once('[')) {
        let ts: jiff::Timestamp = stamp.parse().unwrap_or_else(|e| panic!("{t}: {e}"));
        return ts.to_zoned(time_zone(zone));
    }
    let ts: jiff::Timestamp = t.parse().unwrap_or_else(|e| panic!("{t}: {e}"));
    if t.ends_with('Z') {
        return ts.to_zoned(TimeZone::UTC);
    }
    let off = &t[t.len() - 6..];
    let sign = if off.starts_with('-') { -1 } else { 1 };
    let h: i32 = off[1..3].parse().expect("hours");
    let m: i32 = off[4..6].parse().expect("minutes");
    let offset = Offset::from_seconds(sign * (h * 3600 + m * 60)).expect("offset");
    ts.to_zoned(TimeZone::fixed(offset))
}

/// An oracle time, `None` for Go's zero time.
pub fn zoned(v: &J) -> Option<Zoned> {
    let Some(Tag::Time(t)) = Tag::of(v) else {
        panic!("not a time: {v}");
    };
    (t != "0001-01-01T00:00:00Z").then(|| parse_time(t))
}

/// A plain JSON value (with `$nh:` tags) as a front matter value.
pub fn value(v: &J) -> Value {
    if let Some(tag) = Tag::of(v) {
        return match tag {
            Tag::Time(t) => Value::Date(ssg_base::Date::Zoned(parse_time(t))),
            Tag::Float(f) => Value::Float(f),
            other => panic!("unexpected tag {other:?}"),
        };
    }
    match v {
        J::Null => Value::Null,
        J::Bool(b) => Value::Bool(*b),
        J::Number(n) => n
            .as_i64()
            .map_or_else(|| Value::Float(n.as_f64().expect("number")), Value::Int),
        J::String(s) => Value::string(s),
        J::Array(a) => Value::Array(Arc::new(a.iter().map(value).collect())),
        J::Object(o) => Value::map(
            o.iter()
                .map(|(k, v)| (k.as_str(), value(v)))
                .collect::<Map>(),
        ),
    }
}

/// The URL helpers of an oracle `PathSpec`.
pub fn site_urls(ps: &J) -> SiteUrls {
    let flag = |k: &str| ps[k].as_bool().unwrap_or_else(|| panic!("{k}: {ps}"));
    SiteUrls {
        base_url: BaseUrl::parse(s(&ps["baseURL"])).expect("base URL"),
        language_prefix: s(&ps["languagePrefix"]).to_owned(),
        link_style: if flag("canonifyURLs") {
            LinkStyle::Canonify
        } else {
            LinkStyle::Relative
        },
        path_case: if flag("disablePathToLower") {
            PathCase::Preserve
        } else {
            PathCase::Lower
        },
        accents: if flag("removePathAccents") {
            Accents::Remove
        } else {
            Accents::Keep
        },
    }
}

/// An oracle output format, with its media type's suffix (`.html`).
pub fn output_format(f: &J) -> (OutputFormat, String) {
    let flag = |k: &str| f[k].as_bool().unwrap_or_else(|| panic!("{k}: {f}"));
    let ugly = if flag("noUgly") {
        UglyPolicy::Never
    } else if flag("ugly") {
        UglyPolicy::Always
    } else {
        UglyPolicy::Inherit
    };
    let format = OutputFormat {
        name: s(&f["name"]).to_owned(),
        media_type: MediaTypeId::from_index(0),
        base_name: s(&f["baseName"]).to_owned(),
        path: s(&f["path"]).to_owned(),
        rel: s(&f["rel"]).to_owned(),
        protocol: s(&f["protocol"]).to_owned(),
        escaping: if flag("isPlainText") {
            Escaping::Plain
        } else {
            Escaping::Html
        },
        is_html: flag("isHTML"),
        ugly,
        links: if flag("permalinkable") {
            LinkPolicy::Own
        } else {
            LinkPolicy::UsePrimary
        },
        listing: if flag("notAlternative") {
            Listing::NotAlternative
        } else {
            Listing::Alternative
        },
        placement: if flag("root") {
            Placement::Root
        } else {
            Placement::LanguageDir
        },
        weight: i32::try_from(f["weight"].as_i64().expect("weight")).expect("weight"),
    };
    (format, s(&f["mediaType"]["fullSuffix"]).to_owned())
}

/// A recorded Go path (`paths` of the fixtures): the source path and `Base()`.
pub struct OraclePath {
    pub source: SourcePath,
    pub base: String,
}

pub fn oracle_paths(entries: &J) -> Vec<OraclePath> {
    entries
        .as_array()
        .expect("paths")
        .iter()
        .map(|e| {
            let c = &e["checks"];
            OraclePath {
                source: SourcePath {
                    dir: s(&c["containerDir"]).to_owned(),
                    name: s(&c["baseNameNoIdentifier"]).to_owned(),
                    shape: if c["isBundle"].as_bool().expect("isBundle") {
                        PathShape::Bundle
                    } else {
                        PathShape::File
                    },
                },
                base: s(&c["base"]).to_owned(),
            }
        })
        .collect()
}

/// The front matter menus of a page's params, decoded the way the page model does.
///
/// # Errors
/// What `ssg-page` rejects.
pub fn page_menus(params: &Params) -> Result<Vec<PageMenuEntry>, String> {
    let mut only = Map::new();
    for key in ["menus", "menu"] {
        if let Some(v) = params.get(key) {
            only.insert(key, v.clone());
        }
    }
    if only.is_empty() {
        return Ok(Vec::new());
    }
    let types = ssg_config::MediaTypes::default();
    let formats = ssg_config::OutputFormats::builtin(&types);
    let resolver = DateResolver::new(&[]);
    let ctx = MetaCtx {
        kind: PageKind::Page,
        formats: &formats,
        media_types: &types,
        sitemap: &SitemapConfig::default(),
        cjk_default: Cjk::No,
        ext: "md",
        dates: &resolver,
        file: None,
        time_zone: &TimeZone::UTC,
    };
    meta_from_params(Params::fold(&only), &ctx)
        .map(|m| m.menus)
        .map_err(|e| e.to_string())
}

/// A page of a fixture's page dump.
pub struct DumpPage {
    pub lang: LangIdx,
    pub kind: PageKind,
    pub lang_key: String,
    pub path: String,
    pub section: String,
    pub title: String,
    pub link_title: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub page_type: String,
    pub layout: String,
    pub bundle_type: String,
    pub draft: bool,
    pub weight: i32,
    pub keywords: Vec<String>,
    pub aliases: Vec<String>,
    pub dates: Dates,
    pub params: Params,
    pub menus: Vec<PageMenuEntry>,
    pub rel_permalink: String,
    pub fragments: Vec<String>,
    pub headings: Vec<String>,
    pub ancestor_of: Vec<usize>,
    pub outputs: Vec<(FormatId, TargetPaths)>,
    pub aliases_rendered: Rendering,
}

/// A [`NavModel`] over the `pages` dump of a fixture. `pageRef`s resolve by path in the same
/// language (the real `get_page` is T23b's).
pub struct DumpSite {
    pub pages: Vec<DumpPage>,
    pub langs: usize,
}

impl DumpSite {
    pub fn new(fx: &J) -> Self {
        let pages: Vec<DumpPage> = fx["pages"]
            .as_array()
            .expect("pages")
            .iter()
            .map(|p| {
                let params = p["params"]
                    .as_object()
                    .map(|_| Params::fold(value(&p["params"]).as_map().expect("map")))
                    .unwrap_or_default();
                DumpPage {
                    lang: LangIdx::from_index(idx(&p["site"])),
                    kind: PageKind::parse(s(&p["kind"])).expect("kind"),
                    lang_key: s(&p["lang"]).to_owned(),
                    path: s(&p["path"]).to_owned(),
                    section: s(&p["section"]).to_owned(),
                    title: s(&p["title"]).to_owned(),
                    link_title: s(&p["linkTitle"]).to_owned(),
                    name: s(&p["name"]).to_owned(),
                    slug: s(&p["slug"]).to_owned(),
                    description: s(&p["description"]).to_owned(),
                    page_type: s(&p["pageType"]).to_owned(),
                    layout: s(&p["layout"]).to_owned(),
                    bundle_type: s(&p["bundleType"]).to_owned(),
                    draft: p["draft"].as_bool().unwrap_or(false),
                    weight: i32::try_from(p["weight"].as_i64().unwrap_or(0)).expect("weight"),
                    keywords: strings(&p["keywords"]),
                    aliases: strings(&p["aliases"]),
                    dates: Dates {
                        date: zoned(&p["date"]),
                        lastmod: zoned(&p["lastmod"]),
                        publish_date: zoned(&p["publishDate"]),
                        expiry_date: zoned(&p["expiryDate"]),
                    },
                    menus: page_menus(&params).unwrap_or_default(),
                    params,
                    rel_permalink: s(&p["relPermalink"]).to_owned(),
                    fragments: strings(&p["fragments"]),
                    headings: strings(&p["headings"]),
                    ancestor_of: p["ancestorOf"]
                        .as_array()
                        .map(|a| a.iter().map(idx).collect())
                        .unwrap_or_default(),
                    outputs: Vec::new(),
                    aliases_rendered: Rendering::Rendered,
                }
            })
            .collect();
        let langs = fx["sites"].as_array().map_or(1, Vec::len);
        Self { pages, langs }
    }
}

impl NavModel for DumpSite {
    fn page(&self, id: PageId) -> PageFacts<'_> {
        let p = &self.pages[id.index()];
        PageFacts {
            id,
            lang: p.lang,
            kind: p.kind,
            lang_key: &p.lang_key,
            section: &p.section,
            title: &p.title,
            link_title: &p.link_title,
            name: &p.name,
            slug: &p.slug,
            description: &p.description,
            page_type: &p.page_type,
            layout: &p.layout,
            bundle_type: &p.bundle_type,
            draft: p.draft,
            weight: p.weight,
            keywords: &p.keywords,
            aliases: &p.aliases,
            dates: &p.dates,
            params: &p.params,
            menus: &p.menus,
            rel_permalink: &p.rel_permalink,
            fragments: &p.fragments,
            outputs: &p.outputs,
            rendering: p.aliases_rendered,
            list: ListMode::Always,
        }
    }

    fn tree_pages(&self, lang: LangIdx) -> Vec<PageId> {
        let mut ids: Vec<usize> = (0..self.pages.len())
            .filter(|&i| self.pages[i].lang == lang)
            .collect();
        let key = |i: &usize| self.pages[*i].path.trim_start_matches('/').to_owned();
        ids.sort_by_key(key);
        ids.into_iter().map(page_id).collect()
    }

    fn resolve_page_ref(&self, lang: LangIdx, reference: &str) -> Option<PageId> {
        let want = format!("/{}", reference.trim_matches('/')).to_lowercase();
        self.pages
            .iter()
            .position(|p| p.lang == lang && p.path.to_lowercase() == want)
            .map(page_id)
    }

    fn is_ancestor(&self, ancestor: PageId, page: PageId) -> bool {
        self.pages[ancestor.index()]
            .ancestor_of
            .contains(&page.index())
    }

    fn pages(&self, _: PageId) -> &[PageId] {
        &[]
    }

    fn regular_pages(&self, _: PageId) -> &[PageId] {
        &[]
    }

    fn site_regular_pages(&self, _: LangIdx) -> &[PageId] {
        &[]
    }

    fn home(&self, lang: LangIdx) -> Option<PageId> {
        self.pages
            .iter()
            .position(|p| p.lang == lang && p.kind == PageKind::Home)
            .map(page_id)
    }
}

/// A project directory with `config.json` = `config` and the given files.
pub struct Project {
    pub _tmp: tempfile::TempDir,
    pub cfg: Arc<Config>,
}

impl Project {
    /// # Errors
    /// The configuration error.
    pub fn json(config: &J) -> Result<Self, String> {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("site");
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join("config.json"), config.to_string()).expect("write");
        Self::try_at(tmp, &dir)
    }

    /// A recorded site (`config.toml` and its files).
    pub fn recorded(site: &J) -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join(s(&site["name"]));
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join("config.toml"), s(&site["toml"])).expect("write");
        for f in site["files"].as_array().expect("files") {
            let fp = dir.join(s(&f["path"]));
            fs::create_dir_all(fp.parent().expect("parent")).expect("mkdir");
            let content = match f["repo"].as_str() {
                Some(r) => fs::read(repo_file(r)).unwrap_or_else(|e| panic!("{r}: {e}")),
                None => s(&f["content"]).as_bytes().to_vec(),
            };
            fs::write(fp, content).expect("write");
        }
        Self::at(tmp, &dir)
    }

    fn at(tmp: tempfile::TempDir, dir: &Path) -> Self {
        Self::try_at(tmp, dir).unwrap_or_else(|e| panic!("{e}"))
    }

    fn try_at(tmp: tempfile::TempDir, dir: &Path) -> Result<Self, String> {
        let home = tmp.path().join("home");
        let cfg = ssg_config::load(&LoadOptions {
            source: dir.to_path_buf(),
            env: vec![("HOME".into(), home.to_string_lossy().into_owned())],
            ..LoadOptions::default()
        })
        .map_err(|e| format!("{}: {e}", dir.display()))?;
        Ok(Self {
            _tmp: tmp,
            cfg: Arc::new(cfg),
        })
    }
}

/// Pass/fail counts of one fixture family, with the first failures for the report.
#[derive(Default)]
pub struct Tally {
    pub total: usize,
    pub failed: usize,
    /// Differences accepted as documented deviations, by class.
    pub accepted: BTreeMap<&'static str, usize>,
    /// Cases outside this crate (template argument conversion), by class.
    pub skipped: BTreeMap<&'static str, usize>,
    pub samples: Vec<String>,
}

impl Tally {
    pub fn pass(&mut self) {
        self.total += 1;
    }

    pub fn fail(&mut self, detail: impl FnOnce() -> String) {
        self.total += 1;
        self.failed += 1;
        if self.samples.len() < 25 {
            self.samples.push(detail());
        }
    }

    pub fn check(&mut self, ok: bool, detail: impl FnOnce() -> String) {
        if ok {
            self.pass();
        } else {
            self.fail(detail);
        }
    }

    /// A difference that `expected_diffs.toml` documents.
    pub fn accept(&mut self, class: &'static str) {
        self.total += 1;
        *self.accepted.entry(class).or_default() += 1;
    }

    /// A case that tests another crate's job.
    pub fn skip(&mut self, class: &'static str) {
        *self.skipped.entry(class).or_default() += 1;
    }

    /// Prints the tally and asserts that nothing unexplained differs and that the accepted
    /// deviations are the reviewed ones.
    pub fn finish(&self, family: &str) {
        let exact = self.total - self.failed - self.accepted.values().sum::<usize>();
        #[expect(clippy::cast_precision_loss, reason = "a percentage")]
        let rate = exact as f64 * 100.0 / self.total.max(1) as f64;
        eprintln!(
            "{family}: {} checks, {exact} exact ({rate:.3}%), accepted deviations {:?}, \
             not this crate's {:?}, unexplained {}",
            self.total, self.accepted, self.skipped, self.failed
        );
        for s in &self.samples {
            eprintln!("  {s}");
        }
        assert_eq!(self.failed, 0, "{family}: unexplained differences");
        let got: BTreeMap<String, usize> = self
            .accepted
            .iter()
            .map(|(k, v)| ((*k).to_owned(), *v))
            .collect();
        assert_eq!(
            got,
            expected_diffs(family),
            "{family}: accepted deviations differ from expected_diffs.toml"
        );
    }
}

/// The reviewed deviation counts of a family (`crates/nav/expected_diffs.toml`).
fn expected_diffs(family: &str) -> BTreeMap<String, usize> {
    let path = repo_dir().join("crates/nav/expected_diffs.toml");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let doc = Value::from_toml_str(&text).expect("expected_diffs.toml");
    let Some(classes) = doc
        .as_map()
        .and_then(|m| m.get(family))
        .and_then(Value::as_map)
    else {
        return BTreeMap::new();
    };
    classes
        .iter()
        .map(|(class, entry)| {
            let count = entry
                .as_map()
                .and_then(|e| e.get("count"))
                .and_then(Value::as_i64)
                .unwrap_or_else(|| panic!("{family}.{class}: no count"));
            (class.to_owned(), usize::try_from(count).expect("count"))
        })
        .collect()
}

/// A value as the oracle writes it.
pub fn to_json(v: &Value) -> J {
    match v {
        Value::Null => J::Null,
        Value::Bool(b) => J::Bool(*b),
        Value::Int(i) => J::from(*i),
        Value::Float(f) => J::from(*f),
        Value::String(s) => J::from(&**s),
        Value::Date(ssg_base::Date::Zoned(z)) => {
            serde_json::json!({ "$nh:time": z.timestamp().to_string() })
        }
        Value::Date(ssg_base::Date::Local(dt)) => {
            serde_json::json!({ "$nh:local": dt.to_string() })
        }
        Value::Array(a) => J::Array(a.iter().map(to_json).collect()),
        Value::Map(m) => J::Object(m.iter().map(|(k, v)| (k.to_owned(), to_json(v))).collect()),
    }
}

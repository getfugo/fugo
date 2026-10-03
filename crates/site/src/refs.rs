//! Page references: `.GetPage`, `.Site.GetPage`, `ref` and `relref` (semantics §12.4).
//!
//! A reference without an extension names a content file with `.md` (`/` names the home page's
//! `/_index.md`); it is read like a content path (lower case, spaces as `-`, language and
//! index names dropped) and looked up in the language's tree:
//!
//! 1. relative to the page asking (a relative reference; `./` and `../` end here): from a
//!    branch page's own directory, else from its container (a `../x.md` with an extension goes
//!    from the page's directory), then, for a reference with an extension, as a file next to
//!    the page's own file;
//! 2. from the root, then (with an extension) as a file below the content root;
//! 3. a reference without a `/`, asked outside a page or by `ref`/`relref`, by page name
//!    (`about` finds `/company/about`); a name two pages share is ambiguous.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use std::sync::Arc;

use ssg_base::paths::{self, ContentKey};
use ssg_base::url::{self, Component};
use ssg_base::{IdVec, LangIdx, PageId, PageKind};
use ssg_vfs::{BundleKind, FileRef, Parsed, PathParser};

use crate::Model;

/// Why a reference does not resolve to a page.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RefError {
    #[error("page reference {0:?} is ambiguous")]
    Ambiguous(String),
    #[error("too many arguments to GetPage: {0:?} (use one path, such as \"/posts/my-post\")")]
    TooManyArguments(Vec<String>),
    #[error("page reference {0:?}: page not found")]
    NotFound(String),
    #[error("page reference {reference:?}: the page has no output format {format:?}")]
    NoFormat { reference: String, format: String },
    #[error("page reference {reference:?}: no language {lang:?}")]
    NoLanguage { reference: String, lang: String },
}

/// The arguments of `ref`/`relref`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RefArgs {
    /// The reference, with an optional `#fragment`.
    pub path: String,
    /// Resolve in another language.
    pub lang: Option<String>,
    /// Link to this output format of the page.
    pub output_format: Option<String>,
}

/// Which link `ref_link` makes: `ref`'s permalink or `relref`'s relative permalink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefLink {
    Permalink,
    RelPermalink,
}

/// Who looks a reference up: `.GetPage` finds pages by name only outside a page, `ref`
/// everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lookup {
    GetPage,
    Ref,
}

/// A page name in the name index: one page, or several.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Named {
    One(PageId),
    Ambiguous,
}

/// The lookup tables of references.
#[derive(Clone, Debug)]
pub(crate) struct RefIndex {
    parser: Arc<PathParser>,
    /// Per language: page name (`BaseNameNoIdentifier`) → page.
    names: IdVec<LangIdx, BTreeMap<String, Named>>,
}

impl RefIndex {
    pub fn new(parser: Arc<PathParser>) -> Self {
        Self {
            parser,
            names: IdVec::new(),
        }
    }

    /// The content path parser.
    pub fn parser(&self) -> &PathParser {
        &self.parser
    }

    /// Indexes the model's pages (after the structure is assembled).
    pub fn build(m: &Model) -> Self {
        let mut names: IdVec<LangIdx, BTreeMap<String, Named>> = IdVec::new();
        for site in &m.sites {
            let mut n: BTreeMap<String, Named> = BTreeMap::new();
            for (_, id) in site.tree.iter() {
                n.entry(m.pages[id].path_info.name.clone())
                    .and_modify(|e| *e = Named::Ambiguous)
                    .or_insert(Named::One(id));
            }
            names.push(n);
        }
        Self {
            parser: m.refs.parser.clone(),
            names,
        }
    }

    /// The key and name of a content path.
    fn parse(&self, rel: &str) -> Option<(ContentKey, String)> {
        match self.parser.parse(ssg_vfs::Component::Content, rel) {
            Parsed::File(info) => Some((info.key.clone(), info.name.clone())),
            Parsed::DisabledLanguage => None,
        }
    }
}

/// Whether a reference has an extension: a `.` in its last element (Go's `HasExt`).
fn has_ext(s: &str) -> bool {
    s.rsplit('/').next().unwrap_or(s).contains('.')
}

impl Model {
    /// `.GetPage`/`.Site.GetPage` with one reference, from the page `from` (relative references
    /// need one). `Ok(None)`: no such page.
    ///
    /// # Errors
    /// [`RefError::Ambiguous`] for a page name two pages share.
    pub fn get_page(
        &self,
        lang: LangIdx,
        reference: &str,
        from: Option<PageId>,
    ) -> Result<Option<PageId>, RefError> {
        self.find(lang, reference, from, Lookup::GetPage)
    }

    /// `.Site.GetPage` with its legacy arguments (`"section" "blog"`, `"home"`): the last
    /// non-empty argument is the reference; a lone kind `home` (or `section` with an empty
    /// path) is the home page.
    ///
    /// # Errors
    /// More than two non-empty arguments, or an ambiguous page name.
    pub fn site_get_page(&self, lang: LangIdx, args: &[&str]) -> Result<Option<PageId>, RefError> {
        let refs: Vec<&str> = args
            .iter()
            .copied()
            .filter(|r| !r.is_empty() && *r != "/")
            .collect();
        let key = match refs.as_slice() {
            [] => "/",
            [k, ..] if *k == PageKind::Home.as_str() => "/",
            [k] if args.len() == 2 && *k == PageKind::Section.as_str() => "/",
            [k] => k,
            [_, k] => k,
            _ => {
                return Err(RefError::TooManyArguments(
                    args.iter().map(|s| (*s).to_owned()).collect(),
                ));
            }
        };
        self.get_page(lang, key, None)
    }

    /// The page a `ref`/`relref` reference names (which also finds pages by name from inside a
    /// page).
    ///
    /// # Errors
    /// [`RefError::Ambiguous`] for a page name two pages share.
    pub fn ref_page(
        &self,
        lang: LangIdx,
        reference: &str,
        from: Option<PageId>,
    ) -> Result<Option<PageId>, RefError> {
        self.find(lang, reference, from, Lookup::Ref)
    }

    /// `ref` ([`RefLink::Permalink`]) and `relref` ([`RefLink::RelPermalink`]): the page's link,
    /// in its primary format or `args.output_format`, with the reference's `#fragment`.
    /// An empty reference is the empty string; a lone `#fragment` is itself.
    ///
    /// # Errors
    /// The page is not found (the caller reports it at `refLinks.errorLevel` and links to
    /// `refLinks.notFoundURL`), the language or format does not exist, or the name is
    /// ambiguous.
    pub fn ref_link(
        &self,
        lang: LangIdx,
        args: &RefArgs,
        from: Option<PageId>,
        kind: RefLink,
    ) -> Result<String, RefError> {
        let lang = match args.lang.as_deref().filter(|l| !l.is_empty()) {
            None => lang,
            Some(l) => self
                .config
                .sites
                .iter_enumerated()
                .find(|(_, s)| s.language.key == l)
                .map(|(i, _)| i)
                .ok_or_else(|| RefError::NoLanguage {
                    reference: args.path.clone(),
                    lang: l.to_owned(),
                })?,
        };
        if args.path.is_empty() {
            return Ok(String::new());
        }
        let reference = args.path.replace('\\', "/");
        let (path, fragment) = match reference.split_once('#') {
            Some((p, f)) => (p, Some(f)),
            None => (reference.as_str(), None),
        };
        let path = path.split_once('?').map_or(path, |(p, _)| p);
        let path = url::unescape(path, Component::Path)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_else(|| path.to_owned());
        let mut link = String::new();
        if !path.is_empty() {
            let page = self
                .ref_page(lang, &path, from)?
                .ok_or_else(|| RefError::NotFound(path.clone()))?;
            let p = &self.pages[page];
            let url = match args.output_format.as_deref().filter(|f| !f.is_empty()) {
                None => p.urls.first(),
                Some(name) => self
                    .config
                    .output_formats
                    .by_name(&ssg_base::text::to_lower(name))
                    .and_then(|f| p.url(f))
                    .filter(|u| u.links.is_some())
                    .ok_or_else(|| RefError::NoFormat {
                        reference: path.clone(),
                        format: name.to_owned(),
                    })
                    .map(Some)?,
            };
            if let Some(l) = url.and_then(|u| u.links.as_ref()) {
                link = match kind {
                    RefLink::RelPermalink => l.rel_permalink.escaped(),
                    RefLink::Permalink => l.permalink.to_string(),
                };
            }
        }
        if let Some(f) = fragment.filter(|f| !f.is_empty()) {
            link.push('#');
            link.push_str(f);
        }
        Ok(link)
    }

    fn find(
        &self,
        lang: LangIdx,
        reference: &str,
        from: Option<PageId>,
        lookup: Lookup,
    ) -> Result<Option<PageId>, RefError> {
        let slashed = reference.replace('\\', "/");
        let in_ref = slashed.strip_suffix('/').unwrap_or(&slashed);
        let r = if in_ref.is_empty() { "/" } else { in_ref };
        if has_ext(r) {
            return self.find_ref(lang, from, lookup, true, in_ref, r);
        }
        let candidates: Vec<String> = if r == "/" {
            vec!["/_index.md".to_owned()]
        } else if r.ends_with("/index") {
            vec![format!("{r}/index.md"), format!("{r}.md")]
        } else {
            vec![format!("{r}.md")]
        };
        for c in candidates {
            if let Some(id) = self.find_ref(lang, from, lookup, false, in_ref, &c)? {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    fn find_ref(
        &self,
        lang: LangIdx,
        from: Option<PageId>,
        lookup: Lookup,
        had_ext: bool,
        in_ref: &str,
        r: &str,
    ) -> Result<Option<PageId>, RefError> {
        let idx = &self.refs;
        let tree = &self.sites[lang].tree;
        if let Some(ctx) = from
            && !r.starts_with('/')
        {
            let p = &self.pages[ctx];
            // Go's `Dir()`, else its `ContainerDir()` (empty for a bundle at the root).
            let base_dir =
                if p.path_info.kind == BundleKind::Branch || (had_ext && r.starts_with("../")) {
                    p.dir_key().to_path()
                } else {
                    match p.key.parent() {
                        Some(k) if k.is_home() && p.path_info.kind.is_bundle() => String::new(),
                        k => k.unwrap_or_default().to_path(),
                    }
                };
            let mut rel = paths::join(&[&base_dir, r]);
            if rel.starts_with("..") {
                // Above the root is the root.
                rel = paths::join(&["/", &rel]);
            }
            if rel != "."
                && let Some((key, _)) = idx.parse(&rel)
                && let Some(id) = tree.get(&key)
            {
                return Ok(Some(id));
            }
            if had_ext
                && let Some(s) = &p.source
                && let Some(id) = self.by_file(lang, &s.file, r)
            {
                return Ok(Some(id));
            }
        }
        if r.starts_with('.') {
            return Ok(None);
        }
        let Some((key, name)) = idx.parse(r) else {
            return Ok(None);
        };
        if let Some(id) = tree.get(&key) {
            return Ok(Some(id));
        }
        let home = &self.pages[self.sites[lang].home];
        if had_ext
            && let Some(s) = &home.source
            && let Some(id) = self.by_file(lang, &s.file, r)
        {
            return Ok(Some(id));
        }
        if (lookup == Lookup::Ref || from.is_none()) && !in_ref.contains('/') {
            return match idx.names.get(lang).and_then(|n| n.get(&name)) {
                None => Ok(None),
                Some(Named::One(id)) => Ok(Some(*id)),
                Some(Named::Ambiguous) => Err(RefError::Ambiguous(in_ref.to_owned())),
            };
        }
        Ok(None)
    }

    /// The page of the existing content file or directory `rel` next to `file`, in `lang`'s
    /// tree (a directory names its bundle).
    fn by_file(&self, lang: LangIdx, file: &FileRef, rel: &str) -> Option<PageId> {
        let root = file.abs.to_str()?.strip_suffix(file.rel.as_str())?;
        let dir = file.abs.parent()?;
        let target = clean_path(&dir.join(rel.trim_start_matches('/')));
        if !target.exists() {
            return None;
        }
        let below = target.to_str()?.strip_prefix(root)?;
        let (key, _) = self.refs.parse(below)?;
        self.sites[lang].tree.get(&key)
    }
}

/// `p` with `.` and `..` elements resolved lexically.
fn clean_path(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

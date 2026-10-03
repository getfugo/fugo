//! The path parser: a component-relative file path → [`PathInfo`] (identity, language, output
//! format, bundle kind, section).
//!
//! The rules (the Go implementation's `common/paths/pathparser.go`):
//!
//! - **Normalisation.** The key and every derived name come from the lower-cased path with
//!   spaces replaced by `-` ([`normalize_key`]); nothing else changes (`&`, `'`, `.` stay).
//!   [`PathInfo::original`] holds the same names in the file's own spelling.
//! - **Identifiers** are read right to left from the last path element only: the first is the
//!   extension. In a name with more than one dot (content and layouts only) the next one may be
//!   a language key; a *disabled* language makes the file [`Parsed::DisabledLanguage`]. In
//!   layouts the remaining identifiers are output formats, page kinds, `baseof`, or layout
//!   names. Any other identifier stays part of the name (`v1.2.3.md` is `v1.2.3`).
//! - **Bundle kind** (content and archetypes with a content suffix): `index` is a leaf bundle,
//!   `_index` a branch bundle, anything else a single page. `_content.gotmpl` is a content
//!   adapter, and so is `_content.html` in the content component: our adapters are Tera
//!   templates (Go would read that file as an HTML page). Files inside a leaf bundle are made
//!   resources by discovery ([`PathInfo::into_bundled`]).
//! - **Key.** A page's key drops the extension, the language and the `index`/`_index` element;
//!   a resource keeps its extension (`blog/post/cover.jpg`, `blog/post/notes.md`).

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use ssg_base::paths::{ContentKey, normalize_key};
use ssg_base::{FormatId, LangIdx, PageKind};
use ssg_config::Config;

use crate::Component;

/// What a file is to the content tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BundleKind {
    /// A page of its own: `posts/my-post.md`.
    Single,
    /// A leaf bundle's index: `posts/my-post/index.md`.
    Leaf,
    /// A branch bundle's index: `posts/_index.md`.
    Branch,
    /// A content adapter: `_content.html` (a Tera template), or Go's `_content.gotmpl`.
    ContentAdapter,
    /// A content file inside a leaf bundle (other than the bundle's own index).
    ContentResource,
    /// Any other file: bundle images and data, and every file of the other components.
    Resource,
}

impl BundleKind {
    /// Leaf and branch bundles and content adapters: the key is the directory.
    #[must_use]
    pub const fn is_bundle(self) -> bool {
        matches!(self, Self::Leaf | Self::Branch | Self::ContentAdapter)
    }

    /// Files that become pages (keyed without extension).
    #[must_use]
    pub const fn is_page(self) -> bool {
        matches!(
            self,
            Self::Single | Self::Leaf | Self::Branch | Self::ContentAdapter
        )
    }

    /// Files with a content suffix (pages and bundled content resources).
    #[must_use]
    pub const fn is_content(self) -> bool {
        !matches!(self, Self::Resource)
    }
}

/// What a file of the layouts component is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutRole {
    /// A page or list template (`single.html`, `posts/list.rss.xml`).
    Template,
    /// A base template (`baseof.html`, `baseof.list.html`).
    Baseof,
    /// Under `_partials/`.
    Partial,
    /// Under `_shortcodes/`.
    Shortcode,
    /// Under `_markup/` (render hooks).
    Markup,
}

/// The identifiers of a layout file name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutParts {
    pub role: LayoutRole,
    /// A page kind identifier (`home.html`, `list.section.html`).
    pub kind: Option<PageKind>,
    /// The layout identifier (the leftmost identifier that is nothing else).
    pub layout: Option<String>,
}

/// The names of a path in the file's own spelling (case and spaces kept).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Original {
    /// The full path with a leading slash (`/Posts/My Post.en.md`).
    pub path: String,
    /// The key with a leading slash (`/Posts/My Post`); section titles and term names use it.
    pub base: String,
    /// See [`PathInfo::name`].
    pub name: String,
    /// See [`PathInfo::section`].
    pub section: String,
}

/// A parsed file path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathInfo {
    pub component: Component,
    /// The tree key (Go's `Base()` without the leading slash).
    pub key: ContentKey,
    /// The normalised full path with a leading slash (`/posts/my-post.en.md`).
    pub path: String,
    /// The logical name: the bundle directory for bundles, else the file name without
    /// identifiers (`my-post`).
    pub name: String,
    /// The first path element (`posts`); empty for root files and root leaf bundles.
    pub section: String,
    /// The extension without the dot (normalised), empty if none.
    pub ext: String,
    /// The language named in the file name (`index.th.md`).
    pub lang: Option<LangIdx>,
    /// The output format named in a layout file name (`list.rss.xml`).
    pub format: Option<FormatId>,
    pub kind: BundleKind,
    /// Set for the layouts component.
    pub layout: Option<LayoutParts>,
    pub original: Original,
    norm: Shape,
    orig: Shape,
}

impl PathInfo {
    /// The normalised directory of the path with a leading slash (`/posts`, `/` at the root).
    #[must_use]
    pub fn dir(&self) -> &str {
        match &self.path[..self.norm.container_high - 1] {
            "" => "/",
            d => d,
        }
    }

    /// The same file as a resource of a leaf bundle: content files become
    /// [`BundleKind::ContentResource`], everything else [`BundleKind::Resource`]; the key then
    /// keeps the extension (`post/notes.md`).
    #[must_use]
    pub fn into_bundled(mut self) -> Self {
        self.kind = if self.kind.is_content() {
            BundleKind::ContentResource
        } else {
            BundleKind::Resource
        };
        self.derive_names();
        self
    }

    fn derive_names(&mut self) {
        self.key = ContentKey::from_source(&self.norm.base(self.kind));
        self.name = self.norm.base_name(self.kind).to_owned();
        self.original.base = self.orig.base(self.kind);
        self.original.name = self.orig.base_name(self.kind).to_owned();
    }
}

/// The result of [`PathParser::parse`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parsed {
    File(Box<PathInfo>),
    /// The file name carries a disabled language (`post.fr.md` with `fr` disabled): the file
    /// is not part of the build.
    DisabledLanguage,
}

/// An output format as the parser sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatSpec {
    pub name: String,
    pub id: FormatId,
    /// The media type's suffixes.
    pub suffixes: Vec<String>,
}

/// What the parser knows about a project; built from a [`Config`] by
/// [`PathParser::from_config`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathParserSpec {
    /// Enabled language keys.
    pub languages: Vec<(String, LangIdx)>,
    /// Disabled language keys.
    pub disabled_languages: Vec<String>,
    pub output_formats: Vec<FormatSpec>,
    /// The suffixes of the content media types (`md`, `html`, …).
    pub content_suffixes: Vec<String>,
}

/// Parses component-relative paths into [`PathInfo`].
#[derive(Clone, Debug)]
pub struct PathParser {
    languages: BTreeMap<String, LangIdx>,
    disabled: BTreeSet<String>,
    formats: Vec<FormatSpec>,
    content_suffixes: BTreeSet<String>,
}

impl PathParser {
    #[must_use]
    pub fn new(spec: PathParserSpec) -> Self {
        let lower = |s: String| normalize_key(&s);
        Self {
            languages: spec
                .languages
                .into_iter()
                .map(|(k, i)| (lower(k), i))
                .collect(),
            disabled: spec.disabled_languages.into_iter().map(lower).collect(),
            formats: spec
                .output_formats
                .into_iter()
                .map(|f| FormatSpec {
                    name: lower(f.name),
                    id: f.id,
                    suffixes: f.suffixes.into_iter().map(lower).collect(),
                })
                .collect(),
            content_suffixes: spec.content_suffixes.into_iter().map(lower).collect(),
        }
    }

    /// The parser of a project: its enabled and disabled languages, output formats and
    /// content media types.
    #[must_use]
    pub fn from_config(cfg: &Config) -> Self {
        let types = &cfg.media_types;
        Self::new(PathParserSpec {
            languages: cfg
                .sites
                .iter_enumerated()
                .map(|(i, s)| (s.language.key.clone(), i))
                .collect(),
            disabled_languages: cfg.disabled_languages.clone(),
            output_formats: cfg
                .output_formats
                .iter()
                .map(|(id, f)| FormatSpec {
                    name: f.name.clone(),
                    id,
                    suffixes: types.get(f.media_type).suffixes.clone(),
                })
                .collect(),
            content_suffixes: cfg
                .content_types
                .0
                .iter()
                .flat_map(|&id| types.get(id).suffixes.iter().cloned())
                .collect(),
        })
    }

    /// The index of the enabled language `key`.
    #[must_use]
    pub fn language(&self, key: &str) -> Option<LangIdx> {
        self.languages.get(&normalize_key(key)).copied()
    }

    /// Whether `key` is a disabled language.
    #[must_use]
    pub fn is_disabled_language(&self, key: &str) -> bool {
        self.disabled.contains(&normalize_key(key))
    }

    /// Parses `rel`, a `/`-separated path inside component `c` (a leading slash is optional).
    #[must_use]
    pub fn parse(&self, c: Component, rel: &str) -> Parsed {
        let norm = self.scan(c, normalize_key(rel));
        if norm.disabled {
            return Parsed::DisabledLanguage;
        }
        let orig = self.scan(c, rel.to_owned());
        let kind = match norm.ty {
            Ty::Single => BundleKind::Single,
            Ty::Leaf => BundleKind::Leaf,
            Ty::Branch => BundleKind::Branch,
            Ty::ContentData => BundleKind::ContentAdapter,
            Ty::File | Ty::Markup | Ty::Shortcode | Ty::Partial | Ty::Baseof => {
                BundleKind::Resource
            }
        };
        let layout = (c == Component::Layouts).then(|| LayoutParts {
            role: match norm.ty {
                Ty::Baseof => LayoutRole::Baseof,
                Ty::Partial => LayoutRole::Partial,
                Ty::Shortcode => LayoutRole::Shortcode,
                Ty::Markup => LayoutRole::Markup,
                _ => LayoutRole::Template,
            },
            kind: norm.page_kind,
            layout: norm.shape.id(norm.layout).map(str::to_owned),
        });
        let mut info = PathInfo {
            component: c,
            key: ContentKey::home(),
            path: norm.shape.s.clone(),
            name: String::new(),
            section: norm.shape.section().to_owned(),
            ext: norm.shape.id(Some(0)).unwrap_or_default().to_owned(),
            lang: norm.lang_idx,
            format: norm.format_id,
            kind,
            layout,
            original: Original {
                path: orig.shape.s.clone(),
                base: String::new(),
                name: String::new(),
                section: orig.shape.section().to_owned(),
            },
            norm: norm.shape,
            orig: orig.shape,
        };
        info.derive_names();
        Parsed::File(Box::new(info))
    }

    fn output_format(&self, name: &str, ext: &str) -> Option<FormatId> {
        let f = self.formats.iter().find(|f| f.name == name)?;
        (ext.is_empty() || f.suffixes.iter().any(|s| s == ext)).then_some(f.id)
    }

    /// Finds the identifiers and the kind of `s`. Every comparison uses the normalised form of
    /// the compared piece, so the original spelling of a path gets the same structure as its
    /// normalised form.
    fn scan(&self, c: Component, s: String) -> Scan {
        let s = with_slashes(s);
        let last_slash = s.rfind('/').unwrap_or_default();
        let container_high = last_slash + 1;
        let dots: Vec<usize> = s[container_high..]
            .match_indices('.')
            .map(|(i, _)| container_high + i)
            .collect();
        let folded = normalize_key(&s);
        let mut sc = Scan {
            shape: Shape {
                container_low: s[..last_slash].rfind('/').map(|i| i + 1),
                container_high,
                section_high: s[1..].find('/').map(|i| i + 1),
                ids: Vec::new(),
                s,
            },
            lang: None,
            layout: None,
            baseof: None,
            lang_idx: None,
            format_id: None,
            page_kind: None,
            disabled: false,
            ty: if folded.contains("/_shortcodes/") {
                Ty::Shortcode
            } else {
                Ty::File
            },
        };

        let mut last_dot = 0;
        for &i in dots.iter().rev() {
            self.identifier(c, &mut sc, i + 1..last_dot, dots.len(), false);
            last_dot = i;
        }
        if !dots.is_empty() {
            self.identifier(c, &mut sc, container_high..last_dot, dots.len(), true);
        }

        let shape = &sc.shape;
        if let (Some(first), Some(last)) = (shape.ids.first(), shape.ids.last()) {
            let ext = normalize_key(&shape.s[first.clone()]);
            let is_content = matches!(c, Component::Content | Component::Archetypes)
                && self.content_suffixes.contains(&ext);
            if last.start > container_high {
                let stem = normalize_key(&shape.s[container_high..last.start - 1]);
                if stem == "_content" && c == Component::Content && ext == "html" {
                    // our content adapter (a Tera template), not an HTML page.
                    sc.ty = Ty::ContentData;
                } else if is_content {
                    sc.ty = match stem.as_str() {
                        "index" => Ty::Leaf,
                        "_index" => Ty::Branch,
                        _ => Ty::Single,
                    };
                    let slashes = shape.s.bytes().filter(|&b| b == b'/').count();
                    if slashes == 2 && sc.ty == Ty::Leaf {
                        // A leaf bundle at the root is in no section.
                        sc.shape.section_high = None;
                    }
                } else if stem == "_content" && ext == "gotmpl" {
                    sc.ty = Ty::ContentData;
                }
            }
        }

        if c == Component::Layouts && sc.ty.is_plain() {
            if sc.baseof.is_some() {
                sc.ty = Ty::Baseof;
            } else if folded.contains("/_shortcodes/") {
                sc.ty = Ty::Shortcode;
            } else if folded.contains("/_markup/") {
                sc.ty = Ty::Markup;
            } else if folded.starts_with("/_partials/") {
                sc.ty = Ty::Partial;
            }
        }
        if sc.ty == Ty::Shortcode
            && sc
                .layout
                .is_some_and(|l| sc.shape.ids[l].start == container_high)
        {
            // The shortcode's own name is not a layout.
            sc.layout = None;
        }
        sc
    }

    /// Classifies the identifier at `range` (right to left; the first one is the extension).
    fn identifier(
        &self,
        c: Component,
        sc: &mut Scan,
        range: Range<usize>,
        num_dots: usize,
        is_name: bool,
    ) {
        let ids = &mut sc.shape.ids;
        let range = if ids.is_empty() {
            range.start..sc.shape.s.len()
        } else {
            range
        };
        let id = normalize_key(&sc.shape.s[range.clone()]);
        if ids.is_empty() {
            ids.push(range);
            if c == Component::Layouts {
                sc.format_id = self.output_format(&id, "");
            }
            return;
        }

        let may_have_lang = num_dots > 1
            && sc.lang.is_none()
            && !self.languages.is_empty()
            && matches!(c, Component::Content | Component::Layouts);
        if may_have_lang {
            if let Some(&l) = self.languages.get(&id) {
                sc.lang_idx = Some(l);
            } else if self.disabled.contains(&id) {
                sc.disabled = true;
            }
            if sc.lang_idx.is_some() || sc.disabled {
                sc.lang = Some(ids.len());
                ids.push(range);
                return;
            }
        }
        if c != Component::Layouts {
            if id == "baseof" {
                sc.baseof = Some(ids.len());
                ids.push(range);
            }
            return;
        }

        let ext = normalize_key(&sc.shape.s[ids[0].clone()]);
        if let Some(f) = self.output_format(&id, &ext) {
            // A more specific format than the extension (`amp` in `index.amp.html`).
            sc.format_id = Some(f);
        } else if let Some(k) = sc.page_kind.is_none().then(|| main_kind(&id)).flatten() {
            sc.page_kind = Some(k);
        } else if id == "baseof" {
            sc.baseof = Some(ids.len());
        } else if sc.ty != Ty::Shortcode || !is_name {
            sc.layout = Some(ids.len());
        } else {
            return;
        }
        ids.push(range);
    }
}

/// The page kinds a layout file name may name (the legacy `taxonomyterm` is `taxonomy`).
fn main_kind(id: &str) -> Option<PageKind> {
    PageKind::parse(id).filter(|k| {
        matches!(
            k,
            PageKind::Home
                | PageKind::Page
                | PageKind::Section
                | PageKind::Taxonomy
                | PageKind::Term
        )
    })
}

/// A leading slash and no trailing slash (`/` for the empty path).
fn with_slashes(mut s: String) -> String {
    if !s.starts_with('/') {
        s.insert(0, '/');
    }
    if s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    s
}

/// The Go path type; `ContentResource` only arises from [`PathInfo::into_bundled`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ty {
    File,
    Single,
    Leaf,
    Branch,
    ContentData,
    Markup,
    Shortcode,
    Partial,
    Baseof,
}

impl Ty {
    /// Not (yet) a layout role.
    const fn is_plain(self) -> bool {
        matches!(
            self,
            Self::File | Self::Single | Self::Leaf | Self::Branch | Self::ContentData
        )
    }
}

struct Scan {
    shape: Shape,
    /// Positions in `shape.ids`.
    lang: Option<usize>,
    layout: Option<usize>,
    baseof: Option<usize>,
    lang_idx: Option<LangIdx>,
    format_id: Option<FormatId>,
    page_kind: Option<PageKind>,
    disabled: bool,
    ty: Ty,
}

/// The positions of the parts of one spelling of a path.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Shape {
    /// The path with a leading slash.
    s: String,
    /// Start of the parent directory's name.
    container_low: Option<usize>,
    /// Start of the file name.
    container_high: usize,
    /// End of the first element.
    section_high: Option<usize>,
    /// The identifiers, right to left; `[0]` is the extension.
    ids: Vec<Range<usize>>,
}

impl Shape {
    fn id(&self, i: Option<usize>) -> Option<&str> {
        i.and_then(|i| self.ids.get(i)).map(|r| &self.s[r.clone()])
    }

    fn section(&self) -> &str {
        match self.section_high {
            Some(h) if h > 0 => &self.s[1..h],
            _ => "",
        }
    }

    fn container(&self) -> &str {
        self.container_low
            .map_or("", |low| &self.s[low..self.container_high - 1])
    }

    /// The file name without its identifiers (the name itself when it is an identifier, as in
    /// `/en.x.md`).
    fn name_range(&self) -> Range<usize> {
        match self.ids.last() {
            Some(last) if last.start == self.container_high => last.clone(),
            Some(last) => self.container_high..last.start - 1,
            None => self.container_high..self.s.len(),
        }
    }

    fn base_name(&self, kind: BundleKind) -> &str {
        if kind.is_bundle() {
            self.container()
        } else {
            &self.s[self.name_range()]
        }
    }

    /// Pages drop every identifier (and the bundle's index name); resources keep the
    /// extension.
    fn base(&self, kind: BundleKind) -> String {
        let keep_ext = !kind.is_page();
        let Some(ext) = self.ids.first() else {
            return self.s.clone();
        };
        if keep_ext && self.ids.len() == 1 {
            return self.s.clone();
        }
        let high = if kind.is_bundle() {
            self.container_high - 1
        } else {
            self.name_range().end
        }
        .max(1);
        if keep_ext {
            format!("{}{}", &self.s[..high], &self.s[ext.start - 1..ext.end])
        } else {
            self.s[..high].to_owned()
        }
    }
}

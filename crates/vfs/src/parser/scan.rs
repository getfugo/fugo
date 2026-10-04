//! Scanning a file name as Go does: its base, identifiers (language, output format, kind) and their
//! shape.

use super::*;

/// The page kinds a layout file name may name (the legacy `taxonomyterm` is `taxonomy`).
pub(super) fn main_kind(id: &str) -> Option<PageKind> {
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
pub(super) fn with_slashes(mut s: String) -> String {
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
pub(super) enum Ty {
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
    pub(super) const fn is_plain(self) -> bool {
        matches!(
            self,
            Self::File | Self::Single | Self::Leaf | Self::Branch | Self::ContentData
        )
    }
}

pub(super) struct Scan {
    pub(super) shape: Shape,
    /// Positions in `shape.ids`.
    pub(super) lang: Option<usize>,
    pub(super) layout: Option<usize>,
    pub(super) baseof: Option<usize>,
    pub(super) lang_idx: Option<LangIdx>,
    pub(super) format_id: Option<FormatId>,
    pub(super) page_kind: Option<PageKind>,
    pub(super) disabled: bool,
    pub(super) ty: Ty,
}

/// The positions of the parts of one spelling of a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Shape {
    /// The path with a leading slash.
    pub(super) s: String,
    /// Start of the parent directory's name.
    pub(super) container_low: Option<usize>,
    /// Start of the file name.
    pub(super) container_high: usize,
    /// End of the first element.
    pub(super) section_high: Option<usize>,
    /// The identifiers, right to left; `[0]` is the extension.
    pub(super) ids: Vec<Range<usize>>,
}

impl Shape {
    pub(super) fn id(&self, i: Option<usize>) -> Option<&str> {
        i.and_then(|i| self.ids.get(i)).map(|r| &self.s[r.clone()])
    }

    pub(super) fn section(&self) -> &str {
        match self.section_high {
            Some(h) if h > 0 => &self.s[1..h],
            _ => "",
        }
    }

    pub(super) fn container(&self) -> &str {
        self.container_low
            .map_or("", |low| &self.s[low..self.container_high - 1])
    }

    /// The file name without its identifiers (the name itself when it is an identifier, as in
    /// `/en.x.md`).
    pub(super) fn name_range(&self) -> Range<usize> {
        match self.ids.last() {
            Some(last) if last.start == self.container_high => last.clone(),
            Some(last) => self.container_high..last.start - 1,
            None => self.container_high..self.s.len(),
        }
    }

    pub(super) fn base_name(&self, kind: BundleKind) -> &str {
        if kind.is_bundle() {
            self.container()
        } else {
            &self.s[self.name_range()]
        }
    }

    /// Pages drop every identifier (and the bundle's index name); resources keep the
    /// extension.
    pub(super) fn base(&self, kind: BundleKind) -> String {
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

impl PathParser {
    /// Finds the identifiers and the kind of `s`. Every comparison uses the normalised form of
    /// the compared piece, so the original spelling of a path gets the same structure as its
    /// normalised form.
    pub(super) fn scan(&self, c: Component, s: String) -> Scan {
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
    pub(super) fn identifier(
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

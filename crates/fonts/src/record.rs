//! The characters of the published HTML and CSS, noted as the files are written
//! ([`Recorder`]), and the paths of the published fonts.

use std::collections::BTreeSet;
use std::io;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ssg_base::Sink;
use ssg_base::paths::OutputPath;

use crate::chars;
use crate::css::CssStrings;

/// What a published file is to the recorder, by its extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Html,
    Css,
    Font,
    Other,
}

impl Kind {
    fn of(path: &OutputPath) -> Self {
        let ext = path
            .as_str()
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "html" | "htm" => Self::Html,
            "css" => Self::Css,
            "ttf" | "otf" | "woff" | "woff2" => Self::Font,
            _ => Self::Other,
        }
    }
}

/// Whether the recorder notes the content of the published file at `path` (HTML and CSS).
pub(crate) fn is_noted(path: &OutputPath) -> bool {
    matches!(Kind::of(path), Kind::Html | Kind::Css)
}

/// Whether the published file at `path` is a font, by its extension.
pub(crate) fn is_font(path: &OutputPath) -> bool {
    Kind::of(path) == Kind::Font
}

/// A [`Sink`] that writes into another and notes, on the way, the characters of the HTML and
/// CSS files and the paths of the fonts written.
pub struct Recorder {
    inner: Arc<dyn Sink>,
    /// Whether the pages' text is needed (a `from = "text"` rule), besides their CSS strings.
    text: bool,
    css: Mutex<CssStrings>,
    page_text: Mutex<BTreeSet<char>>,
    fonts: Mutex<BTreeSet<OutputPath>>,
}

impl std::fmt::Debug for Recorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recorder")
            .field("text", &self.text)
            .finish_non_exhaustive()
    }
}

impl Recorder {
    /// A recorder writing into `inner`; `text`: note the pages' text too.
    #[must_use]
    pub fn new(inner: Arc<dyn Sink>, text: bool) -> Self {
        Self {
            inner,
            text,
            css: Mutex::default(),
            page_text: Mutex::default(),
            fonts: Mutex::default(),
        }
    }

    /// Notes the published file `path` with the content `bytes`: the characters of an HTML or
    /// CSS file, the path of a font. Writing through the recorder notes too; call it for the
    /// files that are published around it (static files).
    pub fn note(&self, path: &OutputPath, bytes: &[u8]) {
        let kind = Kind::of(path);
        if kind == Kind::Font {
            lock(&self.fonts).insert(path.clone());
            return;
        }
        if !matches!(kind, Kind::Html | Kind::Css) {
            return;
        }
        let text = String::from_utf8_lossy(bytes);
        let mut found = CssStrings::default();
        found.add(&text);
        lock(&self.css).merge(found);
        if self.text && kind == Kind::Html {
            let mut found = BTreeSet::new();
            chars::add_html_text(&text, &mut found);
            lock(&self.page_text).extend(found);
        }
    }

    /// The characters the CSS noted so far draws.
    #[must_use]
    pub fn css_chars(&self) -> BTreeSet<char> {
        lock(&self.css).chars()
    }

    /// The characters of the pages' text and of the CSS strings noted so far.
    #[must_use]
    pub fn text_chars(&self) -> BTreeSet<char> {
        let mut chars = lock(&self.page_text).clone();
        chars.extend(self.css_chars());
        chars
    }

    /// The fonts written or noted so far, sorted.
    #[must_use]
    pub fn fonts(&self) -> Vec<OutputPath> {
        lock(&self.fonts).iter().cloned().collect()
    }

    /// The sink the recorder writes into.
    #[must_use]
    pub fn inner(&self) -> &dyn Sink {
        self.inner.as_ref()
    }
}

impl Sink for Recorder {
    fn write(&self, path: &OutputPath, bytes: &[u8]) -> io::Result<()> {
        self.note(path, bytes);
        self.inner.write(path, bytes)
    }

    fn exists(&self, path: &OutputPath) -> bool {
        self.inner.exists(path)
    }

    fn read(&self, path: &OutputPath) -> io::Result<Vec<u8>> {
        self.inner.read(path)
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

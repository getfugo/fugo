//! Credits (`page_resources`): an image's `copyright` and `author` params, read per page and
//! given to its file; a file the pages of two languages publish, as one copy they share or as a
//! copy each (`duplicateResourceFiles`, multihost, a content file that is not Markdown), takes
//! the first page's in language order that gives one, whatever the order of the pages; a value
//! that cannot be written is an error naming the page's file and the resource.

use std::fs;

use ssg_base::{Clock, Idx, PageId, PageKind};
use ssg_resources::{ResourceStore, StoreConfig};
use ssg_site::Model;
use ssg_view::{PageResource, ViewError, page_resources};

use crate::support::{load_model, page};

/// Thai is the default content language: the first in language order, though English has the
/// lower weight.
const CONFIG: &str = r#"baseURL = "https://example.org/"
defaultContentLanguage = "th"
[languages.th]
weight = 2
[languages.en]
weight = 1
[imaging]
copyright = "© Site"
author = "Site"
"#;

const PHOTO: &str = "not really a JPEG";

/// How the translations of a bundle publish its files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Setup {
    /// One copy, which they share.
    Shared,
    /// A copy each: `[markup.goldmark] duplicateResourceFiles`, …
    Duplicated,
    /// … a host per language, …
    Multihost,
    /// … or English content files that are HTML, not Markdown.
    Html,
}

impl Setup {
    const ALL: [Self; 4] = [Self::Shared, Self::Duplicated, Self::Multihost, Self::Html];

    fn config(self) -> String {
        match self {
            Self::Shared | Self::Html => CONFIG.to_owned(),
            Self::Duplicated => format!("{CONFIG}[markup.goldmark]\nduplicateResourceFiles = true\n"),
            Self::Multihost => CONFIG.replace(
                "[languages.en]\n",
                "baseURL = \"https://th.example.org/\"\n[languages.en]\nbaseURL = \"https://en.example.org/\"\n",
            ),
        }
    }

    /// The English content file of the leaf bundle `dir`.
    fn en(self, dir: &str) -> String {
        let ext = if self == Self::Html { "html" } else { "md" };
        format!("content/{dir}/index.en.{ext}")
    }
}

/// A leaf bundle's content file: `extra` front matter, and `photo.jpg` metadata giving
/// `params` (YAML), if any.
fn page_file(params: &[&str], extra: &str) -> String {
    let mut out = format!("---\ntitle: T\n{extra}");
    if !params.is_empty() {
        out.push_str("resources:\n- src: photo.jpg\n  params:\n");
        for p in params {
            out.push_str(&format!("    {p}\n"));
        }
    }
    out.push_str("---\n");
    out
}

/// A site of `files`, and its page resources in a store of a build in 2031.
struct Site {
    _dir: tempfile::TempDir,
    model: Model,
    store: ResourceStore,
    resources: Result<Vec<Vec<PageResource>>, ViewError>,
}

fn load(files: &[(String, String)]) -> Site {
    let dir = tempfile::tempdir().expect("tempdir");
    for (rel, text) in files {
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, text).expect("write");
    }
    let model = load_model(dir.path());
    let store = ResourceStore::new(StoreConfig::from_config(
        &model.config,
        None,
        None,
        Clock("2031-06-01T00:00:00Z".parse().expect("clock")),
    ));
    let resources = page_resources(&model, &store);
    Site {
        _dir: dir,
        model,
        store,
        resources,
    }
}

impl Site {
    /// `photo.jpg` of page `p`.
    fn photo(&self, p: PageId) -> &PageResource {
        let lists = self.resources.as_ref().expect("resources");
        lists[p.index()]
            .iter()
            .find(|r| r.view.name == "photo.jpg")
            .expect("photo.jpg")
    }

    /// The author and copyright `photo.jpg` of page `p` is published with (`None`: the
    /// site's).
    fn credit(&self, p: PageId) -> Option<(String, String)> {
        self.store
            .credit(self.photo(p).id)
            .map(|c| (c.author, c.copyright))
    }

    /// Where `photo.jpg` of page `p` is published.
    fn target(&self, p: PageId) -> String {
        self.store.resource(self.photo(p).id).target.to_string()
    }
}

fn file(rel: &str, text: impl Into<String>) -> (String, String) {
    (rel.to_owned(), text.into())
}

#[test]
fn files_take_the_first_language_that_gives_one() {
    for setup in Setup::ALL {
        let s = load(&[
            file("config.toml", setup.config()),
            // The first language gives one, the second none: the first's.
            file(
                "content/a/index.md",
                page_file(&["author: Somchai", "copyright: © {year} Somchai"], ""),
            ),
            file(&setup.en("a"), page_file(&[], "")),
            file("content/a/photo.jpg", PHOTO),
            // The second alone gives one: a page without the metadata takes nothing away.
            file("content/b/index.md", page_file(&[], "")),
            file(&setup.en("b"), page_file(&["author: Ada"], "")),
            file("content/b/photo.jpg", PHOTO),
            // Both: the first language's.
            file("content/c/index.md", page_file(&["author: Somchai"], "")),
            file(
                &setup.en("c"),
                page_file(&["author: Ada", "copyright: © Ada"], ""),
            ),
            file("content/c/photo.jpg", PHOTO),
            // Neither: the site's.
            file("content/d/index.md", page_file(&["credit: Somchai"], "")),
            file(&setup.en("d"), page_file(&[], "")),
            file("content/d/photo.jpg", PHOTO),
            // Shared through a translation key, the English page first in the model (its key
            // `e` before `z`): the first language's still.
            file(
                &setup.en("e"),
                page_file(&["author: Ada"], "translationKey: k\n"),
            ),
            file(
                "content/z/index.md",
                page_file(&["author: Somchai"], "translationKey: k\n"),
            ),
            file("content/z/photo.jpg", PHOTO),
        ]);
        let somchai = |copyright: &str| Some(("Somchai".to_owned(), copyright.to_owned()));
        let ada = Some(("Ada".to_owned(), String::new()));
        for (path, want) in [
            ("/a", somchai("© 2031 Somchai")),
            ("/b", ada),
            ("/c", somchai("")),
            ("/d", None),
        ] {
            let [th, en] = [0, 1].map(|lang| page(&s.model, PageKind::Page, path, lang));
            assert_eq!(
                s.target(th) == s.target(en),
                setup == Setup::Shared,
                "{setup:?} {path}: one copy, or one per language"
            );
            for p in [th, en] {
                assert_eq!(s.credit(p), want, "{setup:?} {path} ({p:?})");
            }
        }
        let en = page(&s.model, PageKind::Page, "/e", 1);
        let th = page(&s.model, PageKind::Page, "/z", 0);
        assert!(en < th, "the English page comes first in the model");
        assert_eq!(s.credit(en), somchai(""), "{setup:?}");
        assert_eq!(s.credit(th), somchai(""), "{setup:?}");
    }
}

#[test]
fn params_that_cannot_be_written_name_the_page_and_the_resource() {
    for (param, reason) in [
        (
            "copyright: \"© {yaer} Ada\"",
            "params.copyright: \"{yaer}\" is not a placeholder: the only one is {year}",
        ),
        (
            "author: \"Ada\\nLovelace\"",
            "params.author: must be at most 1000 bytes of text without control characters",
        ),
        // A folded YAML block keeps its final line break.
        (
            "author: >\n      Ada\n      Lovelace",
            "params.author: must be at most 1000 bytes of text without control characters",
        ),
        (
            "author: 1815",
            "params.author: expected a string, found an integer",
        ),
    ] {
        let s = load(&[
            file("config.toml", CONFIG),
            file("content/a/index.md", page_file(&[], "")),
            file("content/a/index.en.md", page_file(&[param], "")),
            file("content/a/photo.jpg", PHOTO),
        ]);
        let e = s.resources.expect_err(param);
        assert!(matches!(e, ViewError::Credit { .. }), "{e:?}");
        let msg = e.to_string();
        assert!(
            msg.ends_with(&format!(
                "content/a/index.en.md: resource \"photo.jpg\": {reason}"
            )),
            "{msg}"
        );
    }

    // Images published as they are: the params are the site's own, not read.
    let s = load(&[
        file(
            "config.toml",
            CONFIG.replace(
                "copyright = \"© Site\"\nauthor = \"Site\"\n",
                "stripMetadata = false\n",
            ),
        ),
        file("content/a/index.md", page_file(&["author: 1815"], "")),
        file("content/a/photo.jpg", PHOTO),
    ]);
    let p = page(&s.model, PageKind::Page, "/a", 0);
    assert_eq!(s.credit(p), None);
}

/// A long value wrapped in YAML with `>-` is one line.
#[test]
fn folded_yaml_without_the_final_line_break_is_one_line() {
    let s = load(&[
        file("config.toml", CONFIG),
        file(
            "content/a/index.md",
            page_file(
                &["copyright: >-\n      © Ada Lovelace,\n      CC BY 4.0"],
                "",
            ),
        ),
        file("content/a/photo.jpg", PHOTO),
    ]);
    let p = page(&s.model, PageKind::Page, "/a", 0);
    assert_eq!(
        s.credit(p),
        Some((String::new(), "© Ada Lovelace, CC BY 4.0".to_owned()))
    );
}

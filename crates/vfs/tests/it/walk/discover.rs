//! Discovering a site's files: leaf bundles and duplicates, default mounts, several sites, and
//! content adapters.

use super::*;

/// (rel, key, kind, language key) of a discovered file.
pub(super) type Found = (String, String, BundleKind, String);

/// Every discovered file, and the dropped duplicates.
pub(super) fn discover(p: &Project) -> (Vec<Found>, Vec<PathBuf>) {
    let cfg = p.config();
    let found = p
        .vfs()
        .discover_content(&PathParser::from_config(&cfg))
        .unwrap();
    let files = found
        .files
        .iter()
        .map(|f| {
            (
                f.file.rel.clone(),
                f.info.key.as_str().to_owned(),
                f.info.kind,
                cfg.sites[f.lang].language.key.clone(),
            )
        })
        .collect();
    let dropped = found
        .duplicates
        .iter()
        .map(|d| d.dropped.strip_prefix(&p.dir).unwrap().to_path_buf())
        .collect();
    (files, dropped)
}

pub(super) fn rec(rel: &str, key: &str, kind: BundleKind, lang: &str) -> Found {
    (rel.to_owned(), key.to_owned(), kind, lang.to_owned())
}

#[test]
fn leaf_bundles_and_duplicates() {
    use BundleKind::{Branch, ContentResource, Leaf, Resource, Single};
    let p = Project::new(&[
        (
            "config.toml",
            "defaultContentLanguage = \"en\"\n[languages.en]\nweight = 1\n\
             [languages.th]\nweight = 2\n",
        ),
        ("content/_index.md", ""),
        ("content/b/index.md", ""),
        ("content/b/index.th.md", ""),
        ("content/b/_index.md", ""),
        ("content/b/notes.th.md", ""),
        ("content/b/img.jpg", ""),
        ("content/b/sub/index.md", ""),
        ("content/b/sub/data.json", ""),
        ("content/s/_index.md", ""),
        ("content/s/index.md", ""),
        ("content/s/p.md", ""),
        ("content/s/p.html", ""),
        ("content/s/x.md", ""),
        ("content/s/x/_index.md", ""),
        ("content/s/y.th.md", ""),
        ("content/s/y.md", ""),
        ("content/s/img.png", ""),
    ]);
    let (files, dropped) = discover(&p);
    assert_eq!(
        files,
        [
            rec("_index.md", "", Branch, "en"),
            rec("b/index.md", "b", Leaf, "en"),
            rec("b/index.th.md", "b", Leaf, "th"),
            rec("b/_index.md", "b/_index.md", ContentResource, "en"),
            rec("b/img.jpg", "b/img.jpg", Resource, "en"),
            rec("b/notes.th.md", "b/notes.md", ContentResource, "th"),
            rec("b/sub/data.json", "b/sub/data.json", Resource, "en"),
            rec("b/sub/index.md", "b/sub/index.md", ContentResource, "en"),
            rec("s/_index.md", "s", Branch, "en"),
            rec("s/img.png", "s/img.png", Resource, "en"),
            rec("s/p.md", "s/p", Single, "en"),
            rec("s/x/_index.md", "s/x", Branch, "en"),
            rec("s/y.md", "s/y", Single, "en"),
            rec("s/y.th.md", "s/y", Single, "th"),
        ]
    );
    // `_index` before `index` and a bundle before a single page; `md` before `html`.
    assert_eq!(
        dropped,
        [
            PathBuf::from("content/s/index.md"),
            PathBuf::from("content/s/p.html"),
            PathBuf::from("content/s/x.md"),
        ]
    );
}

#[test]
fn a_leaf_bundle_at_the_root_owns_everything() {
    let p = Project::new(&[
        ("config.toml", ""),
        ("content/index.md", ""),
        ("content/a.md", ""),
        ("content/s/_index.md", ""),
    ]);
    let (files, _) = discover(&p);
    let kinds: Vec<(&str, BundleKind)> = files.iter().map(|f| (f.1.as_str(), f.2)).collect();
    assert_eq!(
        kinds,
        [
            ("", BundleKind::Leaf),
            ("a.md", BundleKind::ContentResource),
            ("s/_index.md", BundleKind::ContentResource),
        ]
    );
}

#[test]
fn default_mounts_follow_the_dirs() {
    let p = Project::new(&[
        (
            "config.toml",
            "contentDir = \"c\"\nstaticDir = [\"s1\", \"s2\"]\n",
        ),
        ("c/a.md", ""),
        ("s1/x.txt", "1"),
        ("s2/x.txt", "2"),
        ("s2/y.txt", ""),
    ]);
    let vfs = p.vfs();
    assert_eq!(
        p.walk(&vfs, Component::Content),
        pairs(&[("a.md", "c/a.md")])
    );
    // Static: the later static dir wins (Go's static copy; this test expected `s1/x.txt`
    // while the vfs applied first-mount-wins to static too).
    assert_eq!(
        p.walk(&vfs, Component::Static),
        pairs(&[("x.txt", "s2/x.txt"), ("y.txt", "s2/y.txt")])
    );
    let open = vfs.open(Component::Static, "x.txt").map(|f| p.origin(&f));
    assert_eq!(open.as_deref(), Some("s2/x.txt"));
    let dir: &Path = &p.dir;
    assert!(vfs.mounts().iter().all(|m| m.abs.starts_with(dir)));
}

/// Discovery on whole sites: `FUGO_VFS_SITES=<dir>:<dir> cargo test -p ssg-vfs --
/// --ignored discover_sites` (sites from `cargo dev sites make <site> <dir>`).
#[test]
#[ignore = "needs FUGO_VFS_SITES"]
fn discover_sites() {
    let dirs = std::env::var("FUGO_VFS_SITES").expect("FUGO_VFS_SITES");
    for dir in dirs.split(':') {
        let cfg = load(&LoadOptions {
            source: PathBuf::from(dir),
            ..LoadOptions::default()
        })
        .unwrap();
        let vfs = Vfs::new(&cfg).unwrap();
        let found = vfs
            .discover_content(&PathParser::from_config(&cfg))
            .unwrap();
        let pages = found.files.iter().filter(|f| f.info.kind.is_page()).count();
        let walked: Vec<String> = [
            Component::Layouts,
            Component::Assets,
            Component::Data,
            Component::I18n,
            Component::Static,
        ]
        .iter()
        .map(|&c| format!("{c} {}", vfs.walk(c).unwrap().len()))
        .collect();
        eprintln!(
            "{dir}: {} mounts; content {} pages, {} resources, {} duplicates; {}",
            vfs.mounts().len(),
            pages,
            found.files.len() - pages,
            found.duplicates.len(),
            walked.join(", ")
        );
    }
}

/// Content adapters (`_content.html`, Go's `_content.gotmpl`) are listed apart from the
/// pages: they share their directory with the section's `_index.md`, one per directory and
/// language; a disabled language's adapter is dropped. `_content.html` is an adapter only in
/// the content component.
#[test]
fn content_adapters_are_apart() {
    let p = Project::new(&[
        (
            "config.toml",
            "defaultContentLanguage = \"en\"\n[languages.en]\nweight = 1\n\
             [languages.th]\nweight = 2\n[languages.fr]\nweight = 3\ndisabled = true\n",
        ),
        ("content/news/_index.md", ""),
        ("content/news/_content.html", ""),
        ("content/news/_content.gotmpl", ""),
        ("content/news/_content.th.html", ""),
        ("content/news/_content.fr.html", ""),
        ("content/_content.html", ""),
        ("content/docs/_content.md", ""),
    ]);
    let cfg = p.config();
    let found = p
        .vfs()
        .discover_content(&PathParser::from_config(&cfg))
        .unwrap();
    let adapters: Vec<(&str, &str, &str)> = found
        .adapters
        .iter()
        .map(|f| {
            assert_eq!(f.info.kind, BundleKind::ContentAdapter);
            (
                f.file.rel.as_str(),
                f.info.key.as_str(),
                cfg.sites[f.lang].language.key.as_str(),
            )
        })
        .collect();
    assert_eq!(
        adapters,
        [
            ("_content.html", "", "en"),
            ("news/_content.html", "news", "en"),
            ("news/_content.th.html", "news", "th"),
        ]
    );
    let dropped: Vec<PathBuf> = found
        .duplicates
        .iter()
        .map(|d| d.dropped.strip_prefix(&p.dir).unwrap().to_path_buf())
        .collect();
    assert_eq!(dropped, [PathBuf::from("content/news/_content.gotmpl")]);
    let pages: Vec<(&str, BundleKind)> = found
        .files
        .iter()
        .map(|f| (f.file.rel.as_str(), f.info.kind))
        .collect();
    assert_eq!(
        pages,
        [
            ("docs/_content.md", BundleKind::Single),
            ("news/_index.md", BundleKind::Branch)
        ]
    );

    let parser = PathParser::from_config(&cfg);
    let Parsed::File(layout) = parser.parse(Component::Layouts, "_content.html") else {
        panic!("a file");
    };
    assert_ne!(layout.kind, BundleKind::ContentAdapter);
}

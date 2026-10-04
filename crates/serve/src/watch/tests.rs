use super::*;

#[test]
fn events_are_mapped_to_the_watched_paths() {
    let aliases = RwLock::new(vec![
        (PathBuf::from("/private/var/t"), PathBuf::from("/var/t")),
        (
            PathBuf::from("/private/var/t/site/static"),
            PathBuf::from("/var/t/site/static"),
        ),
        (
            PathBuf::from("/real/theme"),
            PathBuf::from("/var/t/themes/x"),
        ),
    ]);
    let event = |path: &str| {
        Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Any))).add_path(path.into())
    };
    let mapped = |path: &str| as_watched(event(path), &aliases).paths;
    assert_eq!(
        mapped("/private/var/t/site/content/a.md"),
        [PathBuf::from("/var/t/site/content/a.md")]
    );
    assert_eq!(
        mapped("/private/var/t/site/static/b.css"),
        [PathBuf::from("/var/t/site/static/b.css")]
    );
    assert_eq!(
        mapped("/real/theme/layouts/home.html"),
        [PathBuf::from("/var/t/themes/x/layouts/home.html")]
    );
    // Paths no alias holds stay as they are (and `/private/var/tt` is not under `/private/var/t`).
    assert_eq!(mapped("/elsewhere/x"), [PathBuf::from("/elsewhere/x")]);
    assert_eq!(
        mapped("/private/var/tt/x"),
        [PathBuf::from("/private/var/tt/x")]
    );
}

#[test]
fn events_on_gone_paths_are_removals_on_macos() {
    let aliases = RwLock::new(Vec::new());
    let gone_path = std::env::temp_dir().join(format!("ssg-gone-{}", std::process::id()));
    for kind in [
        EventKind::Create(notify::event::CreateKind::File),
        EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Content)),
    ] {
        let e = as_watched(Event::new(kind).add_path(gone_path.clone()), &aliases);
        let want = if cfg!(target_os = "macos") {
            EventKind::Remove(RemoveKind::Any)
        } else {
            kind
        };
        assert_eq!(e.kind, want);
    }
    // An existing path keeps its event, and a rename is left to the debouncer.
    let here = std::env::temp_dir();
    let created = EventKind::Create(notify::event::CreateKind::Folder);
    assert_eq!(
        as_watched(Event::new(created).add_path(here), &aliases).kind,
        created
    );
    let renamed = EventKind::Modify(ModifyKind::Name(RenameMode::From));
    assert_eq!(
        as_watched(Event::new(renamed).add_path(gone_path), &aliases).kind,
        renamed
    );
}

#[test]
fn ignored_names() {
    for name in [
        ".config.toml.swp",
        "one.md~",
        "4913",
        "#one.md#",
        ".#one.md",
        "one.md.tmp",
        "one.md.swx",
        "a.css___jb_tmp___",
        "x.goutputstream-ABC",
        ".DS_Store",
        "one.md.sb-123",
    ] {
        assert!(is_ignored(Path::new(name)), "{name}");
    }
    for name in ["one.md", "site.css", "index.html", "4914", "a~b.md"] {
        assert!(!is_ignored(Path::new(name)), "{name}");
    }
}

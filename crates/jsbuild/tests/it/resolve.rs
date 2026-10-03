use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ssg_jsbuild::{AssetEntry, Assets, MountedDirs, resolve_component};

/// An in-memory assets tree: files map to `/real/<path>`, directories are implied.
fn resolve(files: &[&str], imp: &str) -> Option<String> {
    let tree: BTreeMap<&str, ()> = files.iter().map(|f| (*f, ())).collect();
    let entry = |p: &str| {
        if tree.contains_key(p) {
            Some(AssetEntry::File(PathBuf::from(format!("/real/{p}"))))
        } else if tree.keys().any(|f| f.starts_with(&format!("{p}/"))) {
            Some(AssetEntry::Dir)
        } else {
            None
        }
    };
    resolve_component(imp, entry).map(|p| p.to_string_lossy().into_owned())
}

#[test]
fn component_resolution_order() {
    let r = |files: &[&str], imp| resolve(files, imp);
    // Extensions in order .js, .ts, .tsx, .jsx; a file without an extension comes after them.
    assert_eq!(
        r(&["a.ts", "a.js", "a"], "a").as_deref(),
        Some("/real/a.js")
    );
    assert_eq!(r(&["a.tsx", "a"], "a").as_deref(), Some("/real/a.tsx"));
    assert_eq!(r(&["a"], "a").as_deref(), Some("/real/a"));
    // foo.js next to foo/index.js: the file wins.
    assert_eq!(
        r(&["foo.js", "foo/index.js"], "foo").as_deref(),
        Some("/real/foo.js")
    );
    // A directory gives its index, then index.esm.
    assert_eq!(
        r(&["foo/index.ts"], "foo").as_deref(),
        Some("/real/foo/index.ts")
    );
    assert_eq!(
        r(&["foo/index.esm.js"], "foo").as_deref(),
        Some("/real/foo/index.esm.js")
    );
    assert_eq!(
        r(&["foo/index.esm.js"], "foo/index").as_deref(),
        Some("/real/foo/index.esm.js")
    );
    // An extension already present is not appended again; `.js` falls back to other sources.
    assert_eq!(r(&["x.js.js"], "x.js"), None);
    assert_eq!(r(&["x.ts"], "x.js").as_deref(), Some("/real/x.ts"));
    assert_eq!(r(&["x.js"], "x.js").as_deref(), Some("/real/x.js"));
    assert_eq!(r(&[], "x"), None);
}

#[test]
fn mounted_dirs() {
    let root = crate::scratch("mounted-dirs");
    let assets = root.join("assets");
    let vendor = root.join("node_modules");
    for f in [assets.join("js/a.js"), vendor.join("pkg/index.js")] {
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, "x").unwrap();
    }
    let m = MountedDirs::new()
        .mount(&assets, "")
        .mount(&vendor, "vendor");
    assert_eq!(
        m.entry("js/a.js"),
        Some(AssetEntry::File(assets.join("js/a.js")))
    );
    assert_eq!(
        m.entry("./js/../js/a.js"),
        Some(AssetEntry::File(assets.join("js/a.js")))
    );
    assert_eq!(m.entry("js"), Some(AssetEntry::Dir));
    assert_eq!(m.entry("vendor"), Some(AssetEntry::Dir));
    assert_eq!(
        m.entry("vendor/pkg/index.js"),
        Some(AssetEntry::File(vendor.join("pkg/index.js")))
    );
    assert_eq!(m.entry("../assets/js/a.js"), None);
    assert_eq!(m.entry("js/b.js"), None);
    assert_eq!(
        m.assets_path(&assets.join("js/a.js")).as_deref(),
        Some("js/a.js")
    );
    assert_eq!(
        m.assets_path(&vendor.join("pkg/index.js")).as_deref(),
        Some("vendor/pkg/index.js")
    );
    assert_eq!(m.assets_path(Path::new("/elsewhere/x.js")), None);
}

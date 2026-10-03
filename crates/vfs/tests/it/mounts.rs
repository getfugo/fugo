//! The effective mounts against the Go oracle `oracle/allconfig/load/mounts.json.gz`
//! (`modules[0].mounts` after `Configs.Init`): defaults for unconfigured components, missing
//! sources, per-language `contentDir`/`staticDir`, multihost, legacy `*Dir` keys, the JS config
//! files, duplicates.

use std::fs;
use std::path::Path;

use serde_json::{Value as J, json};
use ssg_config::{LoadOptions, load};
use ssg_testkit::fixture::oracle;
use ssg_vfs::{Module, Mount, Vfs};

/// Writes the case's files (`name/` is a directory) below `root/site`.
/// The Go program's JS package file and the Go build's stats file, as the recorded Go data names
/// them.
const GO_JS_PACKAGE: &str = "package.hugo.json";
const GO_STATS: &str = "hugo_stats.json";

/// Whether Go kept this mount only because it writes the stats file: it kept a mount of a
/// missing stats file, which this port (writing no such file) skips like any missing source.
fn go_stats_placeholder(m: &J, root: &str) -> bool {
    let source = m["source"]
        .as_str()
        .unwrap_or_default()
        .replace("$ROOT", root);
    let path = Path::new(root).join("site").join(&source);
    source.ends_with(GO_STATS) && !path.exists()
}

fn write_case(root: &Path, files: &serde_json::Map<String, J>) {
    let site = root.join("site");
    fs::create_dir_all(&site).unwrap();
    let root_str = root.to_str().unwrap();
    // The Go program's configuration files and its JS package file (names of the recorded Go
    // data) are our `config.*` and `package.config.json`.
    for (name, content) in files {
        let name = name.replace(GO_JS_PACKAGE, "package.config.json");
        let p = site.join(ssg_testkit::fixture::local_path(&name));
        if name.ends_with('/') {
            fs::create_dir_all(&p).unwrap();
        } else {
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            let text = content.as_str().unwrap().replace("$ROOT", root_str);
            fs::write(&p, text).unwrap();
        }
    }
}

fn dump(m: &Mount, root: &str) -> J {
    let list = |v: &[String]| {
        if v.is_empty() {
            "null".to_owned()
        } else {
            serde_json::to_string(v).unwrap()
        }
    };
    json!({
        "source": m.source.replace(root, "$ROOT"),
        "target": m.target,
        "lang": m.lang.clone().unwrap_or_default(),
        "includeFiles": list(&m.include_files),
        "excludeFiles": list(&m.exclude_files),
        "disableWatch": m.disable_watch,
    })
}

/// The JS config mounts are compared as a set (Go lists them in directory order).
fn split(mounts: Vec<J>) -> (Vec<J>, Vec<String>) {
    let (js, rest): (Vec<J>, Vec<J>) = mounts.into_iter().partition(|m| {
        m["target"]
            .as_str()
            .unwrap()
            .starts_with("assets/_jsconfig")
    });
    let mut js: Vec<String> = js.iter().map(canonical).collect();
    js.sort();
    (rest, js)
}

/// A mount as JSON text with sorted keys (serde_json's `Map` keeps insertion order in this
/// workspace, see ssg-funcs' determinism test).
fn canonical(m: &J) -> String {
    serde_json::to_string(&ssg_base::Value::from_json(m.clone())).unwrap()
}

/// Loads the case written below `root` and sets up its mounts.
fn load_case(root: &Path) -> Result<Vfs, String> {
    let options = LoadOptions {
        source: root.join("site"),
        env: vec![
            ("HOME".into(), root.join("home").to_str().unwrap().into()),
            (
                "XDG_CACHE_HOME".into(),
                root.join("xdg").to_str().unwrap().into(),
            ),
        ],
        ..LoadOptions::default()
    };
    load(&options)
        .map_err(|e| e.to_string())
        .and_then(|cfg| Vfs::new(&cfg).map_err(|e| e.to_string()))
}

#[test]
fn mounts_match_go() {
    let f: J = oracle("oracle/allconfig/load/mounts.json.gz");
    let cases = f["cases"].as_array().unwrap();
    let mut checked = 0;
    for c in cases {
        let name = c["case"]["name"].as_str().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_case(root, c["case"]["files"].as_object().unwrap());
        let got = load_case(root);
        if let Some(err) = c["result"].get("err") {
            assert!(got.is_err(), "{name}: want an error like {err}");
            checked += 1;
            continue;
        }
        let vfs = got.unwrap_or_else(|e| panic!("{name}: {e}"));
        let root_str = root.to_str().unwrap();
        let ours: Vec<J> = vfs
            .mounts()
            .iter()
            .filter(|m| m.module == Module::Project)
            .map(|m| dump(m, root_str))
            .collect();
        let mut want: Vec<J> = serde_json::from_str(
            &c["result"]["modules"][0]["mounts"]
                .to_string()
                .replace(GO_JS_PACKAGE, "package.config.json"),
        )
        .unwrap();
        want.retain(|m| !go_stats_placeholder(m, root_str));
        let (ours, ours_js) = split(ours);
        let (want, mut want_js) = split(want);
        // The Go program also mounted the PostCSS, Babel and Tailwind configuration files by
        // default; this port runs none of those tools (a site's own mount of them stays).
        want_js.retain(|m| {
            let m: J = serde_json::from_str(m).unwrap();
            let source = m["source"].as_str().unwrap_or_default();
            let default_mount = m["target"] == format!("assets/_jsconfig/{source}").as_str();
            !(default_mount
                && ["postcss", "babel", "tailwind"]
                    .iter()
                    .any(|t| source.contains(&format!("{t}.config.js"))))
        });
        assert_eq!(ours, want, "{name}");
        assert_eq!(ours_js, want_js, "{name}: JS config mounts");
        checked += 1;
    }
    assert_eq!(checked, cases.len());
    eprintln!("mounts: {checked} cases equal");
}

/// The themes against the oracle's modules after the project (`modules[1..]` of
/// `oracle/allconfig/load/{themes,merge}.json.gz`): the order (nested imports depth first,
/// duplicates and disabled imports skipped), each theme's directory (themes directory,
/// `themesDir`, `_vendor`, absolute and replaced paths) and its mounts (the component
/// directories it has, its own or its import's `[[module.mounts]]`, `noMounts`, JS config
/// files). The cases Go fails fail here too.
#[test]
fn theme_mounts_match_go() {
    let mut themes = 0;
    for group in ["themes", "merge"] {
        let f: J = oracle(&format!("oracle/allconfig/load/{group}.json.gz"));
        for c in f["cases"].as_array().unwrap() {
            let name = c["case"]["name"].as_str().unwrap();
            if c["case"]["ignoreModuleDoesNotExist"].as_bool() == Some(true) {
                // The Go program's module commands only: a missing theme is an error here.
                continue;
            }
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path();
            write_case(root, c["case"]["files"].as_object().unwrap());
            let got = load_case(root);
            if let Some(err) = c["result"].get("err") {
                assert!(got.is_err(), "{name}: want an error like {err}");
                continue;
            }
            let vfs = got.unwrap_or_else(|e| panic!("{name}: {e}"));
            let root_str = root.to_str().unwrap();
            let want = &c["result"]["modules"].as_array().unwrap()[1..];
            let cfg_themes = load(&LoadOptions {
                source: root.join("site"),
                env: vec![(
                    "XDG_CACHE_HOME".into(),
                    root.join("xdg").to_str().unwrap().into(),
                )],
                ..LoadOptions::default()
            })
            .unwrap()
            .themes;
            let paths: Vec<String> = cfg_themes
                .iter()
                .map(|t| t.path.replace(root_str, "$ROOT"))
                .collect();
            let want_paths: Vec<&str> = want.iter().map(|m| m["path"].as_str().unwrap()).collect();
            assert_eq!(paths, want_paths, "{name}: theme order");
            for (i, (theme, w)) in cfg_themes.iter().zip(want).enumerate() {
                let dir = theme.dir.to_str().unwrap().replace(root_str, "$ROOT");
                let what = format!("{name}: {}", theme.path);
                assert_eq!(
                    dir,
                    w["dir"].as_str().unwrap().trim_end_matches('/'),
                    "{what}"
                );
                if w["vendor"].as_bool() == Some(true) {
                    assert_eq!(theme.vendored.as_deref(), w["version"].as_str(), "{what}");
                } else {
                    // For a vendored theme Go names the owner of the `_vendor` directory.
                    assert_eq!(
                        theme.owner.as_deref().unwrap_or("project"),
                        w["owner"].as_str().unwrap(),
                        "{what}: owner"
                    );
                }
                let module = Module::Theme(u16::try_from(i).unwrap());
                let ours: Vec<J> = vfs
                    .mounts()
                    .iter()
                    .filter(|m| m.module == module)
                    .map(|m| dump(m, root_str))
                    .collect();
                let (ours, ours_js) = split(ours);
                let (want_mounts, want_js) =
                    split(w["mounts"].as_array().cloned().unwrap_or_default());
                assert_eq!(ours, want_mounts, "{what}: mounts");
                assert_eq!(ours_js, want_js, "{what}: JS config mounts");
                themes += 1;
            }
        }
    }
    assert!(themes >= 20, "{themes} themes compared");
    eprintln!("theme mounts: {themes} themes equal");
}

//! `docs/data/objects.toml`, the template objects of the documentation's reference, describes
//! exactly the fields of the views (`src/views.rs` and `src/views/`): every serialised field of
//! the structs an object lists has a description, and every description names such a field.

use std::collections::{BTreeMap, BTreeSet};

/// The serialised field names of every `pub struct` of `src`: `r#type` is `type`, and a
/// `#[serde(rename = "…")]` before a field gives its name.
fn struct_fields(src: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut out = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut rename: Option<String> = None;
    for line in src.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("pub struct ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if t.ends_with('{') {
                out.insert(name.clone(), BTreeSet::new());
                current = Some(name);
            }
            continue;
        }
        let Some(name) = &current else { continue };
        if t == "}" {
            current = None;
            continue;
        }
        if let Some(r) = t.strip_prefix("#[serde(rename = \"") {
            rename = r.split('"').next().map(str::to_owned);
            continue;
        }
        if let Some(field) = t.strip_prefix("pub ") {
            let field = field.split(':').next().unwrap_or_default();
            let field = field.strip_prefix("r#").unwrap_or(field);
            let field = rename.take().unwrap_or_else(|| field.to_owned());
            out.get_mut(name).expect("open struct").insert(field);
        }
    }
    out
}

#[test]
fn docs_objects_cover_the_views() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![dir.join("views.rs")];
    let mut modules: Vec<_> = std::fs::read_dir(dir.join("views"))
        .expect("src/views")
        .map(|e| e.expect("directory entry").path())
        .collect();
    modules.sort();
    files.extend(modules);
    let src: String = files
        .iter()
        .map(|f| std::fs::read_to_string(f).unwrap_or_else(|e| panic!("{}: {e}", f.display())))
        .collect();
    let structs = struct_fields(&src);
    let path = ssg_testkit::fixture::repo_dir().join("docs/data/objects.toml");
    let doc: toml::Table = std::fs::read_to_string(&path)
        .expect("docs/data/objects.toml")
        .parse()
        .expect("valid TOML");
    let mut problems = Vec::new();
    for (object, entry) in &doc {
        let names: Vec<&str> = entry["structs"]
            .as_array()
            .expect("structs")
            .iter()
            .map(|v| v.as_str().expect("a struct name"))
            .collect();
        let mut want = BTreeSet::new();
        for n in &names {
            match structs.get(*n) {
                Some(f) => want.extend(f.iter().cloned()),
                None => problems.push(format!("{object}: no struct {n} in the views")),
            }
        }
        let have: BTreeSet<String> = entry["fields"]
            .as_table()
            .expect("fields")
            .keys()
            .cloned()
            .collect();
        for f in want.difference(&have) {
            problems.push(format!("{object}: field `{f}` has no description"));
        }
        for f in have.difference(&want) {
            problems.push(format!(
                "{object}: `{f}` is not a field of {}",
                names.join(", ")
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

//! The structure dump: its records, and the structure oracle (S).

use super::*;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Structure {
    pub records: Vec<Record>,
    pub aliases: Vec<AliasRecord>,
    pub pager_aliases: Vec<AliasRecord>,
    pub resources: Vec<ResourceRecord>,
    pub pages: Vec<PageRecord>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Record {
    pub lang: String,
    pub path: String,
    pub kind: String,
    pub format: String,
    pub target: String,
    pub rel_permalink: String,
    pub permalink: String,
    pub template: String,
    pub template_file: Option<String>,
    pub baseof: String,
    pub baseof_file: Option<String>,
    pub written: Option<bool>,
    pub pagers: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AliasRecord {
    pub from: String,
    pub lang: String,
    pub path: String,
    pub format: String,
    pub permalink: String,
    pub kind: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ResourceRecord {
    pub lang: String,
    pub path: String,
    pub name: String,
    pub rel_permalink: String,
    pub target: Option<String>,
    pub targets: Option<Vec<String>>,
    pub publish: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PageRecord {
    pub lang: String,
    pub path: String,
    pub kind: String,
    pub outputs: Vec<String>,
}

/// A template's name, marked when it is one of the embedded templates.
pub(super) fn template_id(name: &str, file: Option<&str>) -> String {
    if name.is_empty() {
        return String::new();
    }
    if file.unwrap_or(name).starts_with("_embedded/") {
        format!("{name} (embedded)")
    } else {
        name.to_owned()
    }
}

impl Structure {
    /// Every compared fact, by key.
    #[must_use]
    pub fn facts(&self) -> BTreeMap<String, Value> {
        let mut out = BTreeMap::new();
        for r in &self.records {
            out.insert(
                format!("record {} {} {} {}", r.lang, r.path, r.kind, r.format),
                json!({
                    "target": r.target,
                    "relPermalink": r.rel_permalink,
                    "permalink": r.permalink,
                    "template": template_id(&r.template, r.template_file.as_deref()),
                    "baseof": template_id(&r.baseof, r.baseof_file.as_deref()),
                    "written": r.written.unwrap_or(true),
                    "pagers": r.pagers,
                }),
            );
        }
        for (name, aliases) in [("alias", &self.aliases), ("pager", &self.pager_aliases)] {
            for a in aliases {
                let mut v = json!({ "permalink": a.permalink });
                if name == "alias" {
                    v["kind"] = json!(a.kind);
                }
                out.insert(
                    format!("{name} {} {} {} {}", a.from, a.lang, a.path, a.format),
                    v,
                );
            }
        }
        for r in &self.resources {
            let targets = match (&r.targets, &r.target) {
                (Some(t), _) if !t.is_empty() => t.clone(),
                (_, Some(t)) if !t.is_empty() => vec![t.clone()],
                _ => Vec::new(),
            };
            out.insert(
                format!("resource {} {} {}", r.lang, r.path, r.name),
                json!({ "relPermalink": r.rel_permalink, "targets": targets, "publish": r.publish.unwrap_or(true) }),
            );
        }
        for p in &self.pages {
            out.insert(
                format!("page {} {} {}", p.lang, p.path, p.kind),
                json!({ "outputs": p.outputs }),
            );
        }
        out
    }

    /// The directories of targets that more than one (page, format) record claims.
    #[must_use]
    pub fn collision_dirs(&self) -> BTreeSet<String> {
        let mut count: HashMap<&str, usize> = HashMap::new();
        for r in &self.records {
            *count.entry(r.target.as_str()).or_default() += 1;
        }
        count
            .into_iter()
            .filter(|(_, n)| *n > 1)
            .filter_map(|(t, _)| {
                t.trim_matches('/')
                    .rsplit_once('/')
                    .map(|(d, _)| format!("{d}/"))
            })
            .collect()
    }
}

pub(super) fn compare_structure(r: &Structure, c: &Structure) -> Results {
    let (rf, cf) = (r.facts(), c.facts());
    let keys: BTreeSet<&String> = rf.keys().chain(cf.keys()).collect();
    let mut out = Results::new();
    for key in keys {
        let what = key.split(' ').next().unwrap_or("");
        let outcome = match (rf.get(key), cf.get(key)) {
            (Some(rv), None) => Outcome::new(
                Status::Missing,
                [format!("S {what} missing")],
                "only in the reference".into(),
                &json!(["missing", rv]),
            ),
            (None, Some(cv)) => Outcome::new(
                Status::Extra,
                [format!("S {what} extra")],
                "only in the candidate".into(),
                &json!(["extra", cv]),
            ),
            (Some(rv), Some(cv)) if rv != cv => {
                let (ro, co) = (
                    rv.as_object().cloned().unwrap_or_default(),
                    cv.as_object().cloned().unwrap_or_default(),
                );
                let fields: BTreeSet<&String> = ro
                    .keys()
                    .chain(co.keys())
                    .filter(|f| ro.get(*f) != co.get(*f))
                    .collect();
                let detail: Vec<String> = fields
                    .iter()
                    .map(|f| {
                        format!(
                            "{f} {} -> {}",
                            ro.get(*f).unwrap_or(&Value::Null),
                            co.get(*f).unwrap_or(&Value::Null)
                        )
                    })
                    .collect();
                Outcome::new(
                    Status::Diff,
                    fields.iter().map(|f| format!("S {what} {f}")),
                    detail.join("; "),
                    &json!(["diff", rv, cv]),
                )
            }
            _ => Outcome::ok(),
        };
        out.entry(key.clone())
            .or_default()
            .insert(Level::S, outcome);
    }
    out
}

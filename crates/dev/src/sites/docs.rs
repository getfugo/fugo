//! The legacy docs site (testdata/legacy-docs): the offline patch variants
//! (docs/rust-port/REWRITE_PLAN.md §7.3). The Go build always built the Go-template patches. A
//! patch of a file below layouts/ has a Tera counterpart at sites/docs/patches/<variant>/<same
//! path> for every variant it belongs to (`sites patches` checks the 1:1 correspondence); all
//! other patches change the site input both builds share.

use super::*;

/// patches.json.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Patches {
    pub schema: String,
    pub about: String,
    pub variants: Vec<String>,
    /// In the order they are applied.
    pub patches: Vec<Patch>,
}

/// An edit of the docs site.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Patch {
    pub file: String,
    #[serde(flatten)]
    pub edit: Edit,
    /// The variants it belongs to.
    pub variants: Vec<String>,
    pub why: String,
    /// The file below sites/docs/patches/<variant>/ that mirrors a layout patch in the Tera
    /// overlay (none: the patch changes the site input both builds share).
    pub tera: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Edit {
    Remove,
    /// The one occurrence of `old` replaced with `new`.
    Replace {
        old: String,
        new: String,
    },
    Write {
        content: String,
    },
}

impl Patch {
    pub(super) fn in_variant(&self, variant: &str) -> bool {
        self.variants.iter().any(|v| v == variant)
    }
}

impl Patches {
    /// tools/rust-port/i01/patches.json.
    ///
    /// # Errors
    /// A missing or invalid patches.json.
    pub fn read() -> Result<Patches, Fail> {
        json::read(&patches_json())
    }

    /// The file in its canonical form: sorted keys, one patch per line.
    #[must_use]
    pub fn to_text(&self) -> String {
        let doc = serde_json::to_value(self).expect("patches serialize");
        let mut lines: Vec<String> = doc
            .as_object()
            .expect("an object")
            .iter()
            .filter(|(k, _)| *k != "patches")
            .map(|(k, v)| format!("{}: {}", json::string(k), json::line(v)))
            .collect();
        let patches: Vec<String> = self.patches.iter().map(json::line).collect();
        lines.push(format!("\"patches\": [\n{}\n]", patches.join(",\n")));
        lines.sort();
        format!("{{\n{}\n}}\n", lines.join(",\n"))
    }
}

/// The errors of patches.json: not in its canonical form, a `tera` field that is not the file
/// of a layout patch, or Tera patch files that do not correspond 1:1 to the layout patches.
///
/// # Errors
/// A missing or invalid patches.json.
pub fn check_patches() -> Result<Vec<String>, Fail> {
    let path = patches_json();
    let text = std::fs::read_to_string(&path).map_err(io(&path))?;
    let doc = Patches::read()?;
    let mut errors = Vec::new();
    if text != doc.to_text() {
        errors.push(format!(
            "{} is not in its canonical form (run `cargo dev sites patches`)",
            path.display()
        ));
    }
    for p in &doc.patches {
        let want = p.file.starts_with("layouts/").then(|| p.file.clone());
        if p.tera != want {
            errors.push(format!("{}: `tera` must be {want:?}", p.file));
        }
    }
    let root = tera_patches();
    for v in DOCS_VARIANTS {
        let mut want: Vec<&String> = doc
            .patches
            .iter()
            .filter(|p| p.in_variant(v))
            .filter_map(|p| p.tera.as_ref())
            .collect();
        want.sort();
        want.dedup();
        let vdir = root.join(v);
        let have = if vdir.is_dir() {
            manifest::walk(&vdir)
        } else {
            Vec::new()
        };
        errors.extend(
            want.iter()
                .filter(|f| !have.contains(f))
                .map(|f| format!("{v}: no Tera patch file for {f}")),
        );
        errors.extend(
            have.iter()
                .filter(|f| !want.contains(f))
                .map(|f| format!("{v}: Tera patch file {f} has no entry in patches.json")),
        );
    }
    if root.is_dir() {
        let mut extra: Vec<String> = std::fs::read_dir(&root)
            .map_err(io(&root))?
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| !DOCS_VARIANTS.contains(&n.as_str()))
            .collect();
        extra.sort();
        errors.extend(
            extra
                .into_iter()
                .map(|e| format!("{}/{e}: not a variant", root.display())),
        );
    }
    Ok(errors)
}

pub(super) fn make_docs(dir: &Path, variant: &str) -> Result<(), Fail> {
    if !DOCS_VARIANTS.contains(&variant) {
        return Err(fail!(
            "unknown docs patch variant {variant:?} (one of {})",
            DOCS_VARIANTS.join(", ")
        ));
    }
    copy_site(&testdata().join("legacy-docs"), dir)?;
    as_local_site(dir)?;
    for p in Patches::read()?
        .patches
        .iter()
        .filter(|p| p.in_variant(variant))
    {
        match &p.edit {
            Edit::Remove => {
                let path = dir.join(&p.file);
                if path.exists() {
                    std::fs::remove_file(&path).map_err(io(&path))?;
                }
            }
            Edit::Replace { old, new } => edit(dir, &p.file, old, new)?,
            Edit::Write { content } => write(dir, &p.file, content.as_bytes())?,
        }
    }
    Ok(())
}

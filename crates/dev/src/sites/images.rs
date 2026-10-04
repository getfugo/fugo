//! images: the recipes of testdata/golden/images/manifest.json (the golden images of the PSNR
//! gate of T41) as a site whose home page runs every recipe with Go's image processing and
//! prints `<golden name> <RelPermalink>` per line (tools/dev/oracle.sh, frozen at 44529028,
//! copied the published files into testdata/golden/images).

use super::*;

#[derive(Deserialize)]
pub(super) struct Recipe {
    pub(super) golden: String,
    pub(super) source: String,
    pub(super) steps: Vec<Step>,
    #[serde(default)]
    pub(super) imaging: Option<Value>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum Step {
    Spec {
        spec: String,
    },
    Filters {
        filters: Vec<serde_json::Map<String, Value>>,
    },
}

/// The filters of the recipes (the JSON of `ssg_images::ImageFilter`) as Go template calls:
/// the images.* function and the keys of its arguments.
pub(super) const FILTER_ARGS: [(&str, &str, &[&str]); 16] = [
    ("brightness", "Brightness", &["percentage"]),
    ("contrast", "Contrast", &["percentage"]),
    ("gamma", "Gamma", &["gamma"]),
    ("gaussian_blur", "GaussianBlur", &["sigma"]),
    ("grayscale", "Grayscale", &[]),
    ("hue", "Hue", &["shift"]),
    ("invert", "Invert", &[]),
    ("colorize", "Colorize", &["hue", "saturation", "percentage"]),
    ("color_balance", "ColorBalance", &["r", "g", "b"]),
    ("saturation", "Saturation", &["percentage"]),
    ("sepia", "Sepia", &["percentage"]),
    ("sigmoid", "Sigmoid", &["midpoint", "factor"]),
    (
        "unsharp_mask",
        "UnsharpMask",
        &["sigma", "amount", "threshold"],
    ),
    ("pixelate", "Pixelate", &["size"]),
    ("opacity", "Opacity", &["opacity"]),
    ("auto_orient", "AutoOrient", &[]),
];

/// A filter argument as a Go template literal: a string quoted, a number as an integer or a
/// float with a fraction.
pub(super) fn go_value(v: &Value) -> Result<String, Fail> {
    match v {
        Value::String(s) => Ok(json::string(s)),
        Value::Number(n) if n.is_i64() || n.is_u64() => Ok(n.to_string()),
        Value::Number(n) => n
            .as_f64()
            .map(|f| format!("{f:?}"))
            .ok_or_else(|| fail!("images: number {n}")),
        other => Err(fail!("images: unsupported filter argument {other}")),
    }
}

pub(super) struct Images<'a> {
    pub(super) dir: &'a Path,
    /// Repository path -> assets path.
    pub(super) files: Vec<(String, String)>,
}

impl Images<'_> {
    pub(super) fn asset(&mut self, repo_path: &str) -> Result<String, Fail> {
        let name = if let Some((_, a)) = self.files.iter().find(|(r, _)| r == repo_path) {
            a.clone()
        } else {
            let ext = Path::new(repo_path)
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
                .unwrap_or_default();
            let a = format!("g/{:02}{ext}", self.files.len());
            let src = repo_file(repo_path);
            write(
                self.dir,
                &format!("assets/{a}"),
                &std::fs::read(&src).map_err(io(&src))?,
            )?;
            self.files.push((repo_path.to_owned(), a.clone()));
            a
        };
        Ok(format!("(resources.Get \"{name}\")"))
    }

    pub(super) fn filter_call(
        &mut self,
        f: &serde_json::Map<String, Value>,
    ) -> Result<String, Fail> {
        let op = f
            .get("op")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let arg = |k: &str| {
            f.get(k)
                .ok_or_else(|| fail!("images: filter {op} without {k}"))
                .and_then(go_value)
        };
        let zero = Value::from(0);
        if let Some((_, name, keys)) = FILTER_ARGS.iter().find(|(o, _, _)| *o == op) {
            let mut parts = vec![format!("images.{name}")];
            for k in *keys {
                parts.push(arg(k)?);
            }
            return Ok(parts.join(" "));
        }
        match op.as_str() {
            "padding" => {
                let margin: Vec<&Value> = match f
                    .get("margin")
                    .and_then(Value::as_array)
                    .filter(|m| !m.is_empty())
                {
                    Some(m) => m.iter().collect(),
                    None => ["top", "right", "bottom", "left"]
                        .iter()
                        .map(|k| f.get(*k).unwrap_or(&zero))
                        .collect(),
                };
                let mut parts = vec!["images.Padding".to_owned()];
                for m in margin {
                    parts.push(go_value(m)?);
                }
                if let Some(c) = f.get("color") {
                    parts.push(go_value(c)?);
                }
                Ok(parts.join(" "))
            }
            "overlay" => {
                let image =
                    self.asset(f.get("image").and_then(Value::as_str).unwrap_or_default())?;
                let (x, y) = (f.get("x").unwrap_or(&zero), f.get("y").unwrap_or(&zero));
                Ok(format!(
                    "images.Overlay {image} {} {}",
                    go_value(x)?,
                    go_value(y)?
                ))
            }
            "mask" => Ok(format!(
                "images.Mask {}",
                self.asset(f.get("image").and_then(Value::as_str).unwrap_or_default())?
            )),
            "process" => Ok(format!("images.Process {}", arg("spec")?)),
            _ => Err(fail!("images: unsupported filter {op:?}")),
        }
    }
}

pub(super) fn make_images(dir: &Path) -> Result<(), Fail> {
    let recipes: Vec<Recipe> = json::read(&testdata().join("golden/images/manifest.json"))?;
    let mut im = Images {
        dir,
        files: Vec::new(),
    };
    let mut lines = Vec::new();
    for r in &recipes {
        if r.imaging.as_ref().is_some_and(|i| !i.is_null()) {
            return Err(fail!(
                "images: {}: a recipe's own [imaging] is not supported (one site)",
                r.golden
            ));
        }
        lines.push(format!("{{{{- $r := {} }}}}", im.asset(&r.source)?));
        for step in &r.steps {
            match step {
                Step::Spec { spec } => {
                    lines.push(format!("{{{{- $r = $r.Process {} }}}}", json::string(spec)));
                }
                Step::Filters { filters } => {
                    let calls = filters
                        .iter()
                        .map(|f| im.filter_call(f).map(|c| format!("({c})")))
                        .collect::<Result<Vec<_>, _>>()?;
                    lines.push(format!(
                        "{{{{- $r = $r | images.Filter (slice {}) }}}}",
                        calls.join(" ")
                    ));
                }
            }
        }
        lines.push(format!("{} {{{{ $r.RelPermalink }}}}", r.golden));
    }
    write(
        dir,
        "config.toml",
        b"baseURL = \"https://example.org/\"\ndisableKinds = [\"page\", \"section\", \"taxonomy\", \"term\", \"rss\", \"sitemap\", \"robotsTXT\", \"404\"]\n[outputs]\nhome = [\"html\"]\n",
    )?;
    write(
        dir,
        "layouts/home.html",
        format!("{}\n", lines.join("\n")).as_bytes(),
    )
}

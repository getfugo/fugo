//! The end-to-end sites, written into a directory outside the repository.
//!
//! - `docs`: the legacy docs site (testdata/legacy-docs, the Go tree's docs/), patched to build
//!   offline; the patch variant (i01, the default, reduced, or live: unpatched; see [`I01`],
//!   [`REDUCED`], [`LIVE`]) is chosen with `--docs-patches` or by the names docs-i01,
//!   docs-reduced and docs-live;
//! - `testsite`: the Go tree's testsite (testdata/upstream) plus a small config and layouts
//!   (tools/rust-port/i01/testsite.txtar);
//! - `mini`: the e2e oracle's small en/th site (testdata/oracle/commands/e2e/mini.txtar);
//! - `images`: the golden image recipes (testdata/golden/images/manifest.json) as a site;
//! - `errors`: a failing build (tools/rust-port/i01/errors.txtar): the error texts must be Go's;
//! - `probe`: T13's template probe site;
//! - `t24-<name>`: the T24 build-oracle sites (testdata/oracle/sitebuild/build/<name>.json.gz).
//!
//! `--overlay` makes the input of the Rust build: the same site with its layouts replaced by
//! the Tera layouts of the overlay directory, the overlay's assets copied over, its content
//! adapters (content/**/_content.html) replacing the site's Go-template ones
//! (`_content.gotmpl`; an adapter whose Go original the patches removed is not copied), and
//! for docs the variant's Tera patch files (sites/docs/patches/<variant>/, if it has any)
//! layered on top (REWRITE_PLAN.md §7.4). The Go build that wrote the golden data built the
//! site without an overlay (tools/dev/oracle.sh, frozen at 44529028).
//!
//! Files are copied with their permissions and times, as the golden builds' inputs were.

use std::path::{Component, Path, PathBuf};

use crate::py::{self, Py};
use crate::{Fail, fail, manifest, txtar};

/// The I01 site: offline, no Chroma, passthrough, emoji, Tailwind or node modules (acceptance
/// gate A-D1).
pub const I01: &str = "i01";
/// Offline, with Chroma highlighting, passthrough, emoji, remarshal, Tailwind (this port
/// publishes the stylesheet Tailwind built, sites/docs/assets/css) and the real Alpine/Turbo
/// imports (node.sh modules; gate A-D2).
pub const REDUCED: &str = "reduced";
/// The docs site as getfugo.github.io publishes it: no patches (only the committed stats file
/// of the Go build goes), so GetRemote, images.Text, QR, Dither, smartcrop, the x shortcode, the
/// style gallery and the news content adapter all run (gate A-D3: the golden data is the
/// published site, testdata/golden/docs-live/).
pub const LIVE: &str = "live";
pub const DOCS_VARIANTS: [&str; 3] = [I01, REDUCED, LIVE];

/// The Go program's name, as the site inputs it wrote or read carry it: their configuration
/// file (`<name>.toml`) and the prefix of their environment variables (a recorded literal).
const GO_NAME: &str = "hugo";
/// The Go tree's test data by its path there, as the fixtures record it, moved to
/// testdata/upstream (the sites use only these; `ssg_testkit::fixture::UPSTREAM` lists all).
const UPSTREAM: [&str; 1] = ["testsite"];
/// The workspace's directory until it moved to the repository root; paths recorded below it
/// (golden/images/manifest.json) name the same files at the root.
const LEGACY_WORKSPACE: &str = "rust/";

fn testdata() -> PathBuf {
    crate::root().join("testdata")
}

fn i01_dir() -> PathBuf {
    crate::root().join("tools/rust-port/i01")
}

/// tools/rust-port/i01/patches.json: the edits of the docs site per variant, in the order they
/// are applied (`op` remove, replace or write; `tera`: the file below
/// sites/docs/patches/<variant>/ that mirrors a layout patch in the Tera overlay, null when the
/// patch changes the site input both builds share).
#[must_use]
pub fn patches_json() -> PathBuf {
    i01_dir().join("patches.json")
}

fn tera_patches() -> PathBuf {
    crate::root().join("sites/docs/patches")
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> Fail + '_ {
    move |e| fail!("{}: {e}", path.display())
}

fn write(dir: &Path, rel: &str, content: &[u8]) -> Result<(), Fail> {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io(parent))?;
    }
    std::fs::write(&path, content).map_err(io(&path))
}

fn edit(dir: &Path, rel: &str, old: &str, new: &str) -> Result<(), Fail> {
    let path = dir.join(rel);
    let s = std::fs::read_to_string(&path).map_err(io(&path))?;
    let count = s.matches(old).count();
    if count != 1 {
        return Err(fail!("{rel}: {} found {count} times", py::repr_str(old)));
    }
    std::fs::write(&path, s.replacen(old, new, 1)).map_err(io(&path))
}

/// Copies a file with its permissions and times.
fn copy_file(src: &Path, dst: &Path) -> Result<(), Fail> {
    std::fs::copy(src, dst).map_err(io(src))?;
    copy_stat(src, dst)
}

/// The permissions and times of `src` on `dst`.
fn copy_stat(src: &Path, dst: &Path) -> Result<(), Fail> {
    let meta = std::fs::metadata(src).map_err(io(src))?;
    std::fs::set_permissions(dst, meta.permissions()).map_err(io(dst))?;
    filetime::set_file_times(
        dst,
        filetime::FileTime::from_last_access_time(&meta),
        filetime::FileTime::from_last_modification_time(&meta),
    )
    .map_err(io(dst))
}

/// Copies the tree `src` into `dst` (created if missing), following symbolic links, as
/// `shutil.copytree` does.
fn copy_tree(src: &Path, dst: &Path) -> Result<(), Fail> {
    copy_dir(src, dst, false, true)
}

/// A site directory copied as [`copy_tree`] does, without its `public`, `resources` and
/// `node_modules` directories and its dot files.
fn copy_site(src: &Path, dst: &Path) -> Result<(), Fail> {
    copy_dir(src, dst, true, true)
}

fn copy_dir(src: &Path, dst: &Path, site: bool, top: bool) -> Result<(), Fail> {
    std::fs::create_dir_all(dst).map_err(io(dst))?;
    let mut entries: Vec<_> = std::fs::read_dir(src)
        .map_err(io(src))?
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for e in entries {
        let name = e.file_name();
        let name_s = name.to_string_lossy();
        let skipped = matches!(name_s.as_ref(), "public" | "resources" | "node_modules")
            || name_s.starts_with('.');
        if site && top && skipped {
            continue;
        }
        let (from, to) = (e.path(), dst.join(&name));
        if from.is_dir() {
            copy_dir(&from, &to, site, false)?;
        } else {
            copy_file(&from, &to)?;
        }
    }
    copy_stat(src, dst)
}

/// A site of the Go program made a local site: its configuration file (`<GO_NAME>.*`) named
/// `config.*`, and the `security.funcs.getenv` pattern of the Go program's variables
/// (`^<GO_NAME>_`, upper case) made this program's: it reads none of the Go program's names.
/// (The Go build's stats file keeps its name: this port neither reads nor writes it.)
fn as_local_site(dir: &Path) -> Result<(), Fail> {
    const EXTS: [&str; 4] = ["toml", "yaml", "yml", "json"];
    for ext in EXTS {
        let from = dir.join(format!("{GO_NAME}.{ext}"));
        if from.exists() {
            let to = dir.join(format!("config.{ext}"));
            std::fs::rename(&from, &to).map_err(io(&from))?;
        }
    }
    let go = format!("^{}_", GO_NAME.to_uppercase());
    let ours = format!("^{}_", ssg_base::ENV_PREFIX);
    for ext in EXTS {
        let path = dir.join(format!("config.{ext}"));
        if !path.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(io(&path))?;
        let mut out = String::with_capacity(text.len());
        let mut rest = text.as_str();
        while let Some(at) = rest.find(&go) {
            let quoted = at > 0 && matches!(rest.as_bytes()[at - 1], b'\'' | b'"');
            out.push_str(&rest[..at]);
            out.push_str(if quoted { &ours } else { &go });
            rest = &rest[at + go.len()..];
        }
        out.push_str(rest);
        if out != text {
            std::fs::write(&path, out).map_err(io(&path))?;
        }
    }
    Ok(())
}

/// A repository file by the path the fixtures record (`ssg_testkit::fixture::repo_file`).
#[must_use]
pub fn repo_file(rel: &str) -> PathBuf {
    let upstream = UPSTREAM
        .iter()
        .any(|p| rel == *p || rel.starts_with(&format!("{p}/")));
    if !upstream && format!("{rel}/").starts_with("docs/") && !rel.starts_with("docs/rust-port") {
        let rest: Vec<&str> = rel.split('/').skip(1).collect();
        return testdata().join("legacy-docs").join(rest.join("/"));
    }
    let rel = if upstream {
        rel
    } else {
        rel.strip_prefix(LEGACY_WORKSPACE).unwrap_or(rel)
    };
    if upstream {
        testdata().join("upstream").join(rel)
    } else {
        crate::root().join(rel)
    }
}

// ---------------------------------------------------------------------------------------------
// The legacy docs site (testdata/legacy-docs): the offline patch variants
// (docs/rust-port/REWRITE_PLAN.md §7.3). The Go build always built the Go-template patches. A
// patch of a file below layouts/ has a Tera counterpart at sites/docs/patches/<variant>/<same
// path> for every variant it belongs to (`sites patches` checks the 1:1 correspondence); all
// other patches change the site input both builds share.

/// The patches.json document.
///
/// # Errors
/// A missing or invalid patches.json.
pub fn docs_patches() -> Result<Py, Fail> {
    manifest::read_json(&patches_json())
}

fn patch_list(doc: &Py) -> &[Py] {
    doc.get("patches").and_then(Py::as_list).unwrap_or(&[])
}

fn in_variant(p: &Py, variant: &str) -> bool {
    p.get("variants")
        .and_then(Py::as_list)
        .unwrap_or(&[])
        .iter()
        .any(|v| v.as_str() == Some(variant))
}

fn field<'a>(p: &'a Py, k: &str) -> Result<&'a str, Fail> {
    p.get(k)
        .and_then(Py::as_str)
        .ok_or_else(|| fail!("patches.json: a patch without {k}: {}", py::dumps(p, false)))
}

/// patches.json in its canonical form: sorted keys, one patch per line.
#[must_use]
pub fn patches_json_text(doc: &Py) -> String {
    let mut lines: Vec<String> = doc
        .as_dict()
        .into_iter()
        .flatten()
        .filter(|(k, _)| *k != "patches")
        .map(|(k, v)| format!("{}: {}", py::json_str(k, false), py::dumps(v, false)))
        .collect();
    let patches: Vec<String> = patch_list(doc)
        .iter()
        .map(|p| py::dumps(p, false))
        .collect();
    lines.push(format!("\"patches\": [\n{}\n]", patches.join(",\n")));
    lines.sort();
    format!("{{\n{}\n}}\n", lines.join(",\n"))
}

/// The errors of patches.json: not in its canonical form, a `tera` field that is not the file
/// of a layout patch, or Tera patch files that do not correspond 1:1 to the layout patches.
///
/// # Errors
/// A missing or invalid patches.json.
pub fn check_patches() -> Result<Vec<String>, Fail> {
    let path = patches_json();
    let mut errors = Vec::new();
    let text = std::fs::read_to_string(&path).map_err(io(&path))?;
    let doc = docs_patches()?;
    if text != patches_json_text(&doc) {
        errors.push(format!(
            "{} is not in its canonical form (run `cargo dev sites patches`)",
            path.display()
        ));
    }
    for p in patch_list(&doc) {
        let file = field(p, "file")?;
        let want = if file.starts_with("layouts/") {
            Py::Str(file.to_owned())
        } else {
            Py::None
        };
        if *p.get_or_none("tera") != want {
            errors.push(format!(
                "{file}: `tera` must be {}",
                py::dumps(&want, false)
            ));
        }
        if !matches!(field(p, "op")?, "remove" | "replace" | "write") {
            errors.push(format!(
                "{file}: unknown op {}",
                py::repr(p.get_or_none("op"))
            ));
        }
    }
    let root = tera_patches();
    for v in DOCS_VARIANTS {
        let mut want: Vec<String> = patch_list(&doc)
            .iter()
            .filter(|p| in_variant(p, v))
            .filter_map(|p| p.get("tera").and_then(Py::as_str).map(str::to_owned))
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

fn make_docs(dir: &Path, variant: &str) -> Result<(), Fail> {
    if !DOCS_VARIANTS.contains(&variant) {
        return Err(fail!(
            "unknown docs patch variant {} (one of {})",
            py::repr_str(variant),
            DOCS_VARIANTS.join(", ")
        ));
    }
    copy_site(&testdata().join("legacy-docs"), dir)?;
    as_local_site(dir)?;
    let doc = docs_patches()?;
    for p in patch_list(&doc).iter().filter(|p| in_variant(p, variant)) {
        let file = field(p, "file")?;
        match field(p, "op")? {
            "remove" => {
                let path = dir.join(file);
                if path.exists() {
                    std::fs::remove_file(&path).map_err(io(&path))?;
                }
            }
            "replace" => edit(dir, file, field(p, "old")?, field(p, "new")?)?,
            _ => write(dir, file, field(p, "content")?.as_bytes())?,
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// testsite (T25's cli oracle used it with a small config).

fn write_txtar(dir: &Path, archive: &Path) -> Result<(), Fail> {
    let text = std::fs::read_to_string(archive).map_err(io(archive))?;
    for (k, v) in txtar::parse(&text) {
        write(dir, &k, v.as_bytes())?;
    }
    Ok(())
}

fn make_testsite(dir: &Path) -> Result<(), Fail> {
    copy_site(&repo_file("testsite"), dir)?;
    write_txtar(dir, &i01_dir().join("testsite.txtar"))
}

// ---------------------------------------------------------------------------------------------
// The T24 build sites.

fn build_fixtures() -> PathBuf {
    testdata().join("oracle/sitebuild/build")
}

fn t24_names() -> Result<Vec<String>, Fail> {
    let dir = build_fixtures();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map_err(io(&dir))?
        .filter_map(Result::ok)
        .filter_map(|e| {
            e.file_name()
                .to_string_lossy()
                .strip_suffix(".json.gz")
                .map(str::to_owned)
        })
        .collect();
    names.sort();
    Ok(names)
}

fn make_t24(name: &str, dir: &Path) -> Result<(), Fail> {
    let doc = manifest::read_json(&build_fixtures().join(format!("{name}.json.gz")))?;
    let site = doc.get_or_none("site");
    let mut toml = site
        .get("toml")
        .and_then(Py::as_str)
        .ok_or_else(|| fail!("t24-{name}: no toml"))?
        .to_owned();
    if name == "docs" {
        // t24-docs keeps code fences as plain <pre><code> (codeFences = false), as its T24
        // build-oracle comparison was set up before this port had a highlighter.
        let old = "  [markup.highlight]\n";
        if toml.matches(old).count() != 1 {
            return Err(fail!("t24-docs: highlight anchor not found"));
        }
        toml = toml.replacen(old, &format!("{old}    codeFences         = false\n"), 1);
    }
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    write(dir, "config.toml", toml.as_bytes())?;
    for f in site.get("files").and_then(Py::as_list).unwrap_or(&[]) {
        let path = field(f, "path")?;
        match f.get("repo").filter(|r| r.truthy()) {
            Some(repo) => {
                let src = repo_file(&py::str_of(repo));
                write(dir, path, &std::fs::read(&src).map_err(io(&src))?)?;
            }
            None => write(dir, path, field(f, "content")?.as_bytes())?,
        }
    }
    Ok(())
}

/// The GetRemote responses the published docs build got on 2025-10-13 (README.md next to
/// them): `<key>` files as they are, `<key>.gz` (the large ones) decompressed.
fn docs_live_cache(dir: &Path) -> Result<(), Fail> {
    let src = crate::root()
        .join("tools/rust-port/testdata/getremote-cache/docs-live/filecache/getresource");
    let out = dir.join("docs-live/filecache/getresource");
    std::fs::create_dir_all(&out).map_err(io(&out))?;
    let mut names: Vec<String> = std::fs::read_dir(&src)
        .map_err(io(&src))?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let is_key =
        |n: &str| !n.is_empty() && n.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    for name in names {
        let from = src.join(&name);
        if let Some(key) = name.strip_suffix(".gz") {
            let raw = std::fs::read(&from).map_err(io(&from))?;
            let mut data = Vec::new();
            std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&raw[..]), &mut data)
                .map_err(io(&from))?;
            let to = out.join(key);
            std::fs::write(&to, data).map_err(io(&to))?;
        } else if is_key(&name) {
            let to = out.join(&name);
            std::fs::write(&to, std::fs::read(&from).map_err(io(&from))?).map_err(io(&to))?;
        }
    }
    Ok(())
}

const PROBE_CONFIG: &str = r#"baseURL = "https://example.org/"
title = "Site"
disableKinds = ["rss", "sitemap", "taxonomy", "term", "robotsTXT", "404"]
[minify]
disableJSON = true
[outputs]
home = ["html", "json", "plain"]
[outputFormats.plain]
mediaType = "text/plain"
baseName = "index"
isPlainText = true
[params]
intv = 3
floatv = 1.0
arr = ["x", "y"]
[params.nested]
key = "v"
"#;

const PROBE_HOME: &str = r#"+++
title = "Home \"Q\" & 'A' <b>"
description = "desc + plus / slash"
date = 2020-09-06T15:46:26.955Z
yint = 5
yfloat = 4.5
yfloat0 = 5.0
ybig = 12345678901
ystr = "007"
ybool = true
yfalse = false
yempty = ""
yzero = 0
ylist = ["a", "b", "c"]
ylistmixed = [1, "x", 2.5]
yemptylist = []
ymap = {b = 2, a = 1, c = [1, 2]}
yemptymap = {}
ydate = 2021-01-02T00:00:00Z
yneg = -3
yexp = 1.5e10
mixed_case = "mc"
yhtml = "<em>h</em>"
+++
"#;

/// T13's probe site (the template-engine spec's Appendix A/B probe lines) built for real: the
/// store oracle's three home layouts, and a home page whose front matter holds the values of
/// the oracle's stub page (the real page and func map, not the minimal test FuncMap).
fn make_probe(dir: &Path) -> Result<(), Fail> {
    let doc = manifest::read_json(&testdata().join("oracle/tplimpl/probe/probe.json.gz"))?;
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    for (k, v) in doc.get("files").and_then(Py::as_dict).into_iter().flatten() {
        // With the real func map `js` is the Go implementation's js namespace: `.Title | js`
        // prints the namespace struct, which Go prints with its pointer addresses (different
        // in every Go run).
        let v = py::str_of(v).replace("{{ .Title | js }}", "JS-NAMESPACE");
        write(dir, k, v.as_bytes())?;
    }
    write(dir, "config.toml", PROBE_CONFIG.as_bytes())?;
    write(dir, "content/_index.md", PROBE_HOME.as_bytes())
}

// ---------------------------------------------------------------------------------------------
// mini: the e2e oracle's small en/th site (testdata/oracle/commands/e2e/mini.txtar). Its one
// GetRemote call is served from the getresource entry the oracle recorded with the case
// (e2e.json.gz, `_cache/site/filecache/getresource/<key>`).

fn make_mini(dir: &Path) -> Result<(), Fail> {
    if dir.file_name().is_none_or(|n| n != "mini") {
        return Err(fail!(
            "the mini site dir must be named mini (it keys the GetRemote cache)"
        ));
    }
    write_txtar(dir, &testdata().join("oracle/commands/e2e/mini.txtar"))
}

fn mini_cache(dir: &Path) -> Result<(), Fail> {
    let out = dir.join("mini/filecache/getresource");
    std::fs::create_dir_all(&out).map_err(io(&out))?;
    let doc = manifest::read_json(&testdata().join("oracle/commands/e2e/e2e.json.gz"))?;
    let case = doc
        .get("cases")
        .and_then(Py::as_list)
        .unwrap_or(&[])
        .iter()
        .find(|c| c.get("name").and_then(Py::as_str) == Some("mini"))
        .ok_or_else(|| fail!("e2e.json.gz: no case mini"))?;
    let prefix = "_cache/site/filecache/getresource/";
    for (name, content) in case
        .get("files")
        .and_then(Py::as_dict)
        .into_iter()
        .flatten()
    {
        if let Some(key) = name.strip_prefix(prefix) {
            let to = out.join(key);
            std::fs::write(&to, py::str_of(content)).map_err(io(&to))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// images: the recipes of testdata/golden/images/manifest.json (the golden images of the PSNR
// gate of T41) as a site whose home page runs every recipe with Go's image processing and
// prints `<golden name> <RelPermalink>` per line (tools/dev/oracle.sh, frozen at 44529028,
// copied the published files into testdata/golden/images).

/// The filters of the recipes (the JSON of `ssg_images::ImageFilter`) as Go template calls:
/// the images.* function and the keys of its arguments.
const FILTER_ARGS: [(&str, &str, &[&str]); 16] = [
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

/// A filter argument as a Go template literal.
fn go_value(v: &Py) -> Result<String, Fail> {
    match v {
        Py::Str(s) => Ok(py::json_str(s, true)),
        Py::Int(_) | Py::BigInt(_) | Py::Float(_) => Ok(py::repr(v)),
        _ => Err(fail!("images: unsupported filter argument {}", py::repr(v))),
    }
}

struct Images<'a> {
    dir: &'a Path,
    /// Repository path -> assets path.
    files: Vec<(String, String)>,
}

impl Images<'_> {
    fn asset(&mut self, repo_path: &str) -> Result<String, Fail> {
        let name = match self.files.iter().find(|(r, _)| r == repo_path) {
            Some((_, a)) => a.clone(),
            None => {
                let a = format!(
                    "g/{:02}{}",
                    self.files.len(),
                    py::splitext_ext(repo_path).to_lowercase()
                );
                let src = repo_file(repo_path);
                write(
                    self.dir,
                    &format!("assets/{a}"),
                    &std::fs::read(&src).map_err(io(&src))?,
                )?;
                self.files.push((repo_path.to_owned(), a.clone()));
                a
            }
        };
        Ok(format!("(resources.Get \"{name}\")"))
    }

    fn filter_call(&mut self, f: &Py) -> Result<String, Fail> {
        let op = py::str_of(f.get_or_none("op"));
        let arg = |k: &str| {
            f.get(k)
                .ok_or_else(|| fail!("images: filter {op} without {k}"))
                .and_then(go_value)
        };
        if let Some((_, name, keys)) = FILTER_ARGS.iter().find(|(o, _, _)| *o == op) {
            let mut parts = vec![format!("images.{name}")];
            for k in *keys {
                parts.push(arg(k)?);
            }
            return Ok(parts.join(" "));
        }
        match op.as_str() {
            "padding" => {
                let margin: Vec<Py> = match f.get("margin").filter(|m| m.truthy()) {
                    Some(m) => m.as_list().unwrap_or(&[]).to_vec(),
                    None => ["top", "right", "bottom", "left"]
                        .iter()
                        .map(|k| f.get(k).cloned().unwrap_or(Py::Int(0)))
                        .collect(),
                };
                let mut parts = vec!["images.Padding".to_owned()];
                for m in &margin {
                    parts.push(go_value(m)?);
                }
                if let Some(c) = f.get("color") {
                    parts.push(go_value(c)?);
                }
                Ok(parts.join(" "))
            }
            "overlay" => {
                let image = self.asset(&py::str_of(f.get_or_none("image")))?;
                let (x, y) = (
                    f.get("x").cloned().unwrap_or(Py::Int(0)),
                    f.get("y").cloned().unwrap_or(Py::Int(0)),
                );
                Ok(format!(
                    "images.Overlay {image} {} {}",
                    go_value(&x)?,
                    go_value(&y)?
                ))
            }
            "mask" => Ok(format!(
                "images.Mask {}",
                self.asset(&py::str_of(f.get_or_none("image")))?
            )),
            "process" => Ok(format!("images.Process {}", arg("spec")?)),
            _ => Err(fail!("images: unsupported filter {}", py::repr_str(&op))),
        }
    }
}

fn make_images(dir: &Path) -> Result<(), Fail> {
    let recipes = manifest::read_json(&testdata().join("golden/images/manifest.json"))?;
    let mut im = Images {
        dir,
        files: Vec::new(),
    };
    let mut lines = Vec::new();
    for r in recipes.as_list().unwrap_or(&[]) {
        let golden = py::str_of(r.get_or_none("golden"));
        if r.get("imaging").is_some_and(Py::truthy) {
            return Err(fail!(
                "images: {golden}: a recipe's own [imaging] is not supported (one site)"
            ));
        }
        lines.push(format!(
            "{{{{- $r := {} }}}}",
            im.asset(&py::str_of(r.get_or_none("source")))?
        ));
        for step in r.get("steps").and_then(Py::as_list).unwrap_or(&[]) {
            if let Some(spec) = step.get("spec") {
                lines.push(format!("{{{{- $r = $r.Process {} }}}}", go_value(spec)?));
            } else {
                let mut fs = Vec::new();
                for f in step.get("filters").and_then(Py::as_list).unwrap_or(&[]) {
                    fs.push(format!("({})", im.filter_call(f)?));
                }
                lines.push(format!(
                    "{{{{- $r = $r | images.Filter (slice {}) }}}}",
                    fs.join(" ")
                ));
            }
        }
        lines.push(format!("{golden} {{{{ $r.RelPermalink }}}}"));
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

// ---------------------------------------------------------------------------------------------
// The Rust overlay (REWRITE_PLAN.md §7.4): the site as generated above, with its layouts
// replaced by the Tera layouts of sites/<site>/layouts, the Tera versions of
// template-processed assets (sites/<site>/assets) copied over, and for docs the variant's Tera
// patch files (sites/docs/patches/<variant>/) layered on top. Content, i18n, data, config and
// all other assets stay as generated.

fn apply_overlay(dir: &Path, overlay: &Path, variant: Option<&str>) -> Result<(), Fail> {
    if !overlay.join("layouts").is_dir() {
        return Err(fail!("{} has no layouts directory", overlay.display()));
    }
    let layouts = dir.join("layouts");
    if layouts.exists() {
        std::fs::remove_dir_all(&layouts).map_err(io(&layouts))?;
    }
    copy_tree(&overlay.join("layouts"), &layouts)?;
    if overlay.join("assets").is_dir() {
        copy_tree(&overlay.join("assets"), &dir.join("assets"))?;
    }
    let content = overlay.join("content");
    if content.is_dir() {
        for rel in manifest::walk(&content) {
            let dst = dir.join("content").join(&rel);
            if rel == "_content.html" || rel.ends_with("/_content.html") {
                let gotmpl = dst.with_file_name("_content.gotmpl");
                if !gotmpl.exists() {
                    continue; // the patches removed the Go adapter: the variant has none
                }
                std::fs::remove_file(&gotmpl).map_err(io(&gotmpl))?;
            }
            write(
                dir,
                &format!("content/{rel}"),
                &std::fs::read(content.join(&rel)).map_err(io(&content))?,
            )?;
        }
    }
    if let Some(variant) = variant {
        let vdir = overlay.join("patches").join(variant);
        if vdir.is_dir() {
            copy_tree(&vdir, dir)?;
        } else if patch_list(&docs_patches()?)
            .iter()
            .any(|p| p.get("tera").is_some_and(Py::truthy) && in_variant(p, variant))
        {
            return Err(fail!("{} has no patches/{variant}", overlay.display()));
        }
    }
    Ok(())
}

/// The site and its docs patch variant: `docs-<variant>` is docs with `--docs-patches`.
///
/// # Errors
/// A variant that contradicts the name, or a variant for another site.
pub fn site_and_variant(
    name: &str,
    variant: Option<&str>,
) -> Result<(String, Option<String>), Fail> {
    let (base, v) = name.split_once('-').unwrap_or((name, ""));
    if base == "docs" && DOCS_VARIANTS.contains(&v) {
        if variant.is_some_and(|x| x != v) {
            return Err(fail!(
                "{name} contradicts --docs-patches {}",
                variant.unwrap_or_default()
            ));
        }
        return Ok((base.to_owned(), Some(v.to_owned())));
    }
    if name == "docs" {
        return Ok((name.to_owned(), Some(variant.unwrap_or(I01).to_owned())));
    }
    if variant.is_some() {
        return Err(fail!("--docs-patches applies to the docs site only"));
    }
    Ok((name.to_owned(), None))
}

/// `sites list`: every site name.
///
/// # Errors
/// An unreadable fixture directory.
pub fn list() -> Result<Vec<String>, Fail> {
    let mut names: Vec<String> = ["docs", "testsite", "mini", "images", "errors", "probe"]
        .map(str::to_owned)
        .to_vec();
    names.extend(DOCS_VARIANTS.iter().map(|v| format!("docs-{v}")));
    names.extend(t24_names()?.into_iter().map(|n| format!("t24-{n}")));
    Ok(names)
}

/// A lexically normalised absolute path.
fn absolute(p: &Path) -> PathBuf {
    let p = std::path::absolute(p).unwrap_or_else(|_| p.to_owned());
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

/// `sites make`: writes the site `name` into `dir` (which must not exist, outside the
/// repository).
///
/// # Errors
/// An unknown site, an existing or in-repository `dir`, or a failed write.
pub fn make(
    name: &str,
    dir: &Path,
    variant: Option<&str>,
    overlay: Option<&Path>,
) -> Result<(), Fail> {
    let (name, variant) = site_and_variant(name, variant)?;
    if dir.exists() {
        return Err(fail!("{} exists", dir.display()));
    }
    if absolute(dir).starts_with(absolute(&crate::root())) {
        return Err(fail!("refusing to write a site into the repository tree"));
    }
    match name.as_str() {
        "docs" => make_docs(dir, variant.as_deref().unwrap_or(I01))?,
        "testsite" => make_testsite(dir)?,
        "mini" => make_mini(dir)?,
        "images" => make_images(dir)?,
        "errors" => write_txtar(dir, &i01_dir().join("errors.txtar"))?,
        "probe" => make_probe(dir)?,
        n => match n.strip_prefix("t24-") {
            Some(t) => make_t24(t, dir)?,
            None => return Err(fail!("unknown site {name}")),
        },
    }
    if let Some(overlay) = overlay {
        apply_overlay(dir, &absolute(overlay), variant.as_deref())?;
    }
    Ok(())
}

/// `sites cache`: the `--cacheDir` contents the site needs (none for most sites).
///
/// # Errors
/// A failed write.
pub fn cache(name: &str, dir: &Path) -> Result<(), Fail> {
    let (name, variant) = site_and_variant(name, None)?;
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    if name == "docs" && variant.as_deref() == Some(LIVE) {
        docs_live_cache(dir)
    } else if name == "mini" {
        mini_cache(dir)
    } else {
        Ok(())
    }
}

/// `sites patches`: rewrites patches.json in its canonical form (unless `check`), then checks
/// it; the exit status.
///
/// # Errors
/// A missing or invalid patches.json.
pub fn patches(check: bool) -> Result<i32, Fail> {
    if !check {
        let path = patches_json();
        std::fs::write(&path, patches_json_text(&docs_patches()?)).map_err(io(&path))?;
    }
    let errors = check_patches()?;
    for e in &errors {
        eprintln!("sites patches: {e}");
    }
    if !errors.is_empty() {
        return Ok(1);
    }
    println!(
        "patches.json: {} entries; the Tera patch files of {} correspond 1:1",
        patch_list(&docs_patches()?).len(),
        DOCS_VARIANTS.join(", ")
    );
    Ok(0)
}

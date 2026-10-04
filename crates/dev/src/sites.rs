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

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Fail, fail, json, manifest, txtar};

mod docs;
mod images;
mod oracles;

pub use docs::*;
use images::*;
use oracles::*;

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

/// tools/rust-port/i01/patches.json: the edits of the docs site per variant.
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
        return Err(fail!("{rel}: {old:?} found {count} times"));
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

/// Copies the tree `src` into `dst` (created if missing), following symbolic links.
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
            std::fs::rename(&from, dir.join(format!("config.{ext}"))).map_err(io(&from))?;
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
    if upstream {
        return testdata().join("upstream").join(rel);
    }
    if format!("{rel}/").starts_with("docs/") && !rel.starts_with("docs/rust-port") {
        let rest: Vec<&str> = rel.split('/').skip(1).collect();
        return testdata().join("legacy-docs").join(rest.join("/"));
    }
    crate::root().join(rel.strip_prefix(LEGACY_WORKSPACE).unwrap_or(rel))
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
        } else if Patches::read()?
            .patches
            .iter()
            .any(|p| p.tera.is_some() && p.in_variant(variant))
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
        if let Some(other) = variant.filter(|x| *x != v) {
            return Err(fail!("{name} contradicts --docs-patches {other}"));
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
    let doc = Patches::read()?;
    if !check {
        let path = patches_json();
        std::fs::write(&path, doc.to_text()).map_err(io(&path))?;
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
        doc.patches.len(),
        DOCS_VARIANTS.join(", ")
    );
    Ok(0)
}

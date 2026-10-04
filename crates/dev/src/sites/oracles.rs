//! The small sites of the oracles: testsite, the T24 build sites, the probe and mini.

use super::*;

// ---------------------------------------------------------------------------------------------
// testsite (T25's cli oracle used it with a small config).

pub(super) fn write_txtar(dir: &Path, archive: &Path) -> Result<(), Fail> {
    let text = std::fs::read_to_string(archive).map_err(io(archive))?;
    for (k, v) in txtar::parse(&text) {
        write(dir, &k, v.as_bytes())?;
    }
    Ok(())
}

pub(super) fn make_testsite(dir: &Path) -> Result<(), Fail> {
    copy_site(&repo_file("testsite"), dir)?;
    write_txtar(dir, &i01_dir().join("testsite.txtar"))
}

// ---------------------------------------------------------------------------------------------
// The T24 build sites.

#[derive(Deserialize)]
pub(super) struct BuildFixture {
    pub(super) site: FixtureSite,
}

#[derive(Deserialize)]
pub(super) struct FixtureSite {
    pub(super) toml: String,
    pub(super) files: Vec<FixtureFile>,
}

/// A file of a fixture site: a repository file (`repo`) or its content.
#[derive(Deserialize)]
pub(super) struct FixtureFile {
    pub(super) path: String,
    #[serde(default)]
    pub(super) repo: Option<String>,
    #[serde(default)]
    pub(super) content: Option<String>,
}

pub(super) fn build_fixtures() -> PathBuf {
    testdata().join("oracle/sitebuild/build")
}

pub(super) fn t24_names() -> Result<Vec<String>, Fail> {
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

pub(super) fn make_t24(name: &str, dir: &Path) -> Result<(), Fail> {
    let fixture: BuildFixture = json::read(&build_fixtures().join(format!("{name}.json.gz")))?;
    let mut toml = fixture.site.toml;
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
    for f in fixture.site.files {
        match (f.repo.filter(|r| !r.is_empty()), f.content) {
            (Some(repo), _) => {
                let src = repo_file(&repo);
                write(dir, &f.path, &std::fs::read(&src).map_err(io(&src))?)?;
            }
            (None, Some(content)) => write(dir, &f.path, content.as_bytes())?,
            (None, None) => {
                return Err(fail!("t24-{name}: {} has neither repo nor content", f.path));
            }
        }
    }
    Ok(())
}

/// The GetRemote responses the published docs build got on 2025-10-13 (README.md next to
/// them): `<key>` files as they are, `<key>.gz` (the large ones) decompressed.
pub(super) fn docs_live_cache(dir: &Path) -> Result<(), Fail> {
    let src = crate::root()
        .join("tools/rust-port/testdata/getremote-cache/docs-live/filecache/getresource");
    let out = dir.join("docs-live/filecache/getresource");
    std::fs::create_dir_all(&out).map_err(io(&out))?;
    let is_key =
        |n: &str| !n.is_empty() && n.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    for e in std::fs::read_dir(&src)
        .map_err(io(&src))?
        .filter_map(Result::ok)
    {
        let name = e.file_name().to_string_lossy().into_owned();
        let from = e.path();
        if let Some(key) = name.strip_suffix(".gz") {
            let to = out.join(key);
            std::fs::write(&to, json::read_bytes(&from)?).map_err(io(&to))?;
        } else if is_key(&name) {
            let to = out.join(&name);
            std::fs::write(&to, std::fs::read(&from).map_err(io(&from))?).map_err(io(&to))?;
        }
    }
    Ok(())
}

pub(super) const PROBE_CONFIG: &str = r#"baseURL = "https://example.org/"
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

pub(super) const PROBE_HOME: &str = r#"+++
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

#[derive(Deserialize)]
pub(super) struct Probe {
    pub(super) files: BTreeMap<String, String>,
}

/// T13's probe site (the template-engine spec's Appendix A/B probe lines) built for real: the
/// store oracle's three home layouts, and a home page whose front matter holds the values of
/// the oracle's stub page (the real page and func map, not the minimal test FuncMap).
pub(super) fn make_probe(dir: &Path) -> Result<(), Fail> {
    let probe: Probe = json::read(&testdata().join("oracle/tplimpl/probe/probe.json.gz"))?;
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    for (k, v) in &probe.files {
        // With the real func map `js` is the Go implementation's js namespace: `.Title | js`
        // prints the namespace struct, which Go prints with its pointer addresses (different
        // in every Go run).
        write(
            dir,
            k,
            v.replace("{{ .Title | js }}", "JS-NAMESPACE").as_bytes(),
        )?;
    }
    write(dir, "config.toml", PROBE_CONFIG.as_bytes())?;
    write(dir, "content/_index.md", PROBE_HOME.as_bytes())
}

// ---------------------------------------------------------------------------------------------
// mini: the e2e oracle's small en/th site (testdata/oracle/commands/e2e/mini.txtar). Its one
// GetRemote call is served from the getresource entry the oracle recorded with the case
// (e2e.json.gz, `_cache/site/filecache/getresource/<key>`).

#[derive(Deserialize)]
pub(super) struct E2e {
    pub(super) cases: Vec<E2eCase>,
}

#[derive(Deserialize)]
pub(super) struct E2eCase {
    pub(super) name: String,
    #[serde(default)]
    pub(super) files: BTreeMap<String, Value>,
}

pub(super) fn make_mini(dir: &Path) -> Result<(), Fail> {
    if dir.file_name().is_none_or(|n| n != "mini") {
        return Err(fail!(
            "the mini site dir must be named mini (it keys the GetRemote cache)"
        ));
    }
    write_txtar(dir, &testdata().join("oracle/commands/e2e/mini.txtar"))
}

pub(super) fn mini_cache(dir: &Path) -> Result<(), Fail> {
    let out = dir.join("mini/filecache/getresource");
    std::fs::create_dir_all(&out).map_err(io(&out))?;
    let e2e: E2e = json::read(&testdata().join("oracle/commands/e2e/e2e.json.gz"))?;
    let case = e2e
        .cases
        .into_iter()
        .find(|c| c.name == "mini")
        .ok_or_else(|| fail!("e2e.json.gz: no case mini"))?;
    for (name, content) in &case.files {
        if let (Some(key), Some(content)) = (
            name.strip_prefix("_cache/site/filecache/getresource/"),
            content.as_str(),
        ) {
            let to = out.join(key);
            std::fs::write(&to, content).map_err(io(&to))?;
        }
    }
    Ok(())
}

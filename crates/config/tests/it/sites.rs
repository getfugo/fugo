//! The T24 build-oracle sites (`sitebuild/assemble/*`): each site's `config.toml` loads, its
//! languages come in Go's order, and the formats of its enabled content kinds are Go's `.Site`
//! render formats (in order), less the formats that only pages' front matter `outputs` add.
//! A summary of each loaded configuration is kept as an insta snapshot.

use serde_json::{Value as J, json};
use ssg_config::{Config, LoadOptions};

use crate::support::fixture;

const SITES: &[&str] = &[
    "asm-build",
    "asm-cascade",
    "asm-flags",
    "asm-i18n",
    "asm-multihost",
    "asm-taxo",
    "asm-ugly",
    "content",
    "contentdir",
    "docs",
    "edge-tree",
    "homeleaf",
    "nokinds",
    "shortcodes",
    "synthetic",
    "testsite",
];

fn load_toml(toml: &str) -> (tempfile::TempDir, Config) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let dir = tmp.path().join("site");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("config.toml"), toml).expect("write");
    let options = LoadOptions {
        source: dir,
        env: vec![(
            "HOME".into(),
            tmp.path().join("home").to_string_lossy().into_owned(),
        )],
        ..LoadOptions::default()
    };
    let c = ssg_config::load(&options).unwrap_or_else(|e| panic!("{e}"));
    (tmp, c)
}

/// The formats pages of the site render in: the formats of the enabled content kinds, in
/// render order.
fn render_formats(c: &Config, s: &ssg_config::SiteConfig) -> Vec<String> {
    let mut ids: Vec<_> = s
        .outputs
        .iter()
        .filter(|(k, _)| k.is_content())
        .flat_map(|(_, ids)| ids.iter().copied())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids.iter()
        .map(|&id| c.output_formats.get(id).name.clone())
        .collect()
}

/// A summary of the typed configuration for the snapshot (no paths).
/// [`summary`] as snapshots record it: numbers as numbers, keys sorted (serializing a
/// `serde_json::Value` into YAML does neither once rolldown turns on serde_json's
/// `arbitrary_precision` and `preserve_order`).
pub fn snapshot(c: &Config) -> ssg_base::Value {
    ssg_base::Value::from_json(summary(c))
}

pub fn summary(c: &Config) -> J {
    json!({
        "environment": c.environment,
        "multihost": c.multihost,
        "defaultLanguageInSubdir": c.default_language_in_subdir,
        "timeout": c.timeout.as_secs(),
        "formats": c.output_formats.iter().map(|(_, f)| &f.name).collect::<Vec<_>>(),
        "content": c.content,
        "minifyOutput": c.minify.minify_output,
        "security": {"inlineShortcodes": c.security.inline_shortcodes},
        "sites": c.sites.iter().map(|s| json!({
            "lang": s.language.key,
            "code": s.language.code,
            "name": s.language.name,
            "weight": s.language.weight,
            "timeZone": s.language.time_zone.iana_name(),
            "urlPrefix": s.language.url_prefix,
            "baseURL": s.base_url.as_str(),
            "title": s.title,
            "outputs": s.outputs.iter()
                .map(|(k, ids)| (k.as_str().to_owned(),
                    J::from(ids.iter().map(|&id| c.output_formats.get(id).name.clone()).collect::<Vec<_>>())))
                .collect::<serde_json::Map<_, _>>(),
            "disableKinds": s.disable_kinds.iter().map(ssg_base::PageKind::as_str).collect::<Vec<_>>(),
            "taxonomies": s.taxonomies.iter().map(|t| format!("{}={}", t.singular, t.plural)).collect::<Vec<_>>(),
            "permalinks": s.permalinks,
            "pagination": s.pagination,
            "ugly": s.urls.ugly,
            "menus": s.menus.len(),
            "cascade": s.cascade.len(),
            "params": s.params.len(),
            "mainSections": s.main_sections,
        })).collect::<Vec<_>>(),
    })
}

#[test]
fn t24_sites() {
    let (mut checks, mut exact) = (0, 0);
    for name in SITES {
        let fx = fixture(&format!("oracle/sitebuild/assemble/{name}.json.gz"));
        let (_tmp, c) = load_toml(fx["site"]["toml"].as_str().expect("toml"));
        let sites = fx["dump"]["sites"].as_array().expect("sites");
        let langs: Vec<&str> = c.sites.iter().map(|s| s.language.key.as_str()).collect();
        let want: Vec<&str> = sites.iter().filter_map(|s| s["lang"].as_str()).collect();
        assert_eq!(langs, want, "{name}: languages");
        for (s, w) in c.sites.iter().zip(sites) {
            let want: Vec<String> = w["renderFormats"]
                .as_array()
                .expect("renderFormats")
                .iter()
                .filter_map(|f| f.as_str().map(str::to_owned))
                .collect();
            let mine = render_formats(&c, s);
            let mut rest = want.iter();
            assert!(
                mine.iter().all(|f| rest.any(|w| w == f)),
                "{name} [{}]: render formats {mine:?}, want (a subsequence of) {want:?}",
                s.language.key
            );
            checks += 1;
            if mine == want {
                exact += 1;
            }
        }
        ssg_testkit::snapshot::settings().bind(|| {
            insta::assert_yaml_snapshot!(format!("t24-{name}"), snapshot(&c));
        });
    }
    eprintln!(
        "t24 sites: {} sites, languages equal; {checks} site languages, render formats equal for {exact} (the others add formats in page front matter)",
        SITES.len()
    );
}

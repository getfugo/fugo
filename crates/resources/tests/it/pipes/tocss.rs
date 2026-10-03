//! `to_css` against the `tocss` oracle (Go's LibSass `toCSS` on t16site, 31 cases) and on
//! a small fixture stylesheet (`tests/fixtures/styles.txtar`).
//!
//! grass follows dart-sass, not LibSass: output styles `nested` and `compact` are written
//! expanded, numbers get up to 10 decimals (`precision` has no effect), and there are no
//! source maps. So CSS is compared normalised (whitespace, `@charset`/BOM, leading zeros,
//! trailing semicolons); the cases whose normalised CSS still differs are listed in
//! `expected_diffs.toml` `[tocss]` with the reason. Errors must be errors at the same file
//! and line.

use std::collections::BTreeMap;

use serde_json::Value as J;
use ssg_resources::pipes::ToCssOptions;
use ssg_resources::{ResourceError, Transform};

use super::{Project, fixture_site, project, run_steps};
use crate::support::expected_diffs;

/// CSS without formatting differences.
pub fn normalise(css: &str) -> String {
    let css = css
        .trim_start_matches('\u{feff}')
        .replace("@charset \"UTF-8\";", "");
    let mut out = String::with_capacity(css.len());
    let mut last_space = false;
    for c in css.chars() {
        if c.is_whitespace() {
            last_space = true;
            continue;
        }
        if last_space
            && !out.is_empty()
            && !"{};:,()/>".contains(c)
            && !out.ends_with(['{', '}', ';', ':', ',', '(', '/', '>'])
        {
            out.push(' ');
        }
        last_space = false;
        out.push(c);
    }
    let out = out.replace(";}", "}");
    // Leading zeros: `0.5` → `.5` after a separator.
    let mut res = String::with_capacity(out.len());
    let b: Vec<char> = out.chars().collect();
    for (i, &c) in b.iter().enumerate() {
        let prev = if i == 0 { ' ' } else { b[i - 1] };
        if c == '0'
            && b.get(i + 1) == Some(&'.')
            && b.get(i + 2).is_some_and(char::is_ascii_digit)
            && !prev.is_ascii_digit()
            && prev != '.'
        {
            continue;
        }
        res.push(c);
    }
    res
}

/// Normalised CSS without the accepted LibSass differences: plain CSS `@import`s dropped
/// (dart-sass hoists them to the top; Go's LibSass path hides them in a comment, so
/// compressed output loses them), numbers rounded to 3 decimals (LibSass writes `precision`
/// decimals, dart-sass 10), CSS escapes in strings written as the character, `calc()` of
/// constants simplified, no source map comment.
fn dart_view(css: &str) -> String {
    let mut css = normalise(css);
    if let Some(i) = css.find("/*# sourceMappingURL=") {
        let end = css[i..].find("*/").map_or(css.len(), |e| i + e + 2);
        css.replace_range(i..end, "");
    }
    while let Some(i) = css.find("@import \"") {
        let end = css[i..].find(';').map_or(css.len(), |e| i + e + 1);
        css.replace_range(i..end, "");
    }
    let css = css
        .replace("\\2192 ", "\u{2192}")
        .replace("calc(10px + 2px)", "12px");
    // Decimals rounded to 3 places.
    let mut out = String::with_capacity(css.len());
    let mut rest = css.as_str();
    while let Some(i) = rest.find('.') {
        let (head, tail) = rest.split_at(i);
        out.push_str(head);
        let digits = tail[1..].bytes().take_while(u8::is_ascii_digit).count();
        if digits > 3 {
            let value: f64 = format!("0.{}", &tail[1..=digits]).parse().unwrap();
            let rounded = format!("{value:.3}");
            out.push_str(rounded.trim_start_matches('0').trim_end_matches('0'));
            rest = &tail[1 + digits..];
        } else {
            out.push('.');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

fn run_fixture(p: &Project, fx: &J) -> (usize, Vec<String>, BTreeMap<String, String>) {
    let diffs = expected_diffs();
    let accepted = diffs
        .get("tocss")
        .and_then(|t| t.as_table())
        .cloned()
        .unwrap_or_default();
    let mut failures = Vec::new();
    let mut deviations = BTreeMap::new();
    let mut equal = 0;
    for (case, want) in fx["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fx["results"].as_array().unwrap())
    {
        let name = case["name"].as_str().unwrap();
        let got = run_steps(p, case, &BTreeMap::new());
        let expected_error = want
            .get("contentErr")
            .or_else(|| want.get("stepErr"))
            .and_then(J::as_str);
        match (got, expected_error) {
            (Ok(id), None) => {
                let css = String::from_utf8(p.store.content(id).unwrap().to_vec()).unwrap();
                let w = normalise(want["content"].as_str().unwrap());
                let g = normalise(&css);
                let r = p.store.resource(id);
                if want["mediaType"] != r.media_type_string() {
                    failures.push(format!("{name}: media type {}", r.media_type_string()));
                }
                let minified = case["steps"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|s| s["op"] == "minify");
                if let Some(link) = want.get("relPermalink").and_then(J::as_str)
                    && link != r.rel_permalink
                    && !minified
                {
                    failures.push(format!("{name}: link {} != {link}", r.rel_permalink));
                }
                if w == g {
                    equal += 1;
                } else if accepted.contains_key(name) && minified {
                    // Minified by lightningcss: the fingerprinted link has Go's shape.
                    let link = &r.rel_permalink;
                    let hex = link
                        .strip_prefix("/scss/main.min.")
                        .and_then(|l| l.strip_suffix(".css"))
                        .unwrap_or_default();
                    if hex.len() == 64 && !g.contains('\n') {
                        deviations.insert(name.to_owned(), g);
                    } else {
                        failures.push(format!("{name}: link {link}"));
                    }
                } else if accepted.contains_key(name)
                    && dart_view(want["content"].as_str().unwrap()) == dart_view(&css)
                {
                    deviations.insert(name.to_owned(), g);
                } else {
                    failures.push(format!("{name}:\n  want {w}\n  got  {g}"));
                }
            }
            (Err((_, e)), Some(w)) => {
                // Same file and line (`file:line:col` in both), or both a decode error.
                let want_at = w
                    .split('"')
                    .find(|s| s.starts_with("$SITE/") || s.starts_with("build:vars"))
                    .map(|s| p.site(s));
                let want_file_line = want_at
                    .as_deref()
                    .map(|s| s.rsplit_once(':').map_or(s, |x| x.0).to_owned());
                match want_file_line {
                    Some(fl) if !e.contains(&format!("{fl}:")) => {
                        if accepted.contains_key(name) {
                            deviations.insert(name.to_owned(), e);
                        } else {
                            failures.push(format!("{name}: error {e:?}, want at {fl}"));
                        }
                    }
                    _ => equal += 1,
                }
            }
            (Ok(id), Some(w)) => {
                let css = String::from_utf8_lossy(&p.store.content(id).unwrap()).into_owned();
                if accepted.contains_key(name) {
                    deviations.insert(name.to_owned(), normalise(&css));
                } else {
                    failures.push(format!("{name}: compiled, Go fails: {w}"));
                }
            }
            (Err((_, e)), None) => failures.push(format!("{name}: {e}")),
        }
    }
    (equal, failures, deviations)
}

#[test]
fn tocss_oracle() {
    let fx: J = ssg_testkit::fixture::oracle("oracle/resource-transformers/tocss/synth.json.gz");
    let p = project(&fixture_site("t16site"), |_| {});
    let (equal, failures, deviations) = run_fixture(&p, &fx);
    let accepted = expected_diffs()["tocss"].as_table().unwrap().clone();
    let unused: Vec<&String> = accepted
        .keys()
        .filter(|k| !deviations.contains_key(*k))
        .collect();
    assert!(
        failures.is_empty() && unused.is_empty(),
        "{} failures:\n{}\nexpected_diffs [tocss] entries that now match: {unused:?}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(equal + deviations.len(), 31);
    eprintln!(
        "tocss: {equal} of 31 cases equal to LibSass (normalised), {} accepted deviations",
        deviations.len()
    );
}

#[test]
fn tocss_styles_scss() {
    // The SCSS of tests/fixtures/styles.txtar: a variables
    // partial, a component in a subdirectory, darken/mix/lighten, @each, and the slash division
    // `$gutter / 2`, which dart-sass semantics compute (6px), where plain CSS `a / b` stays.
    let txtar = std::fs::read_to_string(
        crate::support::repo_dir().join("crates/resources/tests/fixtures/styles.txtar"),
    )
    .unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let site = tmp.path().join("site");
    std::fs::create_dir_all(&site).unwrap();
    std::fs::write(
        site.join("config.toml"),
        "baseURL = \"https://example.org/\"\n",
    )
    .unwrap();
    let mut files = 0;
    for (name, body) in txtar_files(&txtar) {
        if name.starts_with("assets/scss/") {
            let path = site.join(&name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
            files += 1;
        }
    }
    assert_eq!(files, 3);
    let p = project(&site, |_| {});
    let src = p.asset("scss/website.scss");
    let opts = ToCssOptions::from_json(&serde_json::json!({
        "enableSourceMap": false,
        "includePaths": ["node_modules", "assets/scss"],
        "outputStyle": "compressed"
    }))
    .unwrap();
    let css_id = p.store.transform(src, Transform::ToCss(opts)).unwrap();
    let css = String::from_utf8(p.store.content(css_id).unwrap().to_vec()).unwrap();
    assert!(css.contains(".pagination li{padding:6px}"), "{css}");
    assert!(
        css.contains("body{font-family:\"Inter\",Helvetica,sans-serif;color:#a55318;"),
        "{css}"
    );
    assert!(
        css.contains(".tax-brand{border-left:2px solid #eaa16c}"),
        "{css}"
    );
    assert!(css.contains(".rating-star{color:#f4b308}"), "{css}");
    assert!(
        css.contains(".card .card-title{font-size:1.25rem}"),
        "{css}"
    );
    assert_eq!(p.store.resource(css_id).rel_permalink, "/scss/website.css");
    eprintln!("tocss: fixture SCSS compiled ({} bytes)", css.len());

    // Plain CSS slash-separated values are kept; a division of variables is computed.
    let div = p
        .store
        .from_string(
            "scss/div.scss",
            "$a: 10px; .x { font: 12px/1.5 serif; w: 100% / 3; h: $a / 2; m: math-div(1, 2); }",
            &ssg_resources::CallSite::in_lang(p.lang()),
        )
        .unwrap();
    let t = Transform::ToCss(
        ToCssOptions::from_json(&serde_json::json!({"outputStyle": "compressed"})).unwrap(),
    );
    let out = p.store.transform(div, t).unwrap();
    let css = String::from_utf8(p.store.content(out).unwrap().to_vec()).unwrap();
    assert!(css.contains("font:12px/1.5 serif"), "{css}");
    assert!(css.contains("w:100%/3"), "{css}");
    assert!(css.contains("h:5px"), "{css}");
}

#[test]
fn tocss_errors_name_the_file() {
    let p = project(&fixture_site("t16site"), |_| {});
    let src = p.asset("scss/errors/in-partial.scss");
    let id = p
        .store
        .transform(src, Transform::ToCss(ToCssOptions::default()))
        .unwrap();
    let err = p.store.realize(id).unwrap_err();
    let ResourceError::Pipe {
        transform, source, ..
    } = &err
    else {
        panic!("{err}");
    };
    assert_eq!(*transform, "to_css");
    let msg = source.to_string();
    let partial = p.dir.join("assets/scss/errors/_broken.scss");
    assert!(
        msg.starts_with(&format!("{}:1:", partial.display())),
        "{msg}"
    );
    assert!(
        err.to_string()
            .starts_with("/scss/errors/in-partial.scss: to_css: "),
        "{err}"
    );
}

/// The files of a txtar archive: `-- name --` lines start them.
fn txtar_files(text: &str) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = Vec::new();
    for line in text.split_inclusive('\n') {
        let t = line.trim_end();
        if let Some(name) = t.strip_prefix("-- ").and_then(|r| r.strip_suffix(" --")) {
            files.push((name.to_owned(), String::new()));
        } else if let Some((_, body)) = files.last_mut() {
            body.push_str(line);
        }
    }
    files
}

#[test]
fn tocss_explicit_extension_imports_use_the_load_paths() {
    // grass looks for `@import "x.scss"` only next to the importing file; dart-sass (Go) also
    // searches the load paths. The entry sits at the assets root (as an `execute_as_template`
    // target does), the files are in an `includePaths` directory, and `_base.scss` imports a
    // sibling that resolves only relative to its own place.
    let site = super::mini_site(&[
        ("config.toml", "baseURL = \"https://example.org/\"\n"),
        (
            "vendor/kit/parts/_base.scss",
            "@import \"colors\";\n$pad: 4px;\n",
        ),
        ("vendor/kit/parts/_colors.scss", "$c: #123456;\n"),
        ("vendor/kit/_mixins.scss", "@mixin box { margin: 1px; }\n"),
        ("vendor/kit/_theme.scss", ".theme { v: plain; }\n"),
        (
            "vendor/kit/_theme.import.scss",
            ".theme { v: import-only; }\n",
        ),
        ("assets/scss/_local.scss", ".local { v: assets; }\n"),
        ("vendor/kit/_local.scss", ".local { v: include-path; }\n"),
    ]);
    let p = project(site.path(), |_| {});
    let compile = |name: &str, src: &str| {
        let id = p
            .store
            .from_string(name, src, &ssg_resources::CallSite::in_lang(p.lang()))
            .unwrap();
        let opts = ToCssOptions::from_json(&serde_json::json!({
            "includePaths": ["vendor/kit"],
            "outputStyle": "compressed"
        }))
        .unwrap();
        let out = p.store.transform(id, Transform::ToCss(opts)).unwrap();
        String::from_utf8(p.store.content(out).unwrap().to_vec()).unwrap()
    };

    let css = compile(
        "style.site.css",
        "@use \"mixins.scss\" as m;\n@import \"parts/base.scss\";\n@import \"theme.scss\";\n\
         .a { color: $c; padding: $pad; @include m.box; }\n",
    );
    assert!(
        css.contains(".a{color:#123456;padding:4px;margin:1px}"),
        "{css}"
    );
    assert!(css.contains(".theme{v:import-only}"), "{css}");

    // A file next to the importer wins over the load paths, as before.
    let css = compile("scss/page.scss", "@import \"local.scss\";\n");
    assert!(css.contains(".local{v:assets}"), "{css}");

    // The indented syntax, with an unquoted URL.
    let css = compile("style.sass", "@import parts/base.scss\n.b\n  color: $c\n");
    assert!(css.contains(".b{color:#123456}"), "{css}");

    // Still an error when no load path has the file.
    let id = p
        .store
        .from_string(
            "missing.scss",
            "@import \"nowhere.scss\";\n",
            &ssg_resources::CallSite::in_lang(p.lang()),
        )
        .unwrap();
    let out = p
        .store
        .transform(id, Transform::ToCss(ToCssOptions::default()))
        .unwrap();
    let err = p.store.realize(out).unwrap_err().to_string();
    assert!(err.contains("Can't find stylesheet to import"), "{err}");
}

//! Checking a case's output against esbuild's: errors, source maps and the files a build writes.

use super::*;

/// The script without an inline source map (checked separately), and that map's JSON.
pub(super) fn split_inline_map(code: &str) -> (String, Option<String>) {
    const MARK: &str = "//# sourceMappingURL=data:application/json;base64,";
    let Some(i) = code.find(MARK) else {
        return (code.to_owned(), None);
    };
    let start = i + MARK.len();
    // The oracle records the map decoded (and pretty-printed), as `DECODED(<map>)`.
    if let Some(map) = code[start..].strip_prefix("DECODED(") {
        let close = map.rfind(')').expect("DECODED(...)");
        let end = start + "DECODED(".len() + close + 1;
        return (
            format!("{}{}", &code[..i], &code[end..]),
            Some(map[..close].to_owned()),
        );
    }
    let end = code[start..].find('\n').map_or(code.len(), |n| start + n);
    let decoded = String::from_utf8(base64_decode(&code[start..end])).unwrap();
    (format!("{}{}", &code[..i], &code[end..]), Some(decoded))
}

pub(super) fn base64_decode(s: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().filter(|&c| c != b'=') {
        let v = ALPHABET.iter().position(|&a| a == c).expect("base64");
        acc = (acc << 6) | u32::try_from(v).unwrap();
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xFF).unwrap());
        }
    }
    out
}

/// The oracle's error text, split into Go's `"file:line:col": message` position (if any) and
/// its message.
pub(super) fn expected_error(err: &str) -> (Option<(String, u32, u32)>, String) {
    let detail = err.split_once("): ").map_or(err, |(_, d)| d);
    if let Some(rest) = detail.strip_prefix('"')
        && let Some((pos, msg)) = rest.split_once("\": ")
    {
        let mut it = pos.rsplitn(3, ':');
        let col = it.next().unwrap().parse().unwrap();
        let line = it.next().unwrap().parse().unwrap();
        let file = it.next().unwrap().to_owned();
        return (Some((file, line, col)), msg.to_owned());
    }
    (None, detail.to_owned())
}

/// Checks an error outcome against the oracle's text; `Err` explains a mismatch.
pub(super) fn check_error(outcome: &Outcome, want: &str, site: &str) -> Result<(), String> {
    let (pos, msg) = expected_error(&want.replace("$SITE", site));
    let quoted = msg.split('"').nth(1).map(str::to_owned).unwrap_or_default();
    // The wording this port keeps from esbuild.
    let same_text =
        msg.starts_with("Could not resolve") || msg.contains("configured target environment");
    let ok = match outcome {
        Outcome::Build(JsBuildError::Build(diags)) => {
            let d = &diags[0];
            let got_pos = d
                .position
                .as_ref()
                .map(|p| (p.file.to_string_lossy().into_owned(), p.line, p.column));
            got_pos == pos && (!same_text || d.text == msg)
        }
        Outcome::Options(OptionsError::Value { value, .. }) => pos.is_none() && *value == quoted,
        Outcome::Options(OptionsError::Type { option, .. }) => {
            msg.contains("error(s) decoding") && msg.to_lowercase().contains(&format!("'{option}'"))
        }
        Outcome::Build(JsBuildError::InjectNotFound(p)) => {
            msg.starts_with("inject: file") && *p == quoted
        }
        Outcome::Build(JsBuildError::InjectAbsolute(_)) => msg.starts_with("inject: absolute"),
        Outcome::Build(JsBuildError::UnsupportedMediaType(m)) => {
            msg.starts_with("unsupported Media Type") && *m == quoted
        }
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(format!("want error {want:?}, got {}", describe(outcome)))
    }
}

pub(super) fn describe(o: &Outcome) -> String {
    match o {
        Outcome::Built(out) => format!("output {:?}", String::from_utf8_lossy(&out.code)),
        Outcome::Raw(b) => format!("raw {:?}", String::from_utf8_lossy(b)),
        Outcome::Options(e) => format!("options error {e:?}"),
        Outcome::Build(e) => format!("build error {e:?}"),
    }
}

/// Checks a source map: every source a file URL whose contents are in `sourcesContent`
/// (unless contents are left out), and every file of the Go implementation's map among them.
pub(super) fn check_map(
    got: &str,
    want: Option<&str>,
    entry_contents: &[u8],
) -> Result<(), String> {
    let got: Json = serde_json::from_str(got).map_err(|e| e.to_string())?;
    // `mappings` is empty when nothing in the script comes from a source.
    if got["version"] != 3 || got["mappings"].as_str().is_none() {
        return Err(format!("not a source map: {got}"));
    }
    let sources = got["sources"].as_array().cloned().unwrap_or_default();
    let contents = got["sourcesContent"].as_array().cloned();
    if let Some(c) = &contents
        && c.len() != sources.len()
    {
        return Err(format!(
            "{} sources for {} contents",
            sources.len(),
            c.len()
        ));
    }
    for (i, s) in sources.iter().enumerate() {
        let url = s.as_str().ok_or("source not a string")?;
        let path = url
            .strip_prefix("file://")
            .ok_or_else(|| format!("source {url} is not a file URL"))?;
        let path = percent_decode(path);
        let Some(c) = contents.as_ref().and_then(|c| c[i].as_str()) else {
            continue;
        };
        // rolldown records the module it made of a JSON or text file, not the file.
        let script = [".js", ".mjs", ".cjs", ".jsx", ".ts", ".tsx"]
            .iter()
            .any(|e| path.ends_with(e));
        match std::fs::read(&path) {
            Ok(bytes) if bytes == c.as_bytes() || !script => {}
            // The entry's contents are what js.Build got, not the file's.
            _ if c.as_bytes() == entry_contents => {}
            Ok(_) => return Err(format!("contents of {path} differ")),
            Err(e) => return Err(format!("source {path}: {e}")),
        }
    }
    if let Some(want) = want {
        let want: Json = serde_json::from_str(want).map_err(|e| e.to_string())?;
        for s in want["sources"].as_array().into_iter().flatten() {
            let Some(url) = s.as_str() else { continue };
            let exists = url
                .strip_prefix("file://")
                .is_some_and(|p| Path::new(&percent_decode(p)).is_file());
            if exists && !sources.contains(s) {
                return Err(format!("source {s} missing from {sources:?}"));
            }
        }
    }
    Ok(())
}

pub(super) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(super) struct Checker<'a> {
    pub(super) node: &'a Path,
    pub(super) dir: PathBuf,
    pub(super) site: String,
}

impl Checker<'_> {
    pub(super) fn check_case(
        &self,
        case: &Json,
        want: &Json,
        outcome: &Outcome,
        contents: &[u8],
    ) -> Result<(), String> {
        let site = &self.site;
        if let Some(err) = want["contentErr"].as_str() {
            return check_error(outcome, err, site);
        }
        let name = case["name"].as_str().unwrap();
        let expected = want["content"]
            .as_str()
            .unwrap()
            .replace("$SITE", site)
            .replace(GO_NAMESPACE, "ns-ssg-");
        let (code, out) = match outcome {
            Outcome::Built(out) => (String::from_utf8(out.code.clone()).unwrap(), out),
            Outcome::Raw(b) => {
                return if *b == expected.as_bytes() {
                    Ok(())
                } else {
                    Err("the asset changed".to_owned())
                };
            }
            other => return Err(format!("want output, got {}", describe(other))),
        };
        let (code, inline_map) = split_inline_map(&code);
        if let Some(map) = &inline_map {
            check_map(map, None, contents).map_err(|e| format!("inline map: {e}"))?;
        }
        let (expected, _) = split_inline_map(&expected);

        let format = format_of(case);
        let want_trace = crate::run::trace(
            self.node,
            &self.dir,
            &format!("{name}-oracle"),
            expected.as_bytes(),
            &format,
        );
        let got_trace = crate::run::trace(
            self.node,
            &self.dir,
            &format!("{name}-ours"),
            code.as_bytes(),
            &format,
        );
        if got_trace != want_trace {
            return Err(format!(
                "behaviour differs:\n--- got\n{got_trace}--- want\n{want_trace}--- code\n{code}"
            ));
        }

        // The entry's names: its asset (in the assets or the `vendor` mount) or, for a
        // concatenation, its target path.
        let entry = case["steps"][0]["path"]
            .as_str()
            .or_else(|| case["steps"][0]["target"].as_str())
            .unwrap_or_default();
        let entry = [
            entry.to_owned(),
            format!("assets/{entry}"),
            format!("node_modules/{}", entry.trim_start_matches("vendor/")),
        ];
        let (got_modules, want_modules) = (
            modules(&code, site, &entry),
            modules(&expected, site, &entry),
        );
        // rolldown leaves out a module it inlined completely (a constant default export).
        if !want_modules.is_empty() && !got_modules.is_subset(&want_modules) {
            return Err(format!(
                "modules: got {got_modules:?}, want {want_modules:?}"
            ));
        }

        let fingerprinted = case["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["op"] == "fingerprint");
        for (path, published) in want["published"].as_object().into_iter().flatten() {
            let published = published
                .as_str()
                .unwrap()
                .replace("$SITE", site)
                .replace(GO_NAMESPACE, "ns-ssg-");
            if let Some(script) = path.strip_suffix(".map") {
                if script != out.target_path {
                    return Err(format!("map at {path}, target {}", out.target_path));
                }
                let map = out.source_map.as_deref().ok_or("no source map")?;
                let map = std::str::from_utf8(map).map_err(|e| e.to_string())?;
                check_map(map, Some(&published), contents)?;
            } else if !fingerprinted && *path != out.target_path {
                return Err(format!("published at {path}, target {}", out.target_path));
            }
        }
        Ok(())
    }
}

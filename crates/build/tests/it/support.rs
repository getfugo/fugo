//! Helpers of the site tests.

use std::fs;
use std::path::Path;

/// Writes `files` (relative path, content) below `dir`.
pub(crate) fn write_files(dir: &Path, files: &[(String, String)]) {
    for (name, content) in files {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, content).expect("write");
    }
}

/// The L1 path normalisation (REWRITE_PLAN.md §7.2): fingerprints `.[0-9a-f]{16,64}.` → `.H.`
/// and processed-image hashes `_hu_[0-9a-f]+` → `_hu_H`.
pub(crate) fn normalize_fingerprints(path: &str) -> String {
    let hex = |s: &str| {
        s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    let mut parts: Vec<String> = path.split('.').map(str::to_owned).collect();
    let last = parts.len().saturating_sub(1);
    for (i, p) in parts.iter_mut().enumerate() {
        if i > 0 && i < last && (16..=64).contains(&p.len()) && hex(p) {
            *p = "H".to_owned();
        }
    }
    let mut out = parts.join(".");
    while let Some(i) = out.find("_hu_") {
        let rest = &out[i + 4..];
        let n = rest.bytes().take_while(u8::is_ascii_hexdigit).count();
        if n == 0 {
            break;
        }
        out = format!("{}_hu_H{}", &out[..i], &rest[n..]);
    }
    out
}

/// The files below `dir`, as `/`-separated relative paths, sorted.
pub(crate) fn files_below(dir: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for e in entries {
            let e = e.expect("entry");
            let path = e.path();
            if e.file_type().expect("type").is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).expect("below root");
                out.push(
                    rel.components()
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

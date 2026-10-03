//! Build script of ssg-layouts: lists `embedded/**` (Go's embedded templates rewritten in
//! Tera, T32) as `include_str!` entries in `$OUT_DIR/embedded.rs`, so adding a template needs no
//! source change. Paths are relative to `embedded/`, `/`-separated and sorted.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let root = manifest.join("embedded");
    if root.is_dir() {
        println!("cargo::rerun-if-changed={}", root.display());
    } else {
        // Re-run when the directory appears (the crate directory's contents change).
        println!("cargo::rerun-if-changed={}", manifest.display());
    }
    let mut files = Vec::new();
    if root.is_dir() {
        collect(&root, "", &mut files);
    }
    files.sort();
    let mut out = String::from("&[\n");
    for (rel, abs) in &files {
        writeln!(out, "    ({rel:?}, include_str!({:?})),", abs.display()).expect("write");
    }
    out.push(']');
    let dest = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets it")).join("embedded.rs");
    fs::write(dest, out).expect("write embedded.rs");
}

fn collect(dir: &Path, below: &str, out: &mut Vec<(String, PathBuf)>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .expect("read embedded/")
        .map(|e| e.expect("dir entry"))
        .collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    for e in entries {
        let name = e.file_name().into_string().expect("UTF-8 file name");
        if name.starts_with('.') {
            continue;
        }
        let rel = if below.is_empty() {
            name.clone()
        } else {
            format!("{below}/{name}")
        };
        let path = e.path();
        if path.is_dir() {
            collect(&path, &rel, out);
        } else {
            out.push((rel, path));
        }
    }
}

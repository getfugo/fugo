//! Release archives: a release build of the program for one target, packaged.
//!
//! The version is `$<PREFIX>_BUILD_VERSION` when that is set (CI's release builds: the tag's
//! version, tools/dev/version.sh), else `version` in `[workspace.package]` of Cargo.toml; the
//! name is the binary's (`[[bin]] name` in crates/cli/Cargo.toml); `<binary> version` must print
//! "<name> v<version>[-<commit>] …" (a smoke test): the version exactly, then the commit the
//! binary names, which is `$<PREFIX>_BUILD_COMMIT` when that is set (as in CI) and otherwise
//! any hex commit or none. The target is a Rust target triple, named in the archive as the Go
//! releases name it (goreleaser's "{{.ProjectName}}_{{.Version}}_{{.Os}}-{{.Arch}}" at
//! 44529028): os linux, darwin or windows, arch amd64 or arm64. Writes
//!
//! - `<out-dir>/<name>_<version>_<os>-<arch>.tar.gz` (`.zip` for Windows)
//! - `<out-dir>/<name>_<version>_<os>-<arch>.tar.gz.sha256`: "<sha256>  <archive>", as
//!   `sha256sum -c` and `shasum -a 256 -c` read it
//!
//! The archive holds, at its root as the Go releases do, the binary, the repository's
//! README.md, LICENSE, NOTICE (the Apache-2.0 attribution notices), PROVENANCE.md, THIRD_PARTY/
//! and, when given, the notices as THIRD_PARTY_NOTICES.txt (the licences of the linked crates,
//! written by `notices`). Entries are sorted, owned by root and dated `SOURCE_DATE_EPOCH`
//! (default: now), so the same binary gives the same archive.
//!
//! .github/workflows/ci.yml runs it for every release target; the release job checks the
//! .sha256 files and joins them into <name>_<version>_checksums.txt, the checksums file of the
//! Go releases (DEVELOPMENT.md, "CI and releases").

use std::io::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{Fail, fail};

fn read_toml(path: &Path) -> Result<toml::Table, Fail> {
    let text = std::fs::read_to_string(path).map_err(|e| fail!("{}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| fail!("{}: {e}", path.display()))
}

/// The version the binary was built with: `$<PREFIX>_BUILD_VERSION`, else the workspace's.
///
/// # Errors
/// An unreadable Cargo.toml.
pub fn build_version() -> Result<String, Fail> {
    if let Some(v) = std::env::var(ssg_base::env_var!("BUILD_VERSION"))
        .ok()
        .filter(|v| !v.is_empty())
    {
        return Ok(v);
    }
    let t = read_toml(&crate::root().join("Cargo.toml"))?;
    t.get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| fail!("Cargo.toml: no [workspace.package] version"))
}

/// The binary's name: `[[bin]] name` of crates/cli/Cargo.toml.
///
/// # Errors
/// An unreadable manifest.
pub fn app_name() -> Result<String, Fail> {
    let t = read_toml(&crate::root().join("crates/cli/Cargo.toml"))?;
    t.get("bin")
        .and_then(toml::Value::as_array)
        .and_then(|b| b.first())
        .and_then(|b| b.get("name"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| fail!("crates/cli/Cargo.toml: no [[bin]] name"))
}

/// `<binary> version` must print the version line of `version` (and of
/// `$<PREFIX>_BUILD_COMMIT` when set); returns the line.
fn check_binary(binary: &Path, version: &str, app: &str) -> Result<String, Fail> {
    let out = std::process::Command::new(binary)
        .arg("version")
        .output()
        .map_err(|e| fail!("{}: {e}", binary.display()))?;
    if !out.status.success() {
        return Err(fail!(
            "`{} version` failed ({})",
            binary.display(),
            out.status
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let words: Vec<&str> = stdout.split_whitespace().collect();
    let token = (words.len() > 1 && words[0] == app).then(|| words[1]);
    let mut want = format!("v{version}");
    let ok = match std::env::var(ssg_base::env_var!("BUILD_COMMIT"))
        .ok()
        .filter(|c| !c.is_empty())
    {
        Some(commit) => {
            want = format!("{want}-{commit}");
            token == Some(want.as_str())
        }
        None => {
            let base = want.clone();
            want = format!("{want}[-<commit>]");
            token.is_some_and(|t| {
                t == base
                    || t.strip_prefix(&format!("{base}-")).is_some_and(|c| {
                        (7..=40).contains(&c.len())
                            && c.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
                    })
            })
        }
    };
    if !ok {
        return Err(fail!(
            "`{} version` printed {}, not '{app} {want} …' (${}, else the version of Cargo.toml)",
            binary.display(),
            crate::py::repr_str(&stdout),
            ssg_base::env_var!("BUILD_VERSION"),
        ));
    }
    Ok(stdout.trim().to_owned())
}

/// `<os>-<arch>` of a Rust target triple, in Go's names.
///
/// # Errors
/// A triple with no Go names.
pub fn go_platform(target: &str) -> Result<String, Fail> {
    let mut parts = target.split('-');
    let arch = match parts.next() {
        Some("x86_64") => "amd64",
        Some("aarch64") => "arm64",
        _ => {
            return Err(fail!(
                "no Go os/arch names for the target {}",
                crate::py::repr_str(target)
            ));
        }
    };
    let oses: Vec<&str> = parts
        .filter(|p| matches!(*p, "linux" | "darwin" | "windows"))
        .collect();
    match oses.as_slice() {
        [os] => Ok(format!("{os}-{arch}")),
        _ => Err(fail!(
            "no Go os/arch names for the target {}",
            crate::py::repr_str(target)
        )),
    }
}

/// An archive entry: its name, its source file (none for a directory) and its mode.
type Item = (String, Option<PathBuf>, u32);

fn entries(binary: &Path, notices: Option<&Path>) -> Result<Vec<Item>, Fail> {
    let root = crate::root();
    let name = binary
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut files: Vec<Item> = vec![(name, Some(binary.to_owned()), 0o755)];
    for f in ["README.md", "LICENSE", "NOTICE", "PROVENANCE.md"] {
        files.push((f.to_owned(), Some(root.join(f)), 0o644));
    }
    if let Some(n) = notices {
        files.push((
            "THIRD_PARTY_NOTICES.txt".to_owned(),
            Some(n.to_owned()),
            0o644,
        ));
    }
    let mut dirs = vec!["THIRD_PARTY".to_owned()];
    for rel in walk_with_dirs(&root.join("THIRD_PARTY")) {
        let (rel, is_dir) = rel;
        let name = format!("THIRD_PARTY/{rel}");
        if is_dir {
            dirs.push(name);
        } else {
            files.push((name, Some(root.join("THIRD_PARTY").join(&rel)), 0o644));
        }
    }
    for (_, src, _) in &files {
        let src = src.as_ref().expect("a file");
        if !src.is_file() {
            return Err(fail!("{} is missing", src.display()));
        }
    }
    let mut items: Vec<Item> = dirs.into_iter().map(|d| (d, None, 0o755)).collect();
    items.extend(files);
    items.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(items)
}

/// The entries below `dir`: (`/`-separated relative path, whether it is a directory).
fn walk_with_dirs(dir: &Path) -> Vec<(String, bool)> {
    walkdir::WalkDir::new(dir)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .map(|e| {
            let rel = e.path().strip_prefix(dir).expect("below the root");
            let rel = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            (rel, e.path().is_dir())
        })
        .collect()
}

fn read(src: &Path) -> Result<Vec<u8>, Fail> {
    std::fs::read(src).map_err(|e| fail!("{}: {e}", src.display()))
}

fn write_tar_gz(path: &Path, items: &[Item], mtime: u64) -> Result<(), Fail> {
    let gz = flate2::GzBuilder::new()
        .mtime(u32::try_from(mtime).unwrap_or(0))
        .write(Vec::new(), flate2::Compression::best());
    let mut tar = tar::Builder::new(gz);
    for (name, src, mode) in items {
        let mut h = tar::Header::new_ustar();
        h.set_mode(*mode);
        h.set_mtime(mtime);
        h.set_uid(0);
        h.set_gid(0);
        h.set_username("").map_err(|e| fail!("{name}: {e}"))?;
        h.set_groupname("").map_err(|e| fail!("{name}: {e}"))?;
        let data = match src {
            None => {
                h.set_entry_type(tar::EntryType::Directory);
                Vec::new()
            }
            Some(src) => {
                h.set_entry_type(tar::EntryType::Regular);
                read(src)?
            }
        };
        h.set_size(data.len() as u64);
        let entry_name = if src.is_none() {
            format!("{name}/")
        } else {
            name.clone()
        };
        tar.append_data(&mut h, &entry_name, &data[..])
            .map_err(|e| fail!("{name}: {e}"))?;
    }
    let gz = tar
        .into_inner()
        .map_err(|e| fail!("{}: {e}", path.display()))?;
    let bytes = gz.finish().map_err(|e| fail!("{}: {e}", path.display()))?;
    std::fs::write(path, bytes).map_err(|e| fail!("{}: {e}", path.display()))
}

/// The civil date (year, month, day) of a day count since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).expect("a day");
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).expect("a month");
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// The MS-DOS date and time of a Unix time (UTC; zip dates start in 1980).
fn dos_time(mtime: u64) -> (u16, u16) {
    let t = i64::try_from(mtime.max(315_532_800)).unwrap_or(i64::MAX);
    let (y, m, d) = civil_from_days(t.div_euclid(86_400));
    let secs = t.rem_euclid(86_400);
    let (hh, mm, ss) = (secs / 3600, secs % 3600 / 60, secs % 60);
    let date = u16::try_from(((y - 1980) << 9) | i64::from(m << 5) | i64::from(d)).unwrap_or(0);
    let time = u16::try_from((hh << 11) | (mm << 5) | (ss / 2)).unwrap_or(0);
    (date, time)
}

/// A zip archive (deflated files, stored directories, Unix modes, no data descriptors).
fn write_zip(path: &Path, items: &[Item], mtime: u64) -> Result<(), Fail> {
    let (date, time) = dos_time(mtime);
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let le16 = |v: &mut Vec<u8>, x: u16| v.extend_from_slice(&x.to_le_bytes());
    let le32 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
    let too_big = |what: &str| {
        fail!(
            "{}: {what} too large for a zip without Zip64",
            path.display()
        )
    };
    for (name, src, mode) in items {
        let (name, data, method, attr) = match src {
            None => (
                format!("{name}/"),
                Vec::new(),
                0u16,
                ((0o40000 | mode) << 16) | 0x10,
            ),
            Some(src) => (name.clone(), read(src)?, 8u16, (0o100000 | mode) << 16),
        };
        let crc = crc32fast::hash(&data);
        let stored = if method == 8 {
            let mut z =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            z.write_all(&data).map_err(|e| fail!("{name}: {e}"))?;
            z.finish().map_err(|e| fail!("{name}: {e}"))?
        } else {
            data.clone()
        };
        let offset = u32::try_from(out.len()).map_err(|_| too_big("the archive"))?;
        let (csize, usize_) = (
            u32::try_from(stored.len()).map_err(|_| too_big(&name))?,
            u32::try_from(data.len()).map_err(|_| too_big(&name))?,
        );
        let name_len = u16::try_from(name.len()).map_err(|_| too_big(&name))?;
        // Local file header.
        le32(&mut out, 0x0403_4b50);
        le16(&mut out, 20); // version needed
        le16(&mut out, 0); // flags
        le16(&mut out, method);
        le16(&mut out, time);
        le16(&mut out, date);
        le32(&mut out, crc);
        le32(&mut out, csize);
        le32(&mut out, usize_);
        le16(&mut out, name_len);
        le16(&mut out, 0); // extra
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&stored);
        // Central directory entry.
        le32(&mut central, 0x0201_4b50);
        le16(&mut central, (3 << 8) | 20); // made by Unix, 2.0
        le16(&mut central, 20);
        le16(&mut central, 0);
        le16(&mut central, method);
        le16(&mut central, time);
        le16(&mut central, date);
        le32(&mut central, crc);
        le32(&mut central, csize);
        le32(&mut central, usize_);
        le16(&mut central, name_len);
        le16(&mut central, 0); // extra
        le16(&mut central, 0); // comment
        le16(&mut central, 0); // disk
        le16(&mut central, 0); // internal attributes
        le32(&mut central, attr);
        le32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let count = u16::try_from(items.len()).map_err(|_| too_big("the entry count"))?;
    let cd_offset = u32::try_from(out.len()).map_err(|_| too_big("the archive"))?;
    let cd_size = u32::try_from(central.len()).map_err(|_| too_big("the central directory"))?;
    out.extend_from_slice(&central);
    le32(&mut out, 0x0605_4b50);
    le16(&mut out, 0);
    le16(&mut out, 0);
    le16(&mut out, count);
    le16(&mut out, count);
    le32(&mut out, cd_size);
    le32(&mut out, cd_offset);
    le16(&mut out, 0);
    std::fs::write(path, out).map_err(|e| fail!("{}: {e}", path.display()))
}

/// Packages `binary` for `target` into `out`.
///
/// # Errors
/// A binary that does not print its version line, an unknown target, missing files, or a
/// failed write.
pub fn run(binary: &Path, target: &str, out: &Path, notices: Option<&Path>) -> Result<(), Fail> {
    let version = build_version()?;
    let app = app_name()?;
    let line = check_binary(binary, &version, &app)?;
    let ext = if target.contains("windows") {
        "zip"
    } else {
        "tar.gz"
    };
    let archive = out.join(format!("{app}_{version}_{}.{ext}", go_platform(target)?));
    let mtime = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<u64>()
                .map_err(|e| fail!("SOURCE_DATE_EPOCH: {e}"))
        })
        .transpose()?
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        });
    std::fs::create_dir_all(out).map_err(|e| fail!("{}: {e}", out.display()))?;
    let items = entries(binary, notices)?;
    if ext == "zip" {
        write_zip(&archive, &items, mtime)?;
    } else {
        write_tar_gz(&archive, &items, mtime)?;
    }
    let bytes = std::fs::read(&archive).map_err(|e| fail!("{}: {e}", archive.display()))?;
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let file_name = archive
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let sha_line = format!("{digest}  {file_name}\n");
    let sha_path = PathBuf::from(format!("{}.sha256", archive.display()));
    std::fs::write(&sha_path, &sha_line).map_err(|e| fail!("{}: {e}", sha_path.display()))?;
    println!(
        "{line}: {} ({} bytes, {} entries)",
        archive.display(),
        bytes.len(),
        items.len()
    );
    print!("{sha_line}");
    Ok(())
}

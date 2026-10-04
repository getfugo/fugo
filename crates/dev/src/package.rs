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

use std::io::{Cursor, Write as _};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

use crate::{Fail, fail};

fn read_toml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, Fail> {
    let text = std::fs::read_to_string(path).map_err(|e| fail!("{}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| fail!("{}: {e}", path.display()))
}

/// The root Cargo.toml, as far as it is read here.
#[derive(Deserialize)]
struct RootManifest {
    workspace: Workspace,
}

#[derive(Deserialize)]
struct Workspace {
    package: WorkspacePackage,
}

#[derive(Deserialize)]
struct WorkspacePackage {
    version: String,
}

/// crates/cli/Cargo.toml, as far as it is read here.
#[derive(Deserialize)]
struct CliManifest {
    bin: Vec<Bin>,
}

#[derive(Deserialize)]
struct Bin {
    name: String,
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
    let m: RootManifest = read_toml(&crate::root().join("Cargo.toml"))?;
    Ok(m.workspace.package.version)
}

/// The binary's name: `[[bin]] name` of crates/cli/Cargo.toml.
///
/// # Errors
/// An unreadable manifest.
pub fn app_name() -> Result<String, Fail> {
    let m: CliManifest = read_toml(&crate::root().join("crates/cli/Cargo.toml"))?;
    m.bin
        .into_iter()
        .next()
        .map(|b| b.name)
        .ok_or_else(|| fail!("crates/cli/Cargo.toml: no [[bin]] name"))
}

/// Whether `token` is `v<version>-<commit>` for an abbreviated or full hex commit.
fn with_any_commit(token: &str, base: &str) -> bool {
    token == base
        || token
            .strip_prefix(base)
            .and_then(|t| t.strip_prefix('-'))
            .is_some_and(|c| {
                (7..=40).contains(&c.len())
                    && c.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            })
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
    let mut words = stdout.split_whitespace();
    let token = (words.next() == Some(app)).then(|| words.next()).flatten();
    let base = format!("v{version}");
    let (ok, want) = match std::env::var(ssg_base::env_var!("BUILD_COMMIT"))
        .ok()
        .filter(|c| !c.is_empty())
    {
        Some(commit) => {
            let want = format!("{base}-{commit}");
            (token == Some(want.as_str()), want)
        }
        None => (
            token.is_some_and(|t| with_any_commit(t, &base)),
            format!("{base}[-<commit>]"),
        ),
    };
    if !ok {
        return Err(fail!(
            "`{} version` printed {stdout:?}, not '{app} {want} …' (${}, else the version of Cargo.toml)",
            binary.display(),
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
    let unknown = || fail!("no Go os/arch names for the target {target:?}");
    let mut parts = target.split('-');
    let arch = match parts.next() {
        Some("x86_64") => "amd64",
        Some("aarch64") => "arm64",
        _ => return Err(unknown()),
    };
    match parts
        .filter(|p| matches!(*p, "linux" | "darwin" | "windows"))
        .collect::<Vec<_>>()[..]
    {
        [os] => Ok(format!("{os}-{arch}")),
        _ => Err(unknown()),
    }
}

/// An archive entry: a directory (its name without the trailing `/`) or a file and its source.
struct Item {
    name: String,
    source: Option<PathBuf>,
    mode: u32,
}

fn entries(binary: &Path, notices: Option<&Path>) -> Result<Vec<Item>, Fail> {
    let root = crate::root();
    let file = |name: &str, source: PathBuf, mode| Item {
        name: name.to_owned(),
        source: Some(source),
        mode,
    };
    let binary_name = binary
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut items = vec![file(&binary_name, binary.to_owned(), 0o755)];
    for f in ["README.md", "LICENSE", "NOTICE", "PROVENANCE.md"] {
        items.push(file(f, root.join(f), 0o644));
    }
    if let Some(n) = notices {
        items.push(file("THIRD_PARTY_NOTICES.txt", n.to_owned(), 0o644));
    }
    items.push(Item {
        name: "THIRD_PARTY".into(),
        source: None,
        mode: 0o755,
    });
    let third_party = root.join("THIRD_PARTY");
    for e in walkdir::WalkDir::new(&third_party)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        let rel = e.path().strip_prefix(&third_party).expect("below the root");
        let rel: Vec<_> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect();
        let name = format!("THIRD_PARTY/{}", rel.join("/"));
        items.push(if e.path().is_dir() {
            Item {
                name,
                source: None,
                mode: 0o755,
            }
        } else {
            file(&name, e.path().to_owned(), 0o644)
        });
    }
    if let Some(missing) = items
        .iter()
        .filter_map(|i| i.source.as_ref())
        .find(|s| !s.is_file())
    {
        return Err(fail!("{} is missing", missing.display()));
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(items)
}

fn read(src: &Path) -> Result<Vec<u8>, Fail> {
    std::fs::read(src).map_err(|e| fail!("{}: {e}", src.display()))
}

fn write_tar_gz(items: &[Item], mtime: jiff::Timestamp) -> Result<Vec<u8>, Fail> {
    let seconds = mtime.as_second();
    let gz = flate2::GzBuilder::new()
        .mtime(u32::try_from(seconds).unwrap_or(0))
        .write(Vec::new(), flate2::Compression::best());
    let mut tar = tar::Builder::new(gz);
    for item in items {
        let mut h = tar::Header::new_ustar();
        h.set_mode(item.mode);
        h.set_mtime(u64::try_from(seconds).unwrap_or(0));
        h.set_uid(0);
        h.set_gid(0);
        let (path, data) = match &item.source {
            None => {
                h.set_entry_type(tar::EntryType::Directory);
                (format!("{}/", item.name), Vec::new())
            }
            Some(src) => {
                h.set_entry_type(tar::EntryType::Regular);
                (item.name.clone(), read(src)?)
            }
        };
        h.set_size(data.len() as u64);
        tar.append_data(&mut h, &path, &data[..])
            .map_err(|e| fail!("{}: {e}", item.name))?;
    }
    let gz = tar.into_inner().map_err(|e| fail!("tar: {e}"))?;
    gz.finish().map_err(|e| fail!("gzip: {e}"))
}

/// A zip archive: deflated files, directories, Unix modes, the time in UTC (zip times have no
/// zone and start in 1980).
fn write_zip(items: &[Item], mtime: jiff::Timestamp) -> Result<Vec<u8>, Fail> {
    let t = mtime.to_zoned(jiff::tz::TimeZone::UTC);
    let narrow = |x: i8| u8::try_from(x).unwrap_or(0);
    let time = zip::DateTime::from_date_and_time(
        u16::try_from(t.year()).unwrap_or(1980),
        narrow(t.month()),
        narrow(t.day()),
        narrow(t.hour()),
        narrow(t.minute()),
        narrow(t.second()),
    )
    .unwrap_or_default();
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for item in items {
        let options = SimpleFileOptions::default()
            .last_modified_time(time)
            .unix_permissions(item.mode);
        let e = |e: &dyn std::fmt::Display| fail!("{}: {e}", item.name);
        match &item.source {
            None => zip.add_directory(&item.name, options).map_err(|x| e(&x))?,
            Some(src) => {
                zip.start_file(
                    &item.name,
                    options.compression_method(zip::CompressionMethod::Deflated),
                )
                .map_err(|x| e(&x))?;
                zip.write_all(&read(src)?).map_err(|x| e(&x))?;
            }
        }
    }
    Ok(zip.finish().map_err(|e| fail!("zip: {e}"))?.into_inner())
}

/// `SOURCE_DATE_EPOCH`, else now.
fn source_date() -> Result<jiff::Timestamp, Fail> {
    match std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .filter(|s| !s.is_empty())
    {
        Some(s) => {
            let seconds: i64 = s.parse().map_err(|e| fail!("SOURCE_DATE_EPOCH: {e}"))?;
            jiff::Timestamp::from_second(seconds).map_err(|e| fail!("SOURCE_DATE_EPOCH: {e}"))
        }
        None => Ok(jiff::Timestamp::now()),
    }
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
    let zip = target.contains("windows");
    let file_name = format!(
        "{app}_{version}_{}.{}",
        go_platform(target)?,
        if zip { "zip" } else { "tar.gz" }
    );
    let mtime = source_date()?;
    let items = entries(binary, notices)?;
    let bytes = if zip {
        write_zip(&items, mtime)?
    } else {
        write_tar_gz(&items, mtime)?
    };
    std::fs::create_dir_all(out).map_err(|e| fail!("{}: {e}", out.display()))?;
    let archive = out.join(&file_name);
    std::fs::write(&archive, &bytes).map_err(|e| fail!("{}: {e}", archive.display()))?;
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let sha_line = format!("{digest}  {file_name}\n");
    let sha_path = out.join(format!("{file_name}.sha256"));
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

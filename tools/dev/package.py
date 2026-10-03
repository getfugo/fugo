#!/usr/bin/env python3
"""Packages a release build of the program for one target (Python 3.11 or later, stdlib only).

Usage:
  package.py <binary> <target> <out-dir> [<notices>]

<version> is $FUGO_BUILD_VERSION when that is set (CI's release builds: the tag's version,
tools/dev/version.py), else `version` in [workspace.package] of Cargo.toml; <name> is the
binary's name (`[[bin]] name` in crates/cli/Cargo.toml); `<binary> version` must print
"<name> v<version>[-<commit>] …" (a smoke test): the version exactly, then the commit the
binary names, which is $FUGO_BUILD_COMMIT when that is set (as in CI) and otherwise any hex
commit or none. <target> is a Rust target triple, named in the archive as the Go releases name
it (goreleaser's "{{.ProjectName}}_{{.Version}}_{{.Os}}-{{.Arch}}" at 44529028): <os> linux,
darwin or windows, <arch> amd64 or arm64. Writes

  <out-dir>/<name>_<version>_<os>-<arch>.tar.gz          (.zip for Windows)
  <out-dir>/<name>_<version>_<os>-<arch>.tar.gz.sha256   "<sha256>  <archive>", as `sha256sum -c`
                                                          and `shasum -a 256 -c` read it

The archive holds, at its root as the Go releases do, the binary, the repository's README.md,
LICENSE, NOTICE (the Apache-2.0 attribution notices), PROVENANCE.md, THIRD_PARTY/ and, when
given, <notices> as THIRD_PARTY_NOTICES.txt (the licences of the linked crates, written by notices.py). Entries are sorted, owned by root and
dated SOURCE_DATE_EPOCH (default: now), so the same binary gives the same archive.

.github/workflows/ci.yml runs it for every release target; the release job checks the .sha256
files and joins them into <name>_<version>_checksums.txt, the checksums file of the Go releases
(DEVELOPMENT.md, "CI and releases").
"""
import gzip
import hashlib
import io
import os
import re
import subprocess
import sys
import tarfile
import time
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
# Go's GOOS and GOARCH names of the parts of a Rust target triple.
GO_OS = {"linux": "linux", "darwin": "darwin", "windows": "windows"}
GO_ARCH = {"x86_64": "amd64", "aarch64": "arm64"}


def build_version():
    """$FUGO_BUILD_VERSION (the version the binary was built with), else `version` of
    [workspace.package] in Cargo.toml."""
    if os.environ.get("FUGO_BUILD_VERSION"):
        return os.environ["FUGO_BUILD_VERSION"]
    with open(ROOT / "Cargo.toml", "rb") as f:
        return tomllib.load(f)["workspace"]["package"]["version"]


def app_name():
    """The binary's name: `[[bin]] name` of crates/cli/Cargo.toml."""
    with open(ROOT / "crates" / "cli" / "Cargo.toml", "rb") as f:
        return tomllib.load(f)["bin"][0]["name"]


def check_binary(binary, version):
    """`<binary> version` must print the version line of `version` (and of $FUGO_BUILD_COMMIT
    when set); returns the line."""
    out = subprocess.run([str(binary), "version"], capture_output=True, text=True,
                         encoding="utf-8", check=True)
    words = out.stdout.split()
    token = words[1] if len(words) > 1 and words[0] == app_name() else None
    want = f"v{version}"
    commit = os.environ.get("FUGO_BUILD_COMMIT")
    if commit:
        ok = token == f"{want}-{commit}"
        want = f"{want}-{commit}"
    else:
        ok = token is not None and (
            token == want or re.fullmatch(rf"{re.escape(want)}-[0-9a-f]{{7,40}}", token))
        want = f"{want}[-<commit>]"
    if not ok:
        sys.exit(f"package.py: `{binary} version` printed {out.stdout!r}, "
                 f"not '{app_name()} {want} …' ($FUGO_BUILD_VERSION, else the version of Cargo.toml)")
    return out.stdout.strip()


def go_platform(target):
    """`<os>-<arch>` of a Rust target triple, in Go's names."""
    arch, *rest = target.split("-")
    oses = [GO_OS[part] for part in rest if part in GO_OS]
    if arch not in GO_ARCH or len(oses) != 1:
        sys.exit(f"package.py: no Go os/arch names for the target {target!r}")
    return f"{oses[0]}-{GO_ARCH[arch]}"


def entries(binary, notices=None):
    """(name in the archive, source file, mode), sorted; directories have no source."""
    files = [
        (binary.name, binary, 0o755),
        ("README.md", ROOT / "README.md", 0o644),
        ("LICENSE", ROOT / "LICENSE", 0o644),
        ("NOTICE", ROOT / "NOTICE", 0o644),
        ("PROVENANCE.md", ROOT / "PROVENANCE.md", 0o644),
    ]
    if notices is not None:
        files.append(("THIRD_PARTY_NOTICES.txt", notices, 0o644))
    third_party = ROOT / "THIRD_PARTY"
    dirs = {"THIRD_PARTY"}
    for path in sorted(third_party.rglob("*")):
        rel = path.relative_to(third_party.parent).as_posix()
        if path.is_dir():
            dirs.add(rel)
        else:
            files.append((rel, path, 0o644))
    for name, src, _ in files:
        if not src.is_file():
            sys.exit(f"package.py: {src} is missing")
    return sorted([(d, None, 0o755) for d in dirs] + files)


def write_tar_gz(path, items, mtime):
    raw = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=mtime, compresslevel=9) as gz:
        with tarfile.open(fileobj=gz, mode="w", format=tarfile.PAX_FORMAT) as tar:
            for name, src, mode in items:
                info = tarfile.TarInfo(name)
                info.mode, info.mtime = mode, mtime
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                if src is None:
                    info.type = tarfile.DIRTYPE
                    tar.addfile(info)
                else:
                    data = src.read_bytes()
                    info.size = len(data)
                    tar.addfile(info, io.BytesIO(data))
    path.write_bytes(raw.getvalue())


def write_zip(path, items, mtime):
    stamp = time.gmtime(max(mtime, 315532800))[:6]  # zip dates start in 1980
    with zipfile.ZipFile(path, "w") as z:
        for name, src, mode in items:
            info = zipfile.ZipInfo(name + ("/" if src is None else ""), date_time=stamp)
            info.create_system = 3  # Unix, so external_attr carries the mode
            if src is None:
                info.external_attr = (0o40000 | mode) << 16 | 0x10
                z.writestr(info, b"")
            else:
                info.external_attr = (0o100000 | mode) << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                z.writestr(info, src.read_bytes())


def main(argv):
    if len(argv) not in (4, 5):
        sys.exit(__doc__)
    binary, target, out = Path(argv[1]), argv[2], Path(argv[3])
    notices = Path(argv[4]) if len(argv) == 5 else None
    version = build_version()
    line = check_binary(binary, version)
    ext = "zip" if "windows" in target else "tar.gz"
    archive = out / f"{app_name()}_{version}_{go_platform(target)}.{ext}"
    mtime = int(os.environ.get("SOURCE_DATE_EPOCH") or time.time())
    out.mkdir(parents=True, exist_ok=True)
    items = entries(binary, notices)
    (write_zip if ext == "zip" else write_tar_gz)(archive, items, mtime)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    sha_line = f"{digest}  {archive.name}\n"
    Path(f"{archive}.sha256").write_text(sha_line, encoding="ascii", newline="\n")
    print(f"{line}: {archive} ({archive.stat().st_size} bytes, {len(items)} entries)")
    print(sha_line, end="")


if __name__ == "__main__":
    main(sys.argv)

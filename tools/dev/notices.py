#!/usr/bin/env python3
"""Writes the third-party licence notices for a release build (Python stdlib only).

Usage:
  notices.py <target> <out-file>

The notices cover every package linked into the binary for <target>: the normal-dependency
closure of the `ssg-cli` package in `cargo metadata --filter-platform <target>`. Build and dev
dependencies are not linked into the binary, and neither are proc-macro packages (they run in
the compiler), so the walk does not enter them. For each package the file lists its name,
version, licence expression, authors and where its source is, followed by the licence and notice
files the package ships: files in the package root named LICENSE*, LICENCE*, COPYING*, NOTICE*,
COPYRIGHT*, UNLICENSE*, AUTHORS* or PATENTS*, and for packages that build C code (`links`, or a
`-sys` name) the same files anywhere below the package, which covers vendored C libraries such
as libwebp. A text already printed for an earlier package is referenced instead of repeated.

A package that ships no licence file but is licensed under MIT (alone or as one choice) gets the
MIT licence text with the authors from its Cargo.toml; one under Apache-2.0 refers to the
Apache-2.0 text another linked package ships; one under a licence whose standard text is in
THIRD_PARTY/spdx/<SPDX id>.txt (BSD-3-Clause, CC0-1.0, BSL-1.0) gets that text, the copyright
holders being its authors. Any other package without a licence file
is printed and the script exits with status 1 unless --allow-missing is given, so a new
dependency like that is noticed. Workspace members are covered by the repository's LICENSE.

package.py puts the result into the release archives as THIRD_PARTY_NOTICES.txt
(.github/workflows/ci.yml, DEVELOPMENT.md "CI and releases").
"""
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
SPDX_TEXTS = ROOT / "THIRD_PARTY" / "spdx"
PREFIXES = ("license", "licence", "copying", "notice", "copyright", "unlicense", "authors",
            "patents")
SKIP_DIRS = {".git", "tests", "test", "benches", "examples", "target", "fuzz"}
MIT = """Permission is hereby granted, free of charge, to any person obtaining a copy of this
software and associated documentation files (the "Software"), to deal in the Software without
restriction, including without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or
substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
"""


def is_notice(path):
    name = path.name.lower()
    return path.is_file() and name.startswith(PREFIXES) and not name.endswith(
        (".rs", ".py", ".sh", ".toml", ".json", ".yml", ".yaml", ".c", ".h"))


def notice_files(pkg):
    root = Path(pkg["manifest_path"]).parent
    found = sorted(p for p in root.iterdir() if is_notice(p))
    if pkg.get("links") or pkg["name"].endswith("-sys"):
        for path in sorted(root.rglob("*")):
            rel = path.relative_to(root)
            if len(rel.parts) > 1 and not SKIP_DIRS.intersection(rel.parts[:-1]) and is_notice(path):
                found.append(path)
    return root, found


def is_proc_macro(pkg):
    return any("proc-macro" in t["kind"] for t in pkg["targets"])


def closure(meta, target_pkg):
    """Package ids linked into `target_pkg`: its normal-dependency closure without proc macros,
    itself excluded."""
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = next(p["id"] for p in meta["packages"]
                if p["name"] == target_pkg and p["id"] in meta["workspace_members"])
    seen, todo = set(), [root]
    while todo:
        node = nodes[todo.pop()]
        for dep in node["deps"]:
            pid = dep["pkg"]
            if (any(k["kind"] is None for k in dep["dep_kinds"]) and pid not in seen
                    and not is_proc_macro(packages[pid])):
                seen.add(pid)
                todo.append(pid)
    return seen


def offers(expr, licence):
    return licence in (expr or "").replace("(", " ").replace(")", " ").replace("/", " ").split()


def offers_mit(expr):
    return offers(expr, "MIT")


def apache_text(ids, packages):
    """The first Apache-2.0 licence file a linked package ships: `(text, "<name> <version>, <file>")`."""
    for pid in ids:
        root, files = notice_files(packages[pid])
        for path in files:
            if "apache" in path.name.lower():
                text = path.read_text(encoding="utf-8", errors="replace").rstrip() + "\n"
                if "Apache License" in text and "Version 2.0" in text:
                    return text, f"{packages[pid]['name']} {packages[pid]['version']}, {path.relative_to(root).as_posix()}"
    return None


def main(argv):
    allow_missing = "--allow-missing" in argv
    args = [a for a in argv[1:] if a != "--allow-missing"]
    if len(args) != 2:
        sys.exit(__doc__)
    target, out = args[0], Path(args[1])
    # cargo prints UTF-8 whatever the locale (Windows' default is cp1252).
    meta = json.loads(subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", target],
        cwd=ROOT, capture_output=True, check=True).stdout.decode("utf-8"))
    packages = {p["id"]: p for p in meta["packages"]}
    members = set(meta["workspace_members"])
    app = next(t["name"] for p in meta["packages"] if p["name"] == "ssg-cli"
               for t in p["targets"] if "bin" in t["kind"])
    ids = sorted((i for i in closure(meta, "ssg-cli") if i not in members),
                 key=lambda i: (packages[i]["name"], packages[i]["version"]))

    missing = []
    printed = {}  # licence text -> "<package> <version>, <file>" that printed it
    parts = [
        f"Third-party software in {app}\n",
        "=" * len(f"Third-party software in {app}") + "\n\n",
        f"{app} for {target} links the {len(ids)} packages below. Their licence and notice\n",
        "files follow each entry. The source code of every package is available from\n",
        "https://crates.io/crates/<name>/<version> and from the repository listed with it.\n",
        "Files copied into the source tree (data, fonts, scripts) are listed in\n",
        "PROVENANCE.md, with their licences in THIRD_PARTY/.\n",
    ]
    for pid in ids:
        pkg = packages[pid]
        name, version, expr = pkg["name"], pkg["version"], pkg.get("license")
        root, files = notice_files(pkg)
        parts.append("\n" + "=" * 78 + "\n")
        parts.append(f"{name} {version}\n")
        parts.append(f"License: {expr or pkg.get('license_file') or 'unknown'}\n")
        if pkg.get("authors"):
            parts.append(f"Authors: {', '.join(pkg['authors'])}\n")
        parts.append(f"Source: https://crates.io/crates/{name}/{version}\n")
        if pkg.get("repository"):
            parts.append(f"Repository: {pkg['repository']}\n")
        if not files:
            apache = offers(expr, "Apache-2.0") and apache_text(ids, packages)
            if offers_mit(expr):
                parts.append("\nThe package ships no licence file. It is licensed under MIT "
                             "(one of the choices above); the MIT licence text follows, the "
                             "copyright holders being its authors listed above.\n\n" + MIT)
            elif apache:
                text, source = apache
                parts.append("\nThe package ships no licence file. It is licensed under Apache-2.0; "
                             f"the licence text is the same as {source}, printed "
                             + ("above" if text in printed else "here") + ".\n")
                if text not in printed:
                    printed[text] = source
                    parts.append("\n" + text)
            elif spdx := next((t for t in sorted(SPDX_TEXTS.glob("*.txt")) if offers(expr, t.stem)), None):
                text = spdx.read_text(encoding="utf-8").rstrip() + "\n"
                source = f"THIRD_PARTY/spdx/{spdx.name}"
                parts.append(f"\nThe package ships no licence file. It is licensed under {spdx.stem}; "
                             "the copyright holders being its authors listed above, the licence text "
                             + (f"is {source}, printed above.\n" if text in printed else "follows.\n"))
                if text not in printed:
                    printed[text] = source
                    parts.append("\n" + text)
            else:
                missing.append(f"{name} {version} ({expr})")
                parts.append("\n(the package ships no licence file; its licence is the "
                             "expression above)\n")
        for path in files:
            rel = path.relative_to(root).as_posix()
            text = path.read_text(encoding="utf-8", errors="replace").rstrip() + "\n"
            if text in printed:
                parts.append(f"\n--- {rel}: the same text as {printed[text]} above ---\n")
            else:
                printed[text] = f"{name} {version}, {rel}"
                parts.append(f"\n--- {rel} ---\n\n{text}")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text("".join(parts), encoding="utf-8", newline="\n")
    print(f"notices.py: {len(ids)} packages, {out.stat().st_size} bytes -> {out}")
    if missing:
        print("notices.py: no licence file in:\n  " + "\n  ".join(missing), file=sys.stderr)
        if not allow_missing:
            sys.exit(1)


if __name__ == "__main__":
    main(sys.argv)

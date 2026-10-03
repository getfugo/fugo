#!/usr/bin/env python3
"""Self-test of structdiff.py (docs/rust-port/REWRITE_PLAN.md §7.2): synthetic perturbations of
a Go build's output, each compared with the unperturbed output, must be classified exactly as
expected (the right file, level and difference class, and nothing else):

  1. drop a file                        L1 missing
  2. add a file                         L1 extra
  3. change an internal link            L2 html links
  4. reorder attributes                 ignored (no difference)
  5. percent-encode a Thai href         ignored (no difference; a percent-encoded one is decoded)
  6. split code into token spans        ignored (no difference: a highlighter's span structure)
  7. change visible text                L3 text
  8. change image dimensions            L4 image
  9. change an RSS item link            L2 xml items
 10. drop the most linked page          L1 missing, and L2 dangling links in every file that
                                        links to it (link integrity)

then the ratchet: against a baseline of the unperturbed output, perturbation 7 unlisted fails;
listed in a changes file it passes, and --update writes it into the baseline; the unperturbed
output against that baseline is an unlisted improvement, which does not fail.

Usage: selftest.py [--go-out DIR [--project DIR]] [--keep]

The Go output: --go-out (a publish directory, --project its site directory), else Go's testsite
output (crates/build/tests/it/testsite-go.txtar) with a Thai page and a PNG added (the
testsite has neither). Python stdlib only; everything happens in a temporary directory.
"""
import argparse
import os
import re
import shutil
import struct
import sys
import tempfile
import urllib.parse
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, HERE)
sys.dont_write_bytecode = True  # the sibling modules below: no __pycache__ in the tree
import manifest as mf  # noqa: E402
import structdiff as sd  # noqa: E402

TESTSITE_GO = os.path.join(ROOT, "crates", "build", "tests", "it", "testsite-go.txtar")
THAI_RE = re.compile(r"[฀-๿]|%E0%B[89]%[89AB][0-9A-F]", re.I)


# ---------------------------------------------------------------------------------------------
# The Go output

def read_txtar(path):
    files, name, buf = {}, None, []
    with open(path, encoding="utf-8", newline="") as fh:
        for line in fh.read().split("\n"):
            if line.startswith("-- ") and line.endswith(" --") and len(line) > 6:
                if name is not None:
                    files[name] = "\n".join(buf)
                name, buf = line[3:-3].strip(), []
            elif name is not None:
                buf.append(line)
    if name is not None:
        files[name] = "\n".join(buf)
    return {k: v if v.endswith("\n") or not v else v + "\n" for k, v in files.items()}


def png(width, height):
    """A valid RGB PNG of the given size."""
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    raw = b"".join(b"\x00" + b"\x80\x40\x20" * width for _ in range(height))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def testsite_output(dest):
    """Go's testsite output plus a Thai page and an image (what the testsite lacks)."""
    for name, text in read_txtar(TESTSITE_GO).items():
        write(os.path.join(dest, name), text)
    th = "/th/%E0%B8%82%E0%B8%99%E0%B8%A1/"
    write(os.path.join(dest, "th", "ขนม", "index.html"),
          "<!DOCTYPE html><html lang=\"th\"><head><title>ขนม</title></head><body>"
          f"<h1 id=\"khanom\">ขนม</h1><p>หน้าขนมไทย with a picture</p><a href=\"{th}\">ขนม</a> "
          "<a href=\"/\">Home</a><img src=\"/img/selftest.png\" alt=\"\"></body></html>\n")
    with open(os.path.join(dest, "img", "selftest.png") if os.path.isdir(os.path.join(dest, "img"))
              else mkdirs(os.path.join(dest, "img", "selftest.png")), "wb") as fh:
        fh.write(png(3, 2))


def mkdirs(path):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    return path


def write(path, text):
    with open(mkdirs(path), "w", encoding="utf-8", newline="") as fh:
        fh.write(text)


# ---------------------------------------------------------------------------------------------
# Comparison

class Run:
    def __init__(self, site, project, structure):
        self.site, self.project, self.structure = site, project, structure

    def side(self, name, out):
        return sd.Side(name, sd.load_manifest(out, self.project, self.site, "minified"),
                       sd.load_manifest(out, self.project, self.site, "unminified"), self.structure)

    def compare(self, ref, cand):
        return sd.summarize(sd.compare(self.site, self.side("go", ref), self.side("perturbed", cand)))


def diffs(res):
    """{(key, level): classes} of every entry that is not ok."""
    out = {}
    for section in ("files", "structure"):
        for k, levels in res[section].items():
            for lv, e in levels.items():
                if e["status"] != "ok":
                    out[(k, lv)] = tuple(e.get("classes", []))
    return out


def html_files(root):
    return [r for r in mf.walk(root) if r.endswith(".html")]


def read(root, rel):
    with open(os.path.join(root, rel), encoding="utf-8") as fh:
        return fh.read()


def put(root, rel, text):
    with open(os.path.join(root, rel), "w", encoding="utf-8", newline="") as fh:
        fh.write(text)


def manifest_of(root, run):
    return sd.load_manifest(root, run.project, run.site, "unminified")


# ---------------------------------------------------------------------------------------------
# The perturbations: each changes the copy `d` and returns (description, {(key, level): class})

def linking(man, rel):
    """The files with a link (or alias target) that only `rel` resolves."""
    files = {mf.norm_path(r) for r in man["files"]}
    rest = files - {mf.norm_path(rel)}
    out = []
    for r, e in man["files"].items():
        if r == rel or e["type"] not in ("html", "alias"):
            continue
        l2 = e.get("L2") or {}
        links = list(l2.get("links", [])) + ([l2["alias"]] if isinstance(l2.get("alias"), str) else [])
        if any(x.startswith("/") and sd.resolves(x, files) and not sd.resolves(x, rest) for x in links):
            out.append(r)
    return out


def drop_page(d, man, most_linked):
    """Drops an HTML page below the root; the files that link to it get dangling links (L2)."""
    pages = sorted((len(linking(man, r)), r) for r, e in man["files"].items() if e["type"] == "html" and "/" in r)
    if not pages:
        raise SystemExit("drop: no HTML page below the root")
    _, rel = pages[-1] if most_linked else pages[0]
    os.remove(os.path.join(d, rel))
    want = {(rel, "L1"): "L1 missing"}
    want.update({(r, "L2"): "L2 dangling links" for r in linking(man, rel)})
    return f"drop {rel} ({len(want) - 1} files link to it)", want


def drop_file(d, run, man):
    """The least linked page (the plain L1 case)."""
    return drop_page(d, man, most_linked=False)


def drop_linked_page(d, run, man):
    """The most linked page: link integrity (L2) in every file that links to it."""
    return drop_page(d, man, most_linked=True)


def add_file(d, run, man):
    rel = "selftest-added/index.html"
    write(os.path.join(d, rel), "<!DOCTYPE html><html><head><title>Added</title></head>"
                                "<body><p>An added page</p><a href=\"/\">Home</a></body></html>\n")
    return f"add {rel}", {(rel, "L1"): "L1 extra"}


A_HREF_RE = re.compile(r"""(<a\b[^>]*?\shref=)(["'])([^"']*)\2""", re.I)


def change_link(d, run, man):
    """Every `<a href>` of one internal URL on a page, pointed at another existing page."""
    bases = man.get("baseURLs") or []
    urls = mf.Urls(bases)
    pages = sorted(p for p in {mf.page_url(r) for r, e in man["files"].items() if e["type"] == "html"}
                   if p.isascii())
    for rel in html_files(d):
        e = man["files"].get(rel)
        if not e or e["type"] != "html":
            continue
        text = read(d, rel)
        own = mf.page_url(rel)
        links = set(e["L2"]["links"])
        for m in A_HREF_RE.finditer(text):
            raw = m.group(3)
            x = urls.internal(raw, own)
            if not x or x == own or "#" in raw or "?" in raw:
                continue
            # Every occurrence of the value is an <a href>: the page's link set loses it.
            anchors = sum(1 for n in A_HREF_RE.finditer(text) if n.group(3) == raw)
            if text.count(f'"{raw}"') + text.count(f"'{raw}'") != anchors:
                continue
            target = next((p for p in pages if p not in links and p != own), None)
            if target is None:
                continue
            base = next((b for b in bases if raw.startswith(b)), None)
            new = base.rstrip("/") + target if base else target
            text = A_HREF_RE.sub(lambda n: n.group(1) + n.group(2) + (new if n.group(3) == raw else n.group(3))
                                 + n.group(2), text)
            put(d, rel, text)
            return f"{rel}: <a href> {raw} -> {new}", {(rel, "L2"): "L2 html links"}
    raise SystemExit("change link: no page with a suitable internal link")


SKIP_RE = re.compile(r"<!--.*?-->|<script\b.*?</script\s*>|<style\b.*?</style\s*>", re.S | re.I)
TAG_RE = re.compile(r"<([a-zA-Z][a-zA-Z0-9-]*)(\s[^<>]*?)(\s*/?)>")
ATTR_RE = re.compile(r"""\s+([^\s"'=<>/]+)(?:\s*=\s*("[^"]*"|'[^']*'|[^\s"'=<>`]+))?""")


def reorder_tags(text):
    """Every start tag with two or more attributes, attributes reversed (comments, scripts and
    styles untouched); returns the text and the number of tags changed."""
    count = 0

    def tag(m):
        nonlocal count
        attrs = list(ATTR_RE.finditer(m.group(2)))
        # Only tags whose attributes parse completely (a quoted `>` ends TAG_RE early).
        if len(attrs) < 2 or "".join(a.group(0) for a in attrs) != m.group(2):
            return m.group(0)
        count += 1
        return "<" + m.group(1) + "".join(" " + a.group(0).strip() for a in reversed(attrs)) + m.group(3) + ">"

    out, pos = [], 0
    for s in SKIP_RE.finditer(text):
        out.append(TAG_RE.sub(tag, text[pos:s.start()]))
        out.append(s.group(0))
        pos = s.end()
    out.append(TAG_RE.sub(tag, text[pos:]))
    return "".join(out), count


def reorder_attributes(d, run, man):
    total = files = 0
    for rel in html_files(d):
        text, n = reorder_tags(read(d, rel))
        if n:
            put(d, rel, text)
            total += n
            files += 1
    if not total:
        raise SystemExit("reorder: no tag with two attributes")
    return f"attributes reversed in {total} tags of {files} files", {}


HREF_RE = re.compile(r"""(\shref=)(["'])([^"']*)\2""", re.I)


def thai_href(d, run, man):
    changed = []
    for rel in html_files(d):
        text = read(d, rel)

        def swap(m):
            v = m.group(3)
            if not THAI_RE.search(v):
                return m.group(0)
            if re.search(r"[฀-๿]", v):
                nv = urllib.parse.quote(v, safe=":/?#[]@!$&'()*+,;=%~")
            else:
                nv = urllib.parse.unquote(v)
            changed.append((rel, v, nv))
            return m.group(1) + m.group(2) + nv + m.group(2)

        new = HREF_RE.sub(swap, text)
        if new != text:
            put(d, rel, new)
    if not changed:
        raise SystemExit("thai href: no Thai href")
    rel, v, nv = changed[0]
    how = "percent-encoded" if "%" in nv and "%" not in v else "percent-decoded (Tera's form)"
    return f"{len(changed)} Thai hrefs {how}, e.g. {rel}: {v} -> {nv}", {}


P_WORD_RE = re.compile(r"(<p\b[^>]*>)([^<]*?\s)([^\W\d_]{4,})(?=\s)")


def split_code(d, run, man):
    """Turns a word of a paragraph into a code element with every character in its own span,
    as a highlighter's token spans (Chroma and syntect split differently): the visible text
    must not change."""
    for rel in html_files(d):
        e = man["files"].get(rel)
        if not e or e["type"] != "html":
            continue
        text = read(d, rel)
        body = text.lower().find("<body")
        m = P_WORD_RE.search(text, max(body, 0))
        if m is None or m.start() < body:
            continue
        word = m.group(3)
        spans = "".join(f'<span class="t">{c}</span>' for c in word)
        put(d, rel, text[:m.start(3)] + f"<code>{spans}</code>" + text[m.end(3):])
        return f"{rel}: {word!r} as <code> with {len(word)} spans", {}
    raise SystemExit("split code: no <p> with a word between spaces")


P_TEXT_RE = re.compile(r"(<p\b[^>]*>)([^<]*?)(\b[^\W\d_]{4,}\b)")


def change_text(d, run, man):
    for rel in html_files(d):
        e = man["files"].get(rel)
        if not e or e["type"] != "html":
            continue
        text = read(d, rel)
        body = text.lower().find("<body")
        m = P_TEXT_RE.search(text, max(body, 0))
        if m is None or m.start() < body:
            continue
        new = text[:m.start(3)] + "SELFTESTWORD" + text[m.end(3):]
        put(d, rel, new)
        return f"{rel}: {m.group(3)!r} -> 'SELFTESTWORD'", {(rel, "L3"): "L3 text"}
    raise SystemExit("change text: no <p> with a word")


def bump_dimensions(b):
    """The image with its width one pixel larger (header only), or None."""
    b = bytearray(b)
    if b[:8] == b"\x89PNG\r\n\x1a\n":
        w = struct.unpack(">I", b[16:20])[0]
        b[16:20] = struct.pack(">I", w + 1)
        return bytes(b)
    if b[:2] == b"\xff\xd8":
        i = 2
        while i + 9 < len(b):
            if b[i] != 0xFF:
                i += 1
                continue
            marker = b[i + 1]
            if marker in (0xD8, 0x01) or 0xD0 <= marker <= 0xD7:
                i += 2
                continue
            if marker in (0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE, 0xCF):
                w = struct.unpack(">H", b[i + 7:i + 9])[0]
                b[i + 7:i + 9] = struct.pack(">H", w + 1)
                return bytes(b)
            i += 2 + struct.unpack(">H", b[i + 2:i + 4])[0]
    return None


def change_image(d, run, man):
    cands = sorted(r for r, e in man["files"].items() if e["type"] == "image")
    # A processed or bundle image first (a static one also changes its bytes at L4).
    cands.sort(key=lambda r: (os.path.isfile(os.path.join(run.project or "", "static", r)), r))
    for rel in cands:
        with open(os.path.join(d, rel), "rb") as fh:
            b = bump_dimensions(fh.read())
        if b is None:
            continue
        with open(os.path.join(d, rel), "wb") as fh:
            fh.write(b)
        return f"{rel}: width + 1 in the header", {(mf.norm_path(rel), "L4"): "L4 image"}
    raise SystemExit("change image: no PNG or JPEG")


ITEM_LINK_RE = re.compile(r"(<item>.*?<link>)([^<]*)(</link>)", re.S)


def change_rss_link(d, run, man):
    for rel in sorted(r for r, e in man["files"].items() if e["type"] == "xml"):
        text = read(d, rel)
        m = ITEM_LINK_RE.search(text)
        if m is None:
            continue
        old = m.group(2)
        u = urllib.parse.urlsplit(old)
        new = urllib.parse.urlunsplit((u.scheme, u.netloc, "/selftest-moved" + u.path, u.query, u.fragment))
        put(d, rel, text[:m.start(2)] + new + text[m.end(2):])
        return f"{rel}: first item link {old} -> {new}", {(rel, "L2"): "L2 xml items"}
    raise SystemExit("change RSS link: no feed with an item")


PERTURBATIONS = [
    ("drop a file", drop_file),
    ("add a file", add_file),
    ("change an internal link", change_link),
    ("reorder attributes (ignored)", reorder_attributes),
    ("percent-encode a Thai href (ignored)", thai_href),
    ("split code into token spans (ignored)", split_code),
    ("change visible text", change_text),
    ("change image dimensions", change_image),
    ("change an RSS item link", change_rss_link),
    # Beyond §7.2's eight: link integrity (the span split is T66's).
    ("drop a linked page (link integrity)", drop_linked_page),
]


def listing(d, n=4):
    items = [f"{lv} {k}: {c if isinstance(c, str) else ', '.join(c)}" for (k, lv), c in sorted(d.items(), key=lambda x: (x[0][1], x[0][0]))]
    return "; ".join(items[:n]) + (f"; … {len(items) - n} more" if len(items) > n else "")


def check(got, want):
    """Whether the differences are exactly the expected ones (each with its class)."""
    if set(got) != set(want):
        return False
    return all(want[k] in got[k] for k in want)


# ---------------------------------------------------------------------------------------------
# The ratchet

def ratchet_checks(run, base_dir, text_dir, tmp):
    results = []
    base_res = run.compare(base_dir, base_dir)
    baseline = os.path.join(tmp, "baseline.json")
    _, doc = sd.ratchet(base_res, None, [], run.site)
    sd.write_baseline(baseline, doc)
    res = run.compare(base_dir, text_dir)
    (key, level), = diffs(res).keys()
    r, _ = sd.ratchet(res, sd.read_baseline(baseline), [], run.site)
    results.append(("an unlisted new diff fails", bool(r["unlisted"]) and not r["listed"],
                    f"{len(r['unlisted'])} unlisted: {level} {key}"))
    changes_dir = os.path.join(tmp, "changes")
    os.makedirs(changes_dir)
    with open(os.path.join(changes_dir, "SELFTEST.md"), "w", encoding="utf-8") as fh:
        fh.write(f"# SELFTEST\n\n- {run.site} {level} `{key}` accepted-deviation: the self-test's text change\n")
    changes, errors = sd.load_changes(changes_dir, ["SELFTEST"])
    r, doc = sd.ratchet(res, sd.read_baseline(baseline), changes, run.site)
    ok = not errors and not r["unlisted"] and len(r["listed"]) == 1
    sd.write_baseline(baseline, doc)
    stored = sd.read_baseline(baseline)["files"][key][level]
    ok = ok and isinstance(stored, dict) and stored["class"] == "accepted-deviation" and stored["task"] == "SELFTEST"
    results.append(("a listed diff passes and --update writes it", ok, f"baseline entry {stored}"))
    r, _ = sd.ratchet(res, sd.read_baseline(baseline), [], run.site)
    results.append(("the same diff against the updated baseline passes",
                    not (r["unlisted"] or r["listed"] or r["improved"]), "unchanged"))
    r, _ = sd.ratchet(base_res, sd.read_baseline(baseline), [], run.site)
    results.append(("an unlisted improvement does not fail",
                    not r["unlisted"] and len(r["improved"]) == 1, f"{len(r['improved'])} improved"))
    bad = os.path.join(changes_dir, "BAD.md")
    with open(bad, "w", encoding="utf-8") as fh:
        fh.write(f"- {run.site} L3 `x` fixed: no such class\n- {run.site} L3 `x` bug-fixed:\n")
    _, errors = sd.load_changes(changes_dir, ["BAD"])
    results.append(("a changes entry without one triage class and a reason is an error", len(errors) == 2,
                    f"{len(errors)} errors"))
    return results


# ---------------------------------------------------------------------------------------------

def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--go-out")
    ap.add_argument("--project")
    ap.add_argument("--keep", action="store_true")
    a = ap.parse_args()
    tmp = tempfile.mkdtemp(prefix="ssg-selftest.")
    try:
        if a.go_out:
            src, project, site = a.go_out, a.project, "go-out"
        else:
            src, site = os.path.join(tmp, "testsite"), "testsite (Go output + Thai page + PNG)"
            testsite_output(src)
            # The project directory only gives the extractor the base URL.
            project = os.path.join(tmp, "testsite-project")
            write(os.path.join(project, "config.toml"), 'baseURL = "https://example.org/"\n')
        base = os.path.join(tmp, "base")
        shutil.copytree(src, base)
        run = Run("selftest", project, None)
        print(f"selftest.py: Go output of {site}: {len(mf.walk(base))} files")
        failures = 0
        ident = diffs(run.compare(base, base))
        print(f"  {'PASS' if not ident else 'FAIL'}  identity: {len(ident)} differences")
        failures += bool(ident)
        man = manifest_of(base, run)
        text_dir = None
        for i, (name, fn) in enumerate(PERTURBATIONS, 1):
            d = os.path.join(tmp, f"p{i}")
            shutil.copytree(base, d)
            what, want = fn(d, run, man)
            got = diffs(run.compare(base, d))
            ok = check(got, want)
            failures += not ok
            print(f"  {'PASS' if ok else 'FAIL'}  {i}. {name}: {what}")
            print(f"          expected {listing(want) or 'no difference'}")
            if not ok:
                print(f"          got      {listing(got) or 'no difference'}")
            if fn is change_text:
                text_dir = d
        for name, ok, detail in ratchet_checks(run, base, text_dir, tmp):
            failures += not ok
            print(f"  {'PASS' if ok else 'FAIL'}  ratchet: {name} ({detail})")
        print(f"selftest.py: {'all checks pass' if not failures else f'{failures} FAILED'}")
        return 1 if failures else 0
    finally:
        if a.keep:
            print(f"selftest.py: kept {tmp}")
        else:
            shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())

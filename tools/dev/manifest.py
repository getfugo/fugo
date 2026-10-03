#!/usr/bin/env python3
"""Manifests of a site build (docs/rust-port/REWRITE_PLAN.md §7.2): one extractor for the Go and
the Rust output alike. Python stdlib only.

Usage:
  manifest.py extract <publish-dir> [--project DIR] [--base-url URL]... [--levels L1,L2,L3,L4]
                      [--site LABEL] [--pass NAME] [--full-text] [-o FILE]
  manifest.py summary <manifest>...

`extract` writes the manifest of every file below <publish-dir> (the project directory gives the
base URLs and static/). The golden manifests of the Go build also record its stats file, written
next to its configuration, under a `project:` key (PROJECT_PREFIX); this port writes no such
file, and structdiff.py leaves those entries out.
The base URLs default to the site config (`baseURL` of <project>/config.toml and of its
languages). A file name ending in `.gz` is written gzipped (deterministically). The schema is
documented in testdata/golden/README.md; in short, per file:

  L1  the path and, when it differs, its normalised form (`norm`): `_hu_<hex>` -> `_hu_H`,
      fingerprints `.<16-64 hex>.` -> `.H.`;
  L2  HTML: <title>, <link rel=canonical|alternate>, the set of internal href/src/srcset URLs,
      the alias target of a redirect page; XML: the link/loc/guid texts and href attributes;
      JSON: the key paths and URL leaves; _redirects/_headers/robots.txt: the line set. URLs are
      percent-decoded and NFC-normalised; internal ones are site paths (`/a/b/`, query and
      fragment kept) with the L1 normalisation applied to the path;
  L3  HTML: the visible text (sha256, length, words; the text itself with --full-text; a tag
      boundary is a space, except a `span`'s inside `pre`/`code`, so highlighter token spans do
      not split words) and the heading ids;
  L4  size and sha256 of every file, `static` for files copied from static/; images: width,
      height and format from the file header; CSS/JS: non-empty and referenced by an HTML page.

The minified pass is compared at L1 and L4, the unminified pass at L2 and L3 (§7.2).
"""
import argparse
import gzip
import hashlib
import html.parser
import io
import json
import os
import re
import struct
import sys
import tomllib
import unicodedata
import urllib.parse

SCHEMA = "ssg-manifest/1"
LEVELS = ("L1", "L2", "L3", "L4")
# The prefix of the golden manifests' keys for files of the project directory (the Go build's
# stats file); extract records none.
PROJECT_PREFIX = "project:"

HU_RE = re.compile(r"_hu_[0-9a-f]+")
FINGERPRINT_RE = re.compile(r"\.[0-9a-f]{16,64}(?=\.)")

IMAGE_EXT = {".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".tif", ".tiff", ".ico"}
LINES_NAMES = {"_redirects", "_headers", "robots.txt"}
TYPOGRAPHIC = str.maketrans({"‘": "'", "’": "'", "‚": "'", "“": '"',
                             "”": '"', "„": '"', "«": '"', "»": '"',
                             "–": "-", "—": "-", " ": " "})


def norm_path(p):
    """L1 normalisation of an output path."""
    return FINGERPRINT_RE.sub(".H", HU_RE.sub("_hu_H", p))


def kind_of(rel):
    name = rel.rsplit("/", 1)[-1]
    ext = os.path.splitext(name)[1].lower()
    if name in LINES_NAMES:
        return "lines"
    if ext in (".html", ".htm"):
        return "html"
    if ext == ".xml":
        return "xml"
    if ext in (".json", ".webmanifest"):
        return "json"
    if ext == ".css":
        return "css"
    if ext in (".js", ".mjs"):
        return "js"
    if ext in IMAGE_EXT:
        return "image"
    return "other"


class Urls:
    """Normalises URLs: internal ones (below a base URL, or root-relative, or relative to the
    page) become percent-decoded, NFC-normalised site paths with any fragment; external ones
    are percent-decoded and NFC-normalised."""

    def __init__(self, bases):
        self.bases = []
        for b in bases:
            u = urllib.parse.urlsplit(b)
            path = u.path if u.path.endswith("/") else u.path + "/"
            self.bases.append((u.netloc.lower(), path))

    @staticmethod
    def _clean(s):
        return unicodedata.normalize("NFC", urllib.parse.unquote(s))

    def internal(self, url, page="/"):
        """The site path of an internal URL, or None."""
        url = url.strip()
        if not url or url.startswith(("mailto:", "tel:", "javascript:", "data:")):
            return None
        u = urllib.parse.urlsplit(url)
        if u.scheme or u.netloc:
            host = u.netloc.lower()
            for netloc, bpath in self.bases:
                if host == netloc and (u.path + "/").startswith(bpath):
                    rest = u.path[len(bpath):] if len(u.path) >= len(bpath) else ""
                    path = "/" + rest
                    break
            else:
                return None
        elif u.path.startswith("/"):
            path = u.path
            for _, bpath in self.bases:
                if bpath != "/" and (path + "/").startswith(bpath):
                    path = "/" + path[len(bpath):]
                    break
        else:
            path = urllib.parse.urljoin(page, u.path) if u.path else page
        out = norm_path(self._clean(path))
        if u.query:
            out += "?" + self._clean(u.query)
        if u.fragment:
            out += "#" + self._clean(u.fragment)
        return out

    def any(self, url, page="/"):
        i = self.internal(url, page)
        return i if i is not None else self._clean(url.strip())


def page_url(rel):
    """The site path a publish-dir file is served at (`a/index.html` -> `/a/`)."""
    if rel == "index.html":
        return "/"
    if rel.endswith("/index.html"):
        return "/" + rel[:-len("index.html")]
    return "/" + rel


class HtmlScan(html.parser.HTMLParser):
    """One pass over an HTML document: title, rel links, URL attributes, alias target, visible
    text and heading ids."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.title = []
        self.rel_links = []
        self.urls = []
        self.refresh = None
        self.text = []
        self.ids = []
        self._skip = 0
        self._title = None
        self._code = 0

    def _boundary(self, tag):
        # A tag boundary separates words, except a `span` in code: highlighters wrap tokens in
        # spans that touch (their span structure is an allowed difference, §7.3).
        if not (tag == "span" and self._code):
            self.text.append(" ")

    def handle_starttag(self, tag, attrs):
        a = {k: (v or "") for k, v in attrs}
        self._boundary(tag)
        if tag in ("pre", "code"):
            self._code += 1
        if tag in ("script", "style"):
            self._skip += 1
        if tag == "title":
            self._title = []
        if tag == "link" and "href" in a:
            rels = a.get("rel", "").lower().split()
            for rel in ("canonical", "alternate"):
                if rel in rels:
                    self.rel_links.append((rel, a["href"], a.get("hreflang", ""), a.get("type", "")))
        for k in ("href", "src"):
            if k in a:
                self.urls.append(a[k])
        if "srcset" in a:
            for c in a["srcset"].split(","):
                parts = c.split()
                if parts:
                    self.urls.append(parts[0])
        if tag == "meta" and a.get("http-equiv", "").lower() == "refresh":
            m = re.search(r"url=(.*)$", a.get("content", ""), re.I)
            if m:
                self.refresh = m.group(1).strip().strip("'\"")
        if len(tag) == 2 and tag[0] == "h" and tag[1] in "123456" and "id" in a:
            self.ids.append(a["id"])

    def handle_startendtag(self, tag, attrs):
        self.handle_starttag(tag, attrs)
        if tag in ("script", "style"):
            self._skip -= 1
        if tag in ("pre", "code"):
            self._code -= 1

    def handle_endtag(self, tag):
        self._boundary(tag)
        if tag in ("pre", "code") and self._code:
            self._code -= 1
        if tag in ("script", "style") and self._skip:
            self._skip -= 1
        if tag == "title" and self._title is not None:
            self.title.append(" ".join("".join(self._title).split()))
            self._title = None

    def handle_data(self, data):
        if self._title is not None:
            self._title.append(data)
        if not self._skip:
            self.text.append(data)


def visible_text(scan):
    t = "".join(scan.text).translate(TYPOGRAPHIC).replace("…", "...")
    return " ".join(t.split())


class XmlScan(html.parser.HTMLParser):
    """The link/loc/guid element texts and href attributes of a feed or sitemap, in order.
    (html.parser rather than xml.etree: it takes any namespace prefix and never fails.)"""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.items = []
        self._el = None
        self._buf = []

    def handle_starttag(self, tag, attrs):
        for k, v in attrs:
            if k == "href" and v:
                self.items.append((tag + " href", v))
        if tag in ("link", "loc", "guid"):
            self._el, self._buf = tag, []

    def handle_endtag(self, tag):
        if self._el == tag:
            self.items.append((tag, "".join(self._buf).strip()))
            self._el = None

    def handle_data(self, data):
        if self._el:
            self._buf.append(data)


def image_info(b):
    """(width, height, format) from an image header, or None."""
    if b[:8] == b"\x89PNG\r\n\x1a\n" and len(b) >= 24:
        w, h = struct.unpack(">II", b[16:24])
        return [w, h, "png"]
    if b[:6] in (b"GIF87a", b"GIF89a") and len(b) >= 10:
        w, h = struct.unpack("<HH", b[6:10])
        return [w, h, "gif"]
    if b[:4] == b"RIFF" and b[8:12] == b"WEBP" and len(b) >= 30:
        chunk = b[12:16]
        if chunk == b"VP8 ":
            w, h = struct.unpack("<HH", b[26:30])
            return [w & 0x3FFF, h & 0x3FFF, "webp"]
        if chunk == b"VP8L":
            bits = int.from_bytes(b[21:25], "little")
            return [(bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1, "webp"]
        if chunk == b"VP8X":
            return [int.from_bytes(b[24:27], "little") + 1, int.from_bytes(b[27:30], "little") + 1, "webp"]
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
            seg = struct.unpack(">H", b[i + 2:i + 4])[0]
            if marker in (0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE, 0xCF):
                h, w = struct.unpack(">HH", b[i + 5:i + 9])
                return [w, h, "jpeg"]
            i += 2 + seg
    if b[:2] == b"BM" and len(b) >= 26:
        w, h = struct.unpack("<ii", b[18:26])
        return [w, abs(h), "bmp"]
    if b[:4] == b"\x00\x00\x01\x00" and len(b) >= 8:
        return [b[6] or 256, b[7] or 256, "ico"]
    return None


def json_structure(v, at, keys, urls, urls_obj, page):
    if isinstance(v, dict):
        for k in sorted(v):
            json_structure(v[k], f"{at}.{k}", keys, urls, urls_obj, page)
        if not v:
            keys.add(at + "{}")
    elif isinstance(v, list):
        for i, x in enumerate(v):
            json_structure(x, f"{at}[{i}]", keys, urls, urls_obj, page)
        if not v:
            keys.add(at + "[]")
    else:
        keys.add(re.sub(r"\[\d+\]", "[]", at))
        if isinstance(v, str) and (v.startswith("/") or "://" in v):
            urls.append(f"{at} {urls_obj.any(v, page)}")


def site_config(project):
    """The base URLs of a site directory's config.toml (and of its languages)."""
    fn = os.path.join(project, "config.toml")
    if not os.path.exists(fn):
        return []
    with open(fn, "rb") as fh:
        cfg = tomllib.load(fh)
    lower = {k.lower(): v for k, v in cfg.items()}
    bases = [lower["baseurl"]] if "baseurl" in lower else []
    for lang in (lower.get("languages") or {}).values():
        if isinstance(lang, dict):
            for k, v in lang.items():
                if k.lower() == "baseurl":
                    bases.append(v)
    return bases


def walk(root):
    out = []
    for d, dirs, files in os.walk(root):
        dirs.sort()
        for f in files:
            p = os.path.join(d, f)
            out.append(os.path.relpath(p, root).replace(os.sep, "/"))
    return sorted(out)


def extract(publish, project, bases, levels, full_text):
    urls = Urls(bases)
    sources = {rel: os.path.join(publish, rel) for rel in walk(publish)}
    static_dir = os.path.join(project, "static") if project else None
    files = {}
    referenced = set()
    for rel in sorted(sources):
        with open(sources[rel], "rb") as fh:
            b = fh.read()
        kind = kind_of(rel)
        e = {"type": kind}
        n = norm_path(rel)
        if n != rel:
            e["norm"] = n
        page = page_url(rel)
        if kind == "html":
            scan = HtmlScan()
            scan.feed(b.decode("utf-8", "replace"))
            scan.close()
            links = sorted({x for x in (urls.internal(u, page) for u in scan.urls) if x is not None})
            referenced.update(x.split("#")[0].split("?")[0] for x in links)
            if scan.refresh is not None:
                e["type"] = "alias"
                if "L2" in levels:
                    e["L2"] = {"alias": urls.any(scan.refresh, page)}
            else:
                if "L2" in levels:
                    e["L2"] = {"title": scan.title,
                               "rel": [[r, urls.any(h, page), hl, t] for r, h, hl, t in scan.rel_links],
                               "links": links}
                if "L3" in levels:
                    text = visible_text(scan)
                    t = {"sha256": hashlib.sha256(text.encode()).hexdigest(), "len": len(text),
                         "words": len(text.split())}
                    if full_text:
                        t["t"] = text
                    e["L3"] = {"text": t, "ids": scan.ids}
        elif kind == "xml" and "L2" in levels:
            scan = XmlScan()
            scan.feed(b.decode("utf-8", "replace"))
            scan.close()
            e["L2"] = {"items": [f"{k} {urls.any(v, page)}" for k, v in scan.items]}
        elif kind == "json" and ("L2" in levels):
            try:
                v = json.loads(b.decode("utf-8"))
            except (UnicodeDecodeError, ValueError) as err:
                e["L2"] = {"error": f"not JSON: {err.__class__.__name__}"}
            else:
                keys, found = set(), []
                json_structure(v, "", keys, found, urls, page)
                e["L2"] = {"keys": sorted(keys), "urls": found}
        elif kind == "lines" and "L2" in levels:
            text = b.decode("utf-8", "replace")
            e["L2"] = {"lines": sorted({" ".join(line.split()) for line in text.splitlines() if line.strip()})}
        if "L4" in levels:
            e["size"] = len(b)
            e["sha256"] = hashlib.sha256(b).hexdigest()
            if static_dir and os.path.isfile(os.path.join(static_dir, rel)):
                e["static"] = True
            if kind == "image":
                info = image_info(b)
                e["L4"] = {"image": info} if info else {"image": None}
            elif kind in ("css", "js"):
                e["L4"] = {"nonEmpty": bool(b.strip())}
        files[rel] = e
    if "L4" in levels:
        for rel, e in files.items():
            if e["type"] in ("css", "js"):
                e["L4"]["referenced"] = "/" + norm_path(rel) in referenced
    return files


def dumps(doc):
    """Sorted keys, one file per line."""
    head = {k: v for k, v in doc.items() if k != "files"}
    lines = [f"{json.dumps(k)}: {json.dumps(v, ensure_ascii=False, sort_keys=True)}" for k, v in sorted(head.items())]
    body = ",\n".join(f"{json.dumps(k, ensure_ascii=False)}: {json.dumps(v, ensure_ascii=False, sort_keys=True)}"
                      for k, v in sorted(doc["files"].items()))
    lines.append('"files": {\n' + body + "\n}")
    return "{\n" + ",\n".join(sorted(lines)) + "\n}\n"


def write_out(path, text):
    data = text.encode("utf-8")
    if path.endswith(".gz"):
        buf = io.BytesIO()
        with gzip.GzipFile(filename="", mode="wb", fileobj=buf, compresslevel=9, mtime=0) as gz:
            gz.write(data)
        data = buf.getvalue()
    with open(path, "wb") as fh:
        fh.write(data)


def read_manifest(path):
    opener = gzip.open if path.endswith(".gz") else open
    with opener(path, "rt", encoding="utf-8") as fh:
        return json.load(fh)


def summary(path):
    doc = read_manifest(path)
    if doc.get("schema", "").startswith("ssg-structure/"):
        written = sum(1 for r in doc["records"] if r.get("written", True))
        return (f"{os.path.basename(path)}: {len(doc['records'])} (page, format) records ({written} "
                f"written), {len(doc['aliases'])} aliases, {len(doc['pagerAliases'])} page/1 aliases, "
                f"{len(doc['resources'])} resources, {len(doc['pages'])} pages, "
                f"{len(doc['layouts'])} layout entries")
    files = doc["files"]
    kinds = {}
    for e in files.values():
        kinds[e["type"]] = kinds.get(e["type"], 0) + 1
    public = sum(1 for k in files if not k.startswith(PROJECT_PREFIX))
    return (f"{os.path.basename(path)}: {doc['count']} files "
            f"({public} in publishDir + {doc['count'] - public} in the project dir); "
            f"levels {','.join(doc['levels'])}; " + ", ".join(f"{k} {v}" for k, v in sorted(kinds.items())))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    ex = sub.add_parser("extract")
    ex.add_argument("publish")
    ex.add_argument("--project")
    ex.add_argument("--base-url", action="append", default=[])
    ex.add_argument("--levels", default=",".join(LEVELS))
    ex.add_argument("--site", default="")
    ex.add_argument("--pass", dest="pass_", default="")
    ex.add_argument("--full-text", action="store_true")
    ex.add_argument("-o", "--out")
    su = sub.add_parser("summary")
    su.add_argument("manifests", nargs="+")
    a = ap.parse_args()
    if a.cmd == "summary":
        for m in a.manifests:
            print(summary(m))
        return
    levels = [lv for lv in a.levels.split(",") if lv]
    bad = [lv for lv in levels if lv not in LEVELS]
    if bad:
        sys.exit(f"unknown levels {bad}")
    bases = a.base_url or (site_config(a.project) if a.project else [])
    files = extract(a.publish, a.project, bases, set(levels), a.full_text)
    doc = {"schema": SCHEMA, "site": a.site, "pass": a.pass_, "levels": levels,
           "baseURLs": bases, "count": len(files), "files": files}
    text = dumps(doc)
    if a.out:
        write_out(a.out, text)
    else:
        sys.stdout.write(text)


if __name__ == "__main__":
    main()

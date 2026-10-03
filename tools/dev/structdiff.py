#!/usr/bin/env python3
"""Structural comparison of two builds of one site (docs/rust-port/REWRITE_PLAN.md §7.2): a
reference (the Go build's committed golden data) against a candidate (the Rust build), level by
level and per file, plus the structure oracle; then the ratchet
(testdata/baselines/<site>.json, tools/dev/changes/<task>.md). Python stdlib only.

Usage:
  structdiff.py compare --site LABEL
        --ref-min M --ref-unmin M [--ref-structure S] [--ref-project DIR] [--ref-name NAME]
        --cand-min M --cand-unmin M [--cand-structure S] [--cand-project DIR] [--cand-name NAME]
        [--collision-dir DIR]... [--json FILE] [--report FILE] [--show N]
        [--baseline FILE [--task ID]... [--changes DIR] [--update] [--report-only]]
  structdiff.py changes [--changes DIR]      # validate every changes/<task>.md

M is a manifest (tools/dev/manifest.py; `.json` or `.json.gz`) or a publish directory,
which is extracted with manifest.py (its project directory, for static/ and the base URLs, is
--ref-project/--cand-project); the `project:` entries of a manifest file (the Go build's stats
file, written next to its configuration) are left out, as this port writes no such file. The minified pass gives L1 and L4, the unminified
pass L1, L2 and L3; S is the structure dump (testdata/golden/README.md). Every comparison
reads both sides through the same extractor, with the §7.2 normalisations:

  L1  the multisets of output paths: `_hu_<hex>` -> `_hu_H`, fingerprints `.<16-64 hex>.` -> `.H.`,
      and the known collision directories collapsed (the directory of every target that two
      (page, format) records of either structure dump claim: whose term wins it is an allowed
      difference, so only the directory's presence is compared, `dir/**`). Both passes.
  L2  HTML: <title>, <link rel=canonical|alternate>, the set of internal href/src/srcset URLs;
      alias targets; RSS/sitemap link/loc/guid lists; JSON key paths and URL leaves; the line
      sets of _redirects/_headers/robots.txt. URLs are percent-decoded and NFC-normalised by
      the extractor. Link integrity: an internal link (or alias target) of the candidate that
      resolves to none of its files is a difference unless the reference's is dangling too.
  L3  HTML: the visible text (entities decoded, typographic characters mapped to ASCII,
      whitespace collapsed; compared by hash) and the heading-ID list.
  L4  images: (width, height, format); files of static/: bytes (sha256); CSS/JS: non-empty and
      referenced (as the reference's are). From the minified pass; when neither side has one and
      both unminified manifests were extracted with L4 (a site published unminified), from those.
  S   per (lang, page, kind, format): target, relPermalink, permalink, template and baseof (the
      v0.146 names; an embedded template is marked as such), written, pagers; per alias file
      (front matter and the language redirect) and per page/1 alias: kind and permalink; per
      (lang, page, resource name): relPermalink, target(s), publish; per page: its output
      formats.

A7 (tracked metric): the share of the reference's HTML pages whose visible text the candidate
has exactly; the worst 20 pages are ranked by a per-page similarity (the Dice coefficient of the
two word multisets when both manifests carry the text, `manifest.py --full-text`; else the
length ratio, marked `len`).

Every (file or structure key, level) gets a status (`ok`, `diff`, `missing`, `extra`) and, when
not ok, a diff fingerprint (12 hex digits of the hash of both sides' normalised values) and the
difference classes. The report (--report, and stdout) has the per-level numbers, the top
difference classes, the worst pages and the differences; --json writes all of it.

Ratchet (with --baseline): each (key, level) is compared with the baseline's status and
fingerprint. A change must be listed in the changes file of the running task
(tools/dev/changes/<task>.md, --task; format in its README.md); an unlisted new or changed
difference fails the run (exit 1). --update writes the baseline with the listed changes applied
(plus new keys that are `ok`, and keys gone from both sides); unlisted changes keep their old
baseline entry. --report-only always exits 0. Without --baseline the run fails on any
difference.
"""
import argparse
import collections
import difflib
import fnmatch
import gzip
import hashlib
import io
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.dont_write_bytecode = True  # the sibling modules below: no __pycache__ in the tree
import manifest as mf  # noqa: E402


SCHEMA = "ssg-structdiff/1"
BASELINE_SCHEMA = "ssg-baseline/1"
LEVELS = ("L1", "L2", "L3", "L4", "S")
CLASSES = ("engine-difference", "bug-fixed", "accepted-deviation")
CHANGES_DIR = os.path.join(HERE, "changes")
GZIP_OVER = 256 * 1024
WORST = 20


# ---------------------------------------------------------------------------------------------
# Inputs

def read_json(path):
    opener = gzip.open if path.endswith(".gz") else open
    with opener(path, "rt", encoding="utf-8") as fh:
        return json.load(fh)


def existing(path):
    """`path`, or `path.gz` when only that exists."""
    if path and not os.path.exists(path) and os.path.exists(path + ".gz"):
        return path + ".gz"
    return path


def load_manifest(src, project, site, pass_, full_text=True):
    """A manifest file, or the manifest of a publish directory (extracted like the golden ones)."""
    if src is None:
        return None
    if os.path.isdir(src):
        levels = ["L1", "L4"] if pass_ == "minified" else ["L1", "L2", "L3"]
        bases = mf.site_config(project) if project else []
        files = mf.extract(src, project, bases, set(levels), full_text)
        return {"schema": mf.SCHEMA, "site": site, "pass": pass_, "levels": levels,
                "baseURLs": bases, "count": len(files), "files": files}
    doc = read_json(existing(src))
    files = doc.get("files")
    if isinstance(files, dict):
        for k in [k for k in files if k.startswith(mf.PROJECT_PREFIX)]:
            del files[k]
            doc["count"] -= 1
    return doc


class Side:
    def __init__(self, name, minified, unminified, structure):
        self.name = name
        self.min = minified
        self.unmin = unminified
        self.structure = structure


# ---------------------------------------------------------------------------------------------
# Results

def fingerprint(*parts):
    data = json.dumps(parts, sort_keys=True, ensure_ascii=False, default=list)
    return hashlib.sha256(data.encode("utf-8")).hexdigest()[:12]


def entry(status, classes=(), detail="", fp_parts=None):
    e = {"status": status}
    if status != "ok":
        e["fp"] = fingerprint(*(fp_parts if fp_parts is not None else [status, detail]))
        e["classes"] = sorted(set(classes))
        if detail:
            e["detail"] = detail
    return e


def short(values, n=6):
    values = list(values)
    s = ", ".join(str(v) for v in values[:n])
    return "{" + s + (f", … {len(values) - n} more" if len(values) > n else "") + "}"


# ---------------------------------------------------------------------------------------------
# L1

def collision_dirs(structure):
    """The directories of targets that more than one (page, format) record claims."""
    if not structure:
        return set()
    count = collections.Counter(r.get("target", "") for r in structure.get("records", []))
    out = set()
    for target, n in count.items():
        d = target.strip("/").rpartition("/")[0]
        if n > 1 and d:
            out.add(d + "/")
    return out


def l1_key(rel, cdirs):
    n = mf.norm_path(rel)
    for d in cdirs:
        if n.startswith(d):
            return d + "**"
    return n


def l1_counts(manifest, cdirs):
    """The number of files per L1 key (a collapsed collision directory counts all its files)."""
    return collections.Counter(l1_key(rel, cdirs) for rel in manifest["files"])


# ---------------------------------------------------------------------------------------------
# Per-file payloads

def by_norm(manifest):
    g = collections.defaultdict(list)
    for rel in sorted(manifest["files"]):
        g[mf.norm_path(rel)].append(manifest["files"][rel])
    return g


def l2_payload(e):
    l2 = e.get("L2")
    if l2 is None:
        return None
    return {"type": e["type"], **l2}


def l3_payload(e):
    l3 = e.get("L3")
    if l3 is None:
        return None
    if e["type"] == "html":
        return {"type": "html", "text": l3["text"]["sha256"], "ids": l3["ids"]}
    return {"type": e["type"], **l3}


def l4_payload(e, static):
    out = {}
    t = e["type"]
    l4 = e.get("L4") or {}
    if t == "image":
        out["image"] = l4.get("image")
    elif t in ("css", "js"):
        out["nonEmpty"] = l4.get("nonEmpty")
        out["referenced"] = l4.get("referenced")
    if static:
        out["sha256"] = e.get("sha256")
    return out or None


def canon(v):
    return json.dumps(v, sort_keys=True, ensure_ascii=False)


def l2_diff(r, c):
    """Classes and a readable detail of two L2 payloads of one file."""
    if r.get("type") != c.get("type"):
        return ["L2 type"], f"type {r.get('type')} -> {c.get('type')}"
    t, cls, parts = r["type"], [], []
    if t == "html":
        if r.get("title") != c.get("title"):
            cls.append("L2 html title")
            parts.append(f"title {r.get('title')} -> {c.get('title')}")
        if r.get("rel") != c.get("rel"):
            cls.append("L2 html rel")
            rs = {tuple(x) for x in r.get("rel", [])}
            cs = {tuple(x) for x in c.get("rel", [])}
            parts.append(f"rel -{short(sorted(rs - cs))} +{short(sorted(cs - rs))}"
                         if rs != cs else "rel order")
        if r.get("links") != c.get("links"):
            cls.append("L2 html links")
            rs, cs = set(r.get("links", [])), set(c.get("links", []))
            parts.append(f"links -{short(sorted(rs - cs))} +{short(sorted(cs - rs))}")
    elif t == "alias":
        cls.append("L2 alias target")
        parts.append(f"alias {r.get('alias')} -> {c.get('alias')}")
    elif t == "xml":
        cls.append("L2 xml items")
        ri, ci = r.get("items", []), c.get("items", [])
        if sorted(ri) == sorted(ci):
            parts.append("items in another order")
        else:
            rc, cc = collections.Counter(ri), collections.Counter(ci)
            parts.append(f"items -{short(sorted((rc - cc).elements()))} +{short(sorted((cc - rc).elements()))}")
    elif t == "json":
        if r.get("error") != c.get("error"):
            cls.append("L2 json error")
            parts.append(f"error {r.get('error')} -> {c.get('error')}")
        if r.get("keys") != c.get("keys"):
            cls.append("L2 json keys")
            rs, cs = set(r.get("keys", [])), set(c.get("keys", []))
            parts.append(f"keys -{short(sorted(rs - cs))} +{short(sorted(cs - rs))}")
        if r.get("urls") != c.get("urls"):
            cls.append("L2 json urls")
            rc, cc = collections.Counter(r.get("urls", [])), collections.Counter(c.get("urls", []))
            parts.append(f"urls -{short(sorted((rc - cc).elements()))} +{short(sorted((cc - rc).elements()))}")
    elif t == "lines":
        cls.append("L2 lines")
        rs, cs = set(r.get("lines", [])), set(c.get("lines", []))
        parts.append(f"lines -{short(sorted(rs - cs))} +{short(sorted(cs - rs))}")
    else:
        cls.append(f"L2 {t}")
    return cls, "; ".join(parts)


def l3_diff(r, c, rtext, ctext):
    if r.get("type") != c.get("type"):
        return ["L3 type"], f"type {r.get('type')} -> {c.get('type')}"
    cls, parts = [], []
    if r["type"] == "html":
        if r["text"] != c["text"]:
            cls.append("L3 text")
            parts.append(f"text {rtext.get('words', '?')} -> {ctext.get('words', '?')} words, "
                         f"{rtext.get('len', '?')} -> {ctext.get('len', '?')} chars")
        if r["ids"] != c["ids"]:
            cls.append("L3 heading ids")
            rs, cs = set(r["ids"]), set(c["ids"])
            parts.append(f"ids -{short(sorted(rs - cs))} +{short(sorted(cs - rs))}"
                         if rs != cs else "ids in another order")
    return cls, "; ".join(parts)


def l4_diff(r, c):
    cls, parts = [], []
    if r.get("image") != c.get("image"):
        cls.append("L4 image")
        parts.append(f"image {r.get('image')} -> {c.get('image')}")
    if r.get("sha256") != c.get("sha256"):
        cls.append("L4 static bytes")
        parts.append("static file bytes differ")
    for k in ("nonEmpty", "referenced"):
        if r.get(k) != c.get(k):
            cls.append(f"L4 css/js {k}")
            parts.append(f"{k} {r.get(k)} -> {c.get(k)}")
    return cls, "; ".join(parts)


def group_detail(rp, cp):
    """The difference of two multisets of payloads (files that share a normalised path)."""
    rc = collections.Counter(canon(x) for x in rp)
    cc = collections.Counter(canon(x) for x in cp)
    return (f"{len(rp)} vs {len(cp)} files: -{short(sorted((rc - cc).elements()), 3)} "
            f"+{short(sorted((cc - rc).elements()), 3)}")


def resolves(link, files):
    """Whether an internal site path resolves to one of `files` (normalised publish paths)."""
    p = link.split("#", 1)[0].split("?", 1)[0]
    if not p.startswith("/"):
        return True
    rel = p[1:]
    if rel == "":
        return "index.html" in files
    if rel.endswith("/"):
        return rel + "index.html" in files
    return rel in files or rel + "/index.html" in files


def dangling(e, files):
    l2 = e.get("L2") or {}
    links = list(l2.get("links", []))
    if e["type"] == "alias" and str(l2.get("alias", "")).startswith("/"):
        links.append(l2["alias"])
    return {x for x in links if x.startswith("/") and not resolves(x, files)}


# ---------------------------------------------------------------------------------------------
# A7

def words_similarity(a, b):
    ca, cb = collections.Counter(a.split()), collections.Counter(b.split())
    total = sum(ca.values()) + sum(cb.values())
    return 2 * sum((ca & cb).values()) / total if total else 1.0


def text_hunks(a, b, width=8, limit=20):
    """The differing stretches of two texts, word by word: `[-removed-] [+added+]`, each side cut
    to `width` words. The common prefix and suffix are trimmed first, so a page with one
    difference costs no alignment."""
    wa, wb = a.split(), b.split()
    i = 0
    while i < min(len(wa), len(wb)) and wa[i] == wb[i]:
        i += 1
    ja, jb = len(wa), len(wb)
    while ja > i and jb > i and wa[ja - 1] == wb[jb - 1]:
        ja -= 1
        jb -= 1
    cut = lambda ws: " ".join(ws[:width]) + (" …" if len(ws) > width else "")  # noqa: E731
    ma, mb = wa[i:ja], wb[i:jb]
    out = []
    for tag, i1, i2, j1, j2 in difflib.SequenceMatcher(None, ma, mb).get_opcodes():
        if tag != "equal":
            out.append(f"[-{cut(ma[i1:i2])}-] [+{cut(mb[j1:j2])}+]")
            if len(out) == limit:
                break
    return out


def a7(ref_un, cand_un):
    """A7, the worst pages, and the text hunks shared by most pages."""
    rg, cg = by_norm(ref_un), by_norm(cand_un)
    pages = []
    hunks = collections.defaultdict(list)
    for n, items in sorted(rg.items()):
        e = items[0]
        if e["type"] != "html" or "L3" not in e:
            continue
        rt = e["L3"]["text"]
        cands = [x for x in cg.get(n, []) if x["type"] == "html" and "L3" in x]
        if not cands:
            pages.append({"file": n, "ratio": 0.0, "method": "missing"})
            continue
        ct = cands[0]["L3"]["text"]
        if rt["sha256"] == ct["sha256"]:
            pages.append({"file": n, "ratio": 1.0, "method": "equal"})
        elif "t" in rt and "t" in ct:
            h = text_hunks(rt["t"], ct["t"])
            for x in dict.fromkeys(h):
                hunks[x].append(n)
            pages.append({"file": n, "ratio": round(words_similarity(rt["t"], ct["t"]), 4),
                          "method": "words", "hunks": h[:3], "hunkCount": len(h)})
        else:
            la, lb = rt.get("len", 0), ct.get("len", 0)
            ratio = min(la, lb) / max(la, lb) if max(la, lb) else 1.0
            pages.append({"file": n, "ratio": round(min(ratio, 0.9999), 4), "method": "len"})
    equal = sum(1 for p in pages if p["method"] == "equal")
    worst = sorted((p for p in pages if p["method"] != "equal"), key=lambda p: (p["ratio"], p["file"]))
    top = sorted(hunks.items(), key=lambda x: (-len(x[1]), x[0]))[:WORST]
    return {"pages": len(pages), "equal": equal,
            "ratio": round(equal / len(pages), 4) if pages else 1.0,
            "meanSimilarity": round(sum(p["ratio"] for p in pages) / len(pages), 4) if pages else 1.0,
            "worst": worst[:WORST],
            "byFile": {p["file"]: p for p in pages if p.get("hunks")},
            "topHunks": [{"hunk": h, "pages": len(fs), "examples": fs[:3]} for h, fs in top]}


# ---------------------------------------------------------------------------------------------
# S: the structure oracle

def template_id(name, file):
    if not name:
        return ""
    return name + (" (embedded)" if (file or "").startswith("_embedded/") else "")


def structure_items(doc):
    """Every compared structure fact of a dump, by key."""
    out = {}
    for r in doc.get("records", []):
        k = f"record {r.get('lang', '')} {r.get('path', '')} {r.get('kind', '')} {r.get('format', '')}"
        out[k] = {
            "target": r.get("target", ""),
            "relPermalink": r.get("relPermalink", ""),
            "permalink": r.get("permalink", ""),
            "template": template_id(r.get("template", ""), r.get("templateFile", r.get("template", ""))),
            "baseof": template_id(r.get("baseof", ""), r.get("baseofFile", r.get("baseof", ""))),
            "written": r.get("written", True),
            "pagers": r.get("pagers", 0),
        }
    for name, section in (("alias", "aliases"), ("pager", "pagerAliases")):
        for a in doc.get(section, []):
            k = f"{name} {a.get('from', '')} {a.get('lang', '')} {a.get('path', '')} {a.get('format', '')}"
            v = {"permalink": a.get("permalink", "")}
            if name == "alias":
                v["kind"] = a.get("kind", "")
            out[k] = v
    for r in doc.get("resources", []):
        k = f"resource {r.get('lang', '')} {r.get('path', '')} {r.get('name', '')}"
        out[k] = {"relPermalink": r.get("relPermalink", ""),
                  "targets": r.get("targets") or ([r["target"]] if r.get("target") else []),
                  "publish": r.get("publish", True)}
    for p in doc.get("pages", []):
        out[f"page {p.get('lang', '')} {p.get('path', '')} {p.get('kind', '')}"] = {
            "outputs": p.get("outputs") or []}
    return out


def compare_structure(ref, cand):
    ri, ci = structure_items(ref), structure_items(cand)
    out = {}
    for k in sorted(set(ri) | set(ci)):
        what = k.split(" ", 1)[0]
        if k not in ci:
            out[k] = entry("missing", [f"S {what} missing"], "only in the reference",
                           ["missing", ri[k]])
        elif k not in ri:
            out[k] = entry("extra", [f"S {what} extra"], "only in the candidate", ["extra", ci[k]])
        elif ri[k] != ci[k]:
            fields = [f for f in sorted(set(ri[k]) | set(ci[k])) if ri[k].get(f) != ci[k].get(f)]
            detail = "; ".join(f"{f} {ri[k].get(f)!r} -> {ci[k].get(f)!r}" for f in fields)
            out[k] = entry("diff", [f"S {what} {f}" for f in fields], detail, ["diff", ri[k], ci[k]])
        else:
            out[k] = entry("ok")
    return out


# ---------------------------------------------------------------------------------------------
# The comparison

def compare(site, ref, cand, extra_collision_dirs=()):
    files = collections.defaultdict(dict)
    notes = []
    cdirs = sorted(collision_dirs(ref.structure) | collision_dirs(cand.structure)
                   | {d.strip("/") + "/" for d in extra_collision_dirs})
    passes = {}

    # L1, both passes; a key's status is the worse of the two.
    for pass_, rm, cm in (("minified", ref.min, cand.min), ("unminified", ref.unmin, cand.unmin)):
        if rm is None or cm is None:
            notes.append(f"L1: no {pass_} pass on {'both sides' if rm is None and cm is None else 'one side'}")
            continue
        rc, cc = l1_counts(rm, cdirs), l1_counts(cm, cdirs)
        ok = missing = extra = matched = 0
        for k in sorted(set(rc) | set(cc)):
            a, b = rc.get(k, 0), cc.get(k, 0)
            if k.endswith("/**") and a and b:
                # A collision directory: only its presence is compared.
                a = b = min(a, b)
                matched += rc[k]
            else:
                matched += min(a, b)
            if a == b:
                e = entry("ok")
                ok += 1
            elif a > b:
                e = entry("missing", ["L1 missing"], f"{pass_}: reference {a}, candidate {b}",
                          ["missing", a, b])
                missing += 1
            else:
                e = entry("extra", ["L1 extra"], f"{pass_}: reference {a}, candidate {b}",
                          ["extra", a, b])
                extra += 1
            prev = files[k].get("L1")
            if prev is None or (prev["status"] == "ok" and e["status"] != "ok"):
                files[k]["L1"] = e
        passes[pass_] = {"matched": matched, "refFiles": len(rm["files"]),
                         "candFiles": len(cm["files"]), "ok": ok, "missing": missing,
                         "extra": extra}

    # L2, L3: the unminified pass.
    if ref.unmin is not None and cand.unmin is not None:
        rg, cg = by_norm(ref.unmin), by_norm(cand.unmin)
        rfiles = {mf.norm_path(x) for x in ref.unmin["files"]}
        cfiles = {mf.norm_path(x) for x in cand.unmin["files"]}
        for n in sorted(set(rg) & set(cg)):
            ritems, citems = rg[n], cg[n]
            # L2
            rp = sorted((l2_payload(e) for e in ritems if l2_payload(e) is not None), key=canon)
            cp = sorted((l2_payload(e) for e in citems if l2_payload(e) is not None), key=canon)
            new_dangling = set()
            for e in citems:
                new_dangling |= dangling(e, cfiles)
            for e in ritems:
                new_dangling -= dangling(e, rfiles)
            if rp or cp:
                cls, detail = [], []
                if [canon(x) for x in rp] != [canon(x) for x in cp]:
                    if len(rp) == 1 and len(cp) == 1:
                        c, d = l2_diff(rp[0], cp[0])
                    else:
                        c, d = [f"L2 {ritems[0]['type']}"], group_detail(rp, cp)
                    cls += c
                    detail.append(d)
                if new_dangling:
                    cls.append("L2 dangling links")
                    detail.append(f"dangling only in the candidate {short(sorted(new_dangling))}")
                files[n]["L2"] = (entry("diff", cls, "; ".join(detail), ["L2", rp, cp, sorted(new_dangling)])
                                  if cls else entry("ok"))
            # L3
            rp = sorted((l3_payload(e) for e in ritems if l3_payload(e) is not None), key=canon)
            cp = sorted((l3_payload(e) for e in citems if l3_payload(e) is not None), key=canon)
            if rp or cp:
                if [canon(x) for x in rp] == [canon(x) for x in cp]:
                    files[n]["L3"] = entry("ok")
                else:
                    if len(rp) == 1 and len(cp) == 1:
                        rtext = (ritems[0].get("L3") or {}).get("text") or {}
                        ctext = (citems[0].get("L3") or {}).get("text") or {}
                        c, d = l3_diff(rp[0], cp[0], rtext, ctext)
                    else:
                        c, d = ["L3 " + ritems[0]["type"]], group_detail(rp, cp)
                    files[n]["L3"] = entry("diff", c, d, ["L3", rp, cp])
    else:
        notes.append("L2, L3: no unminified pass on both sides")

    # L4: the minified pass; a site published unminified (docs-live: one pass, its manifests
    # extracted with L4) has it in the unminified pass.
    l4 = None
    if ref.min is not None and cand.min is not None:
        l4 = (ref.min, cand.min)
    elif (ref.min is None and cand.min is None and ref.unmin is not None and cand.unmin is not None
          and "L4" in ref.unmin.get("levels", []) and "L4" in cand.unmin.get("levels", [])):
        l4 = (ref.unmin, cand.unmin)
        notes.append("L4: from the unminified pass (no minified pass on both sides)")
    if l4 is not None:
        rg, cg = by_norm(l4[0]), by_norm(l4[1])
        for n in sorted(set(rg) & set(cg)):
            static = any(e.get("static") for e in rg[n])
            rp = sorted((l4_payload(e, static) for e in rg[n] if l4_payload(e, static) is not None), key=canon)
            cp = sorted((l4_payload(e, static) for e in cg[n] if l4_payload(e, static) is not None), key=canon)
            if not (rp or cp):
                continue
            if [canon(x) for x in rp] == [canon(x) for x in cp]:
                files[n]["L4"] = entry("ok")
            elif len(rp) == 1 and len(cp) == 1:
                c, d = l4_diff(rp[0], cp[0])
                files[n]["L4"] = entry("diff", c, d, ["L4", rp, cp])
            else:
                files[n]["L4"] = entry("diff", ["L4 " + rg[n][0]["type"]],
                                       group_detail(rp, cp), ["L4", rp, cp])
    else:
        notes.append("L4: no minified pass on both sides")

    # S
    if ref.structure is not None and cand.structure is not None:
        structure = {k: {"S": e} for k, e in compare_structure(ref.structure, cand.structure).items()}
    else:
        structure = {}
        notes.append("S: no structure dump on " + ("both sides" if ref.structure is None and cand.structure is None
                                                    else "one side"))

    text = a7(ref.unmin, cand.unmin) if ref.unmin is not None and cand.unmin is not None else None
    if text is not None:
        for n, p in text.pop("byFile").items():
            e = files.get(n, {}).get("L3")
            if e is not None and e["status"] != "ok":
                more = f" (+{p['hunkCount'] - 1} more)" if p["hunkCount"] > 1 else ""
                e["detail"] = f"{e.get('detail', '')}; {p['hunks'][0]}{more}".lstrip("; ")
    return {"schema": SCHEMA, "site": site, "ref": ref.name, "cand": cand.name,
            "collisionDirs": cdirs, "notes": notes, "passes": passes,
            "files": {k: dict(sorted(v.items())) for k, v in sorted(files.items())},
            "structure": structure, "a7": text}


def summarize(res):
    levels = {}
    for lv in LEVELS:
        section = res["structure"] if lv == "S" else res["files"]
        counts = collections.Counter(v[lv]["status"] for v in section.values() if lv in v)
        levels[lv] = {"compared": sum(counts.values()), **{s: counts.get(s, 0) for s in ("ok", "diff", "missing", "extra")}}
    kinds = collections.defaultdict(collections.Counter)
    for k, v in res["structure"].items():
        kinds[k.split(" ", 1)[0]][v["S"]["status"]] += 1
    levels["S"]["byKind"] = {k: dict(c) for k, c in sorted(kinds.items())}
    classes = collections.defaultdict(list)
    for section in ("files", "structure"):
        for k, v in res[section].items():
            for e in v.values():
                for c in e.get("classes", []):
                    classes[c].append(k)
    res["summary"] = levels
    res["classes"] = [{"class": c, "count": len(ks), "examples": ks[:5]}
                      for c, ks in sorted(classes.items(), key=lambda x: (-len(x[1]), x[0]))]
    res["diffs"] = sum(levels[lv]["compared"] - levels[lv]["ok"] for lv in LEVELS)
    return res


# ---------------------------------------------------------------------------------------------
# The ratchet

ENTRY_RE = re.compile(r"^- (?P<site>[A-Za-z0-9_.-]+) (?P<levels>(?:L[1-4]|S)(?:,(?:L[1-4]|S))*) "
                      r"`(?P<key>[^`]+)` (?P<cls>[a-z-]+): (?P<reason>\S.*)$")
ENTRY_START_RE = re.compile(r"^- [A-Za-z0-9_.-]+ (?:L[1-4]|S)[ ,]")


class Change:
    def __init__(self, task, site, levels, pattern, cls, reason, where):
        self.task, self.site, self.levels, self.pattern = task, site, levels, pattern
        self.cls, self.reason, self.where = cls, reason, where
        self.used = 0

    def matches(self, site, level, key):
        return site == self.site and level in self.levels and fnmatch.fnmatchcase(key, self.pattern)


def parse_changes(path, task):
    """The entries of one changes file, and its format errors."""
    changes, errors = [], []
    with open(path, encoding="utf-8") as fh:
        for i, line in enumerate(fh, 1):
            line = line.rstrip("\n")
            where = f"{os.path.basename(path)}:{i}"
            m = ENTRY_RE.match(line)
            if m is None:
                if ENTRY_START_RE.match(line):
                    errors.append(f"{where}: not `- <site> <level>[,<level>] `<key>` <class>: <reason>`: {line}")
                continue
            if m["cls"] not in CLASSES:
                errors.append(f"{where}: triage class {m['cls']!r} is not one of {', '.join(CLASSES)}")
                continue
            changes.append(Change(task, m["site"], m["levels"].split(","), m["key"], m["cls"],
                                  m["reason"].strip(), where))
    return changes, errors


def load_changes(changes_dir, tasks):
    changes, errors = [], []
    for task in tasks:
        path = os.path.join(changes_dir, f"{task}.md")
        if not os.path.isfile(path):
            errors.append(f"{path}: no changes file for task {task}")
            continue
        c, e = parse_changes(path, task)
        changes += c
        errors += e
    return changes, errors


def sig(v):
    """`ok`, `<status>:<fp>`, or None (no entry)."""
    if v is None:
        return None
    if isinstance(v, str):
        return v
    if v["status"] == "ok":
        return "ok"
    return f"{v['status']}:{v.get('fp', '')}"


def flatten(sections):
    out = {}
    for section in ("files", "structure"):
        for key, levels in (sections.get(section) or {}).items():
            for lv, v in levels.items():
                out[(section, key, lv)] = v
    return out


def read_baseline(path):
    if path is None:
        return None
    path = existing(path)
    if not os.path.exists(path):
        return None
    doc = read_json(path)
    if doc.get("schema") != BASELINE_SCHEMA:
        sys.exit(f"{path}: not a {BASELINE_SCHEMA} baseline")
    return doc


def ratchet(res, baseline, changes, site):
    """Compares the result with the baseline; returns the ratchet report and the new baseline."""
    cur = flatten(res)
    base = flatten(baseline or {})
    unlisted, listed, improved, benign, wrong_class = [], [], [], [], []
    new = dict(base)
    for k in sorted(set(cur) | set(base)):
        c, b = cur.get(k), base.get(k)
        cs, bs = sig(c), sig(b)
        if cs == bs:
            continue
        section, key, lv = k
        item = {"section": section, "key": key, "level": lv, "was": bs or "none", "now": cs or "none"}
        if c is not None and cs != "ok":
            item["classes"] = c.get("classes", [])
            if c.get("detail"):
                item["detail"] = c["detail"]
        good = cs in (None, "ok")
        if good and bs in (None, "ok"):
            benign.append(item)
            if c is None:
                new.pop(k, None)
            else:
                new[k] = "ok"
            continue
        match = next((ch for ch in changes if ch.matches(site, lv, key)), None)
        if match is None:
            (improved if good else unlisted).append(item)
            continue
        match.used += 1
        item.update({"task": match.task, "class": match.cls, "reason": match.reason})
        if not good and match.cls == "bug-fixed":
            wrong_class.append(item)
            continue
        listed.append(item)
        if c is None:
            new.pop(k, None)
        elif good:
            new[k] = "ok"
        else:
            new[k] = {"status": c["status"], "fp": c["fp"], "class": match.cls,
                      "task": match.task, "reason": match.reason}
    unused = [f"{ch.where}: {ch.site} {','.join(ch.levels)} `{ch.pattern}` matched no change"
              for ch in changes if ch.used == 0 and ch.site == site]
    doc = {"schema": BASELINE_SCHEMA, "site": site, "files": {}, "structure": {}}
    for (section, key, lv), v in new.items():
        doc[section].setdefault(key, {})[lv] = v
    for section in ("files", "structure"):
        doc[section] = {k: dict(sorted(v.items())) for k, v in sorted(doc[section].items())}
    return {"baseline": baseline is not None, "unlisted": unlisted, "listed": listed,
            "improved": improved, "benign": len(benign), "wrongClass": wrong_class,
            "unused": unused}, doc


def dump_baseline(doc):
    lines = [f'"schema": {json.dumps(doc["schema"])}', f'"site": {json.dumps(doc["site"])}']
    for section in ("files", "structure"):
        body = ",\n".join(f"{json.dumps(k, ensure_ascii=False)}: {json.dumps(v, ensure_ascii=False, sort_keys=True)}"
                          for k, v in doc[section].items())
        lines.append(f'"{section}": {{\n{body}\n}}' if body else f'"{section}": {{}}')
    return "{\n" + ",\n".join(sorted(lines)) + "\n}\n"


def write_baseline(path, doc):
    text = dump_baseline(doc).encode("utf-8")
    plain = path[:-3] if path.endswith(".gz") else path
    for p in (plain, plain + ".gz"):
        if os.path.exists(p):
            os.remove(p)
    if len(text) > GZIP_OVER:
        buf = io.BytesIO()
        with gzip.GzipFile(filename="", mode="wb", fileobj=buf, compresslevel=9, mtime=0) as gz:
            gz.write(text)
        plain, text = plain + ".gz", buf.getvalue()
    os.makedirs(os.path.dirname(os.path.abspath(plain)), exist_ok=True)
    with open(plain, "wb") as fh:
        fh.write(text)
    return plain


# ---------------------------------------------------------------------------------------------
# The report

def report_text(res, show):
    out = []
    s = res["summary"]
    out.append(f"structdiff {res['site']}: {res['ref']} (reference) vs {res['cand']} (candidate)")
    for pass_, p in res["passes"].items():
        out.append(f"  L1 {pass_:<10} {p['matched']}/{p['refFiles']} files matched (candidate "
                   f"{p['candFiles']} files; paths {p['missing']} missing, {p['extra']} extra)")
    if res["collisionDirs"]:
        out.append(f"     collision directories (collapsed): {', '.join(res['collisionDirs'])}")
    for lv, what in (("L2", "files with equal links"), ("L3", "files with equal text"),
                     ("L4", "files with equal assets")):
        x = s[lv]
        out.append(f"  {lv} {x['ok']}/{x['compared']} {what}")
    x = s["S"]
    kinds = ", ".join(f"{k} {c.get('ok', 0)}/{sum(c.values())}" for k, c in x["byKind"].items())
    out.append(f"  S  {x['ok']}/{x['compared']} structure facts equal ({kinds})")
    a = res.get("a7")
    if a:
        out.append(f"  A7 {a['ratio']:.4f} ({a['equal']}/{a['pages']} pages with equal visible text; "
                   f"mean similarity {a['meanSimilarity']:.4f})")
    for n in res["notes"]:
        out.append(f"  note: {n}")
    out.append(f"  total: {res['diffs']} differences")
    if res["classes"]:
        out.append("Top difference classes:")
        for c in res["classes"][:25]:
            out.append(f"  {c['count']:>5}  {c['class']:<28} e.g. {', '.join(c['examples'][:3])}")
    if a and a.get("topHunks"):
        out.append("Top visible-text differences (word hunks, pages that have them):")
        for h in a["topHunks"]:
            out.append(f"  {h['pages']:>5}  {h['hunk'][:150]}  e.g. {', '.join(h['examples'][:2])}")
    if a and a["worst"]:
        out.append(f"Worst {len(a['worst'])} pages (A7 similarity: `words` = Dice of the word multisets, "
                   f"`len` = length ratio):")
        for p in a["worst"]:
            h = f"  {p['hunkCount']} hunks, first {p['hunks'][0][:120]}" if p.get("hunks") else ""
            out.append(f"  {p['ratio']:.4f} {p['method']:<7} {p['file']}{h}")
    for lv in LEVELS:
        section = res["structure"] if lv == "S" else res["files"]
        bad = [(k, v[lv]) for k, v in section.items() if lv in v and v[lv]["status"] != "ok"]
        if not bad:
            continue
        out.append(f"{lv} differences ({len(bad)}):")
        for k, e in bad[:show]:
            out.append(f"  {e['status']:<7} {k}" + (f": {e['detail'][:300]}" if e.get("detail") else ""))
        if len(bad) > show:
            out.append(f"  … {len(bad) - show} more")
    r = res.get("ratchet")
    if r:
        out.append("Ratchet:")
        if not r["baseline"]:
            out.append("  no baseline yet: every difference is a new one")
        for name, what in (("unlisted", "UNLISTED (fails)"), ("wrongClass", "WRONG CLASS (fails)"),
                           ("listed", "listed"), ("improved", "improved, not listed (kept in the baseline)")):
            items = r[name]
            if not items:
                continue
            out.append(f"  {what}: {len(items)}")
            for it in items[:show]:
                extra = f" [{it['class']} {it['task']}: {it['reason']}]" if "class" in it else ""
                out.append(f"    {it['level']} {it['key']}: {it['was']} -> {it['now']}{extra}")
            if len(items) > show:
                out.append(f"    … {len(items) - show} more")
        for u in r["unused"]:
            out.append(f"  warning: {u}")
        for e in r.get("errors", []):
            out.append(f"  error: {e}")
        if r.get("written"):
            out.append(f"  baseline written: {r['written']}")
        out.append(f"  verdict: {r['verdict']}")
    return "\n".join(out) + "\n"


def write_json(path, doc):
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(doc, fh, ensure_ascii=False, sort_keys=True, indent=0)
        fh.write("\n")


# ---------------------------------------------------------------------------------------------

def cmd_compare(a):
    ref = Side(a.ref_name,
               load_manifest(a.ref_min, a.ref_project, a.site, "minified"),
               load_manifest(a.ref_unmin, a.ref_project, a.site, "unminified"),
               read_json(existing(a.ref_structure)) if a.ref_structure else None)
    cand = Side(a.cand_name,
                load_manifest(a.cand_min, a.cand_project, a.site, "minified"),
                load_manifest(a.cand_unmin, a.cand_project, a.site, "unminified"),
                read_json(existing(a.cand_structure)) if a.cand_structure else None)
    res = summarize(compare(a.site, ref, cand, a.collision_dir))
    status = 0
    if a.baseline:
        baseline = read_baseline(a.baseline)
        changes, errors = load_changes(a.changes, a.task)
        r, doc = ratchet(res, baseline, changes, a.site)
        r["errors"] = errors
        failed = bool(r["unlisted"] or r["wrongClass"] or errors)
        r["verdict"] = "FAIL" if failed else "pass"
        if a.update:
            if not a.task:
                r["errors"].append("--update needs --task (the changes file that lists the changes)")
                failed = True
                r["verdict"] = "FAIL"
            else:
                r["written"] = write_baseline(a.baseline, doc)
        res["ratchet"] = r
        status = 1 if failed else 0
    elif res["diffs"]:
        status = 1
    text = report_text(res, a.show)
    sys.stdout.write(text)
    if a.report:
        with open(a.report, "w", encoding="utf-8") as fh:
            fh.write(text)
    if a.json:
        write_json(a.json, res)
    if a.report_only:
        return 0
    return status


def cmd_changes(a):
    errors = []
    n = 0
    for name in sorted(os.listdir(a.changes)):
        if not name.endswith(".md") or name == "README.md":
            continue
        c, e = parse_changes(os.path.join(a.changes, name), name[:-3])
        n += len(c)
        errors += e
    for e in errors:
        print(f"structdiff.py changes: {e}", file=sys.stderr)
    print(f"{n} change entries in {a.changes}" + (f", {len(errors)} errors" if errors else ""))
    return 1 if errors else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("compare")
    c.add_argument("--site", required=True)
    for side in ("ref", "cand"):
        c.add_argument(f"--{side}-min")
        c.add_argument(f"--{side}-unmin")
        c.add_argument(f"--{side}-structure")
        c.add_argument(f"--{side}-project")
    c.add_argument("--ref-name", default="golden")
    c.add_argument("--cand-name", default="rust")
    c.add_argument("--collision-dir", action="append", default=[])
    c.add_argument("--json")
    c.add_argument("--report")
    c.add_argument("--show", type=int, default=40)
    c.add_argument("--baseline")
    c.add_argument("--task", action="append", default=[])
    c.add_argument("--changes", default=CHANGES_DIR)
    c.add_argument("--update", action="store_true")
    c.add_argument("--report-only", action="store_true")
    ch = sub.add_parser("changes")
    ch.add_argument("--changes", default=CHANGES_DIR)
    a = ap.parse_args()
    sys.exit(cmd_compare(a) if a.cmd == "compare" else cmd_changes(a))


if __name__ == "__main__":
    main()

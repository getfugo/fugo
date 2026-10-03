#!/usr/bin/env python3
"""Extracts the old port's nh-tplfuncs Go-oracle cases that map onto ssg-funcs filters into
one JSONL fixture, tests/fixtures/tplfuncs.jsonl.gz (read by tests/it/oracle.rs).

Usage (from the repository root; Python stdlib only): extract `crates/nh-tplfuncs` of the tag
`go-parity-final` (git archive) into OLD, then

    python3 crates/funcs/tests/fixtures/extract_tplfuncs.py OLD \
        crates/funcs/tests/fixtures/tplfuncs.jsonl.gz

The output is byte-for-byte reproducible (gzip without a timestamp, sorted cases and keys).

Each output line: {"fam", "f", "input", "input_safe", "kwargs", "lang", "want", "src"}, where
want is {"ok": value, "safe": bool} or {"err": go_message}. Values are plain JSON: Go dates
become {"rfc3339", "unix"}, pages become view-like maps with an `id`. Cases with no native
counterpart are skipped: Go-only argument types (float32, named types, typed nils, structs,
bytes), invalid UTF-8, NaN/Inf, time.Time printed as text, Go panics and reflection errors, and
output formats this port does not have (XML, CSV).
"""
import datetime
import gzip
import json
import math
import struct
import sys

OLD = sys.argv[1]
OUT = sys.argv[2]
FX = OLD + "/crates/nh-tplfuncs/tests/fixtures/"


class Skip(Exception):
    pass


def load(rel):
    return json.load(gzip.open(FX + rel))


VALUES = load("data/values.json.gz")
PAGES = VALUES["pages"]

INT_TYPES = {"int", "int8", "int16", "int32", "int64", "uint", "uint8", "uint16", "uint32", "uint64"}


def date(v):
    unix, off, nsec = v["unix"], v["off"], v["nsec"]
    tz = datetime.timezone(datetime.timedelta(seconds=off))
    dt = datetime.datetime.fromtimestamp(unix, tz)
    s = dt.strftime("%Y-%m-%dT%H:%M:%S")
    if nsec:
        s += ("." + "%09d" % nsec).rstrip("0")
    s += "Z" if off == 0 else dt.strftime("%z")[:3] + ":" + dt.strftime("%z")[3:]
    return {"rfc3339": s, "unix": unix}


def page(i):
    p = PAGES[i]
    params = {}
    for k, v in p["params"]["entries"]:
        params[k.lower()] = conv(v)
    return {
        "id": p["pid"],
        "kind": p["kind"],
        "lang": p["lang"],
        "title": p["title"],
        "link_title": p["linkTitle"],
        "weight": p["weight"],
        "date": date(p["date"]),
        "lastmod": date(p["lastmod"]),
        "publish_date": date(p["publishDate"]),
        "path": p["path"],
        "section": p.get("section", ""),
        "type": p.get("pageType", ""),
        "params": params,
    }


def conv(v):
    """A Go value (fixture encoding) as plain JSON, or Skip."""
    t = v["t"]
    if t == "nil":
        return None
    if t == "bool":
        return v["v"]
    if t in INT_TYPES:
        return int(v["v"])
    if t == "float32":
        raise Skip("float32: no native counterpart")
    if t == "float64":
        f = struct.unpack(">d", bytes.fromhex(v["v"]))[0]
        if math.isnan(f) or math.isinf(f):
            raise Skip("non-finite")
        return f
    if t in ("string", "template.HTML", "template.CSS", "template.JS", "template.JSStr",
             "template.URL", "template.HTMLAttr"):
        if not isinstance(v["s"], str):
            raise Skip("invalid utf-8")
        return v["s"]
    if t == "time.Time":
        return date(v)
    if "items" in v:
        if t in ("[]uint8",):
            raise Skip("bytes")
        return [conv(x) for x in v["items"]]
    if "entries" in v:
        out = {}
        for k, x in v["entries"]:
            if not isinstance(k, str):
                raise Skip("key")
            out[k.lower() if t == "maps.Params" else k] = conv(x)
        return out
    if "page" in v and t == "*hugolib.pageState":
        return page(v["page"])
    raise Skip(t)


def is_safe(v):
    return v.get("t") == "template.HTML"


def go_str(v):
    """cast.ToStringE of a scalar, or Skip."""
    t = v["t"]
    if t == "nil":
        return ""
    if t == "string" or t.startswith("template."):
        if not isinstance(v["s"], str):
            raise Skip("utf8")
        return v["s"]
    if t in INT_TYPES:
        return v["v"]
    if t == "bool":
        return "true" if v["v"] else "false"
    raise Skip("not a string")


def want_of(r, pages_as_ids=False):
    if "err" in r:
        return {"err": r["err"] if isinstance(r["err"], str) else "?"}
    ok = r["ok"]
    return {"ok": conv(ok), "safe": ok.get("t") == "template.HTML"}


OUT_CASES = []


def has_time(v):
    if isinstance(v, dict):
        if v.get("t") == "time.Time":
            return True
        return any(has_time(x) for x in v.get("items", [])) or any(
            has_time(x[1]) for x in v.get("entries", []))
    return False


# Families where a Go time.Time argument is only printed (cast to a string, JSON): this port dates
# are `{rfc3339, unix}` maps, so these cases have no counterpart.
TEXT_FAMILIES = {"jsonify", "plainify", "hash", "strings", "html_escape", "html_unescape",
                 "emojify", "humanize", "inflect", "delimit", "urlize", "anchorize", "title_case",
                 "truncate_html", "path", "urls", "pad", "remarshal"}


def go_artifact(r):
    """Go panics and reflection failures: not behaviour to compare against."""
    e = r.get("err")
    return isinstance(e, str) and (e.startswith("runtime error") or e.startswith("reflect"))


def emit(fam, f, input_v, kwargs, r, src, lang="en", input_safe=False, raw_input=False):
    if go_artifact(r):
        return
    if not raw_input and isinstance(input_v, dict) and input_v.get("t") == "time.Time":
        return
    if fam in TEXT_FAMILIES and not raw_input and has_time(input_v):
        return
    try:
        inp = input_v if raw_input else conv(input_v)
        if not raw_input:
            input_safe = is_safe(input_v)
        w = want_of(r)
    except Skip:
        return
    OUT_CASES.append({"fam": fam, "f": f, "input": inp, "input_safe": input_safe,
                      "kwargs": kwargs, "lang": lang, "want": w, "src": src})


def has_named(v):
    """Go named types (`hstring.HTML`, `json.Number`): no native counterpart."""
    if isinstance(v, dict):
        if v.get("t") == "named":
            return True
        return any(has_named(x) for x in v.get("items", [])) or any(
            has_named(x[1]) for x in v.get("entries", []))
    return False


def data_cases(file):
    d = load("data/" + file)
    for i, c in enumerate(d["cases"]):
        args = [VALUES["values"][j] for j in c["a"]]
        if any(has_named(a) for a in args):
            continue
        yield i, c, args


# ── sort_by ──
GO_FIELDS = {"Title": "title", "LinkTitle": "link_title", "Weight": "weight", "Date": "date",
             "Kind": "kind", "Type": "type", "Section": "section", "Lang": "lang", "Path": "path",
             "Params": "params", "Missing": "missing"}


def page_path(key):
    parts = [p for p in key.strip(".").split(".")]
    if not parts or parts == [""]:
        return ""
    head = GO_FIELDS.get(parts[0])
    if head is None:
        raise Skip("go method")
    rest = [p.lower() for p in parts[1:]] if head == "params" else parts[1:]
    return ".".join([head] + rest)


def has_pages(v):
    return v.get("t") == "page.Pages" or any(
        x.get("t", "").startswith("*hugolib") or x.get("t", "").startswith("hugolib")
        for x in v.get("items", []))


for file in ["sort.json.gz", "coll_Sort.json.gz", "pages_sort.json.gz", "scalar_collections.json.gz"]:
    for i, c, args in data_cases(file):
        if c["m"] != "Sort" or not args:
            continue
        try:
            seq = args[0]
            key = ""
            if len(args) > 1:
                k1 = args[1]
                if "items" in k1 or "entries" in k1:
                    key = ""  # cast.ToStringE fails: Go sorts by value
                else:
                    key = go_str(k1)  # Skip for time, float, …: Go prints them into a path
            reverse = False
            if len(args) > 2:
                try:
                    reverse = go_str(args[2]) == "desc"
                except Skip:
                    reverse = False
            if len(args) > 3:
                continue
            k = key.strip(".")
            if k and k != "value" and has_time(seq):
                continue  # Go dates have no attributes; this port dates are maps
            if k == "value" and seq.get("items") is not None:
                k = ""
            if has_pages(seq) and k:
                k = page_path(k)
            elif "Params" in k or k[:1].isupper():
                raise Skip("go field on non-page")
            lang = "th" if c["ns"].endswith("@th") else "en"
            kw = {"attribute": k}
            if reverse:
                kw["reverse"] = True
            emit("sort_by", "sort_by", seq, kw, c["r"], f"data/{file}#{i}", lang)
        except Skip:
            pass

# ── set operations ──
for file in ["coll_sets.json.gz", "coll_Union.json.gz", "coll_Intersect.json.gz",
             "coll_SymDiff.json.gz", "coll_Complement.json.gz", "pages_sets.json.gz"]:
    for i, c, args in data_cases(file):
        m = c["m"]
        src = f"data/{file}#{i}"
        try:
            if m in ("Union", "Intersect") and len(args) == 2:
                emit("sets", m.lower(), args[0], {"with": conv(args[1])}, c["r"], src)
            elif m == "SymDiff" and len(args) == 2:
                emit("sets", "symdiff", args[1], {"with": conv(args[0])}, c["r"], src)
            elif m == "Complement" and len(args) == 2:
                emit("sets", "complement", args[1], {"without": conv(args[0])}, c["r"], src)
        except Skip:
            pass

# ── default_if_empty ──
for file in ["compare_Default.json.gz", "scalar_compare.json.gz", "pages_compare.json.gz"]:
    for i, c, args in data_cases(file):
        if c["m"] != "Default" or len(args) != 2:
            continue
        try:
            emit("default_if_empty", "default_if_empty", args[1], {"value": conv(args[0])},
                 c["r"], f"data/{file}#{i}")
        except Skip:
            pass

# ── jsonify ──
for file in ["encoding_jsonify.json.gz", "scalar_encoding.json.gz"]:
    for i, c, args in data_cases(file):
        if c["m"] != "Jsonify":
            continue
        try:
            if len(args) == 1:
                emit("jsonify", "jsonify", args[0], {}, c["r"], f"data/{file}#{i}")
            elif len(args) == 2:
                opts = conv(args[0])
                if not isinstance(opts, dict) or set(opts) - {"indent"}:
                    continue
                kw = {}
                if "indent" in opts:
                    if not isinstance(opts["indent"], str):
                        continue
                    kw["indent"] = opts["indent"]
                emit("jsonify", "jsonify", args[1], kw, c["r"], f"data/{file}#{i}")
        except Skip:
            pass

# ── hashes ──
for i, c, args in data_cases("scalar_crypto.json.gz"):
    f = {"MD5": "md5", "SHA1": "sha1", "SHA256": "sha256", "FNV32a": "fnv32a"}.get(c["m"])
    if f and len(args) == 1:
        emit("hash", f, args[0], {}, c["r"], f"data/scalar_crypto.json.gz#{i}")
for i, c, args in data_cases("scalar_hash.json.gz"):
    f = {"FNV32a": "fnv32a", "XxHash": "xxhash"}.get(c["m"])
    if f and len(args) == 1:
        emit("hash", f, args[0], {}, c["r"], f"data/scalar_hash.json.gz#{i}")

# ── merge, querify, delimit, append ──
for file in ["coll_merge.json.gz", "coll_Merge.json.gz"]:
    for i, c, args in data_cases(file):
        if c["m"] == "Merge" and len(args) == 2 and all(
                x["t"] == "nil" or "entries" in x or x["t"] in ("string", "int", "bool") for x in args):
            try:
                emit("merge", "merge", args[0], {"with": conv(args[1])}, c["r"], f"data/{file}#{i}")
            except Skip:
                pass
for file in ["coll_querify.json.gz", "coll_Querify.json.gz"]:
    for i, c, args in data_cases(file):
        if c["m"] == "Querify" and len(args) == 1 and "entries" in args[0]:
            try:
                emit("querify", "querify", None, {"params": conv(args[0])}, c["r"],
                     f"data/{file}#{i}", raw_input=True)
            except Skip:
                pass
for file in ["coll_delimit.json.gz", "coll_Delimit.json.gz"]:
    for i, c, args in data_cases(file):
        if c["m"] == "Delimit" and len(args) in (2, 3) and args[0]["t"] not in ("string", "template.HTML"):
            try:
                kw = {"sep": go_str(args[1])}
                if len(args) == 3:
                    kw["last"] = go_str(args[2])
                emit("delimit", "delimit", args[0], kw, c["r"], f"data/{file}#{i}")
            except Skip:
                pass

# ── host fixtures ──


def host_cases(file):
    d = load("host/" + file)
    for i, c in enumerate(d["cases"]):
        if any(has_named(a) for a in c["a"]):
            continue
        yield i, c, c["a"]


for i, c, a in host_cases("strings_truncate.json.gz"):
    if c["m"] != "strings.Truncate" or len(a) not in (2, 3):
        continue
    try:
        if a[0]["t"] not in INT_TYPES:
            continue
        kw = {"length": int(a[0]["v"])}
        if len(a) == 3:
            if a[1]["t"] not in ("string", "template.HTML"):
                continue
            kw["ellipsis"] = go_str(a[1])
            if is_safe(a[1]):
                continue  # a safe ellipsis has no kwarg form
        emit("truncate_html", "truncate_html", a[-1], kw, c["r"], f"host/strings_truncate#{i}")
    except Skip:
        pass

for i, c, a in host_cases("inflect.json.gz"):
    f = {"inflect.Humanize": "humanize", "inflect.Pluralize": "pluralize_word",
         "inflect.Singularize": "singularize_word", "f:humanize": "humanize",
         "f:pluralize": "pluralize_word", "f:singularize": "singularize_word"}.get(c["m"])
    if f and len(a) == 1:
        emit("humanize" if f == "humanize" else "inflect", f, a[0], {}, c["r"], f"host/inflect#{i}")

for file in ["urls_en0.json.gz", "urls_th0.json.gz"]:
    for i, c, a in host_cases(file):
        f = {"urls.URLize": "urlize", "urls.Anchorize": "anchorize"}.get(c["m"])
        if f and len(a) == 1:
            emit(f, f, a[0], {}, c["r"], f"host/{file}#{i}")

for i, c, a in host_cases("transform_text.json.gz"):
    f = {"transform.HTMLEscape": "html_escape", "f:htmlEscape": "html_escape",
         "transform.HTMLUnescape": "html_unescape", "f:htmlUnescape": "html_unescape",
         "transform.Plainify": "plainify", "f:plainify": "plainify",
         "transform.Emojify": "emojify", "f:emojify": "emojify"}.get(c["m"])
    if f and len(a) == 1:
        emit(f, f, a[0], {}, c["r"], f"host/transform_text#{i}")
        # html_escape on safe input: the same cases with the input marked safe
        if f == "html_escape" and a[0]["t"] == "string":
            try:
                OUT_CASES.append({"fam": "html_escape", "f": f, "input": conv(a[0]),
                                  "input_safe": True, "kwargs": {}, "lang": "en",
                                  "want": want_of(c["r"]), "src": f"host/transform_text#{i}/safe"})
            except Skip:
                pass

for i, c, a in host_cases("transform_remarshal.json.gz"):
    if len(a) != 2 or a[0]["t"] != "string" or a[0]["s"].strip().lower() in ("xml", "csv"):
        continue
    emit("remarshal", "remarshal", a[1], {"format": a[0]["s"]}, c["r"], f"host/transform_remarshal#{i}")

for i, c, a in host_cases("path.json.gz"):
    f = {"path.Base": "path_base", "path.BaseName": "path_base_name", "path.Clean": "path_clean",
         "path.Dir": "path_dir", "path.Ext": "path_ext"}.get(c["m"])
    if f and len(a) == 1:
        emit("path", f, a[0], {}, c["r"], f"host/path#{i}")
    elif c["m"] == "path.Join":
        try:
            parts = [conv(x) for x in a]
            if len(parts) == 1 and isinstance(parts[0], list):
                parts = parts[0]
            if not all(isinstance(p, str) for p in parts):
                continue
            emit("path", "path_join", None, {"parts": parts}, c["r"], f"host/path#{i}", raw_input=True)
        except Skip:
            pass

STYLE = {"ap/0": "ap", "chicago/0": "chicago", "firstupper/0": "firstupper", "gostyle/0": "go"}
for i, c, a in host_cases("strings_title.json.gz"):
    if len(a) == 1 and c["env"] in STYLE:
        emit("title_case", "title_case", a[0], {"style": STYLE[c["env"]]}, c["r"], f"host/strings_title#{i}")

for i, c, a in host_cases("urls.json.gz"):
    if c["m"] == "urls.URLDecode" and len(a) == 1:
        emit("urls", "urldecode", a[0], {}, c["r"], f"host/urls#{i}")
    elif c["m"] == "urls.JoinPath":
        try:
            parts = [conv(x) for x in a]
            if len(parts) == 1 and isinstance(parts[0], list):
                parts = parts[0]
            if not all(isinstance(p, str) for p in parts):
                continue
            emit("urls", "join_url", None, {"parts": parts}, c["r"], f"host/urls#{i}", raw_input=True)
        except Skip:
            pass
    elif c["m"] == "urls.Parse" and len(a) == 1 and "ok" in c["r"]:
        ok = c["r"]["ok"]
        if any(not isinstance(ok[k], str) for k in ("path", "fragment", "host", "rawQuery", "s", "scheme")):
            continue
        want = {"ok": {"scheme": ok["scheme"], "host": ok["host"], "path": ok["path"],
                       "fragment": ok["fragment"], "query": ok["rawQuery"],
                       "is_absolute": ok["abs"], "string": ok["s"]}, "safe": False}
        try:
            OUT_CASES.append({"fam": "urls", "f": "parse_url", "input": conv(a[0]), "input_safe": False,
                              "kwargs": {}, "lang": "en", "want": want, "src": f"host/urls#{i}"})
        except Skip:
            pass

for i, c, a in host_cases("strings_binary.json.gz"):
    f = {"strings.Trim": ("trim_chars", "chars"), "strings.TrimLeft": ("trim_start_chars", "chars"),
         "strings.TrimRight": ("trim_end_chars", "chars"), "strings.TrimPrefix": ("strip_prefix", "prefix"),
         "strings.TrimSuffix": ("strip_suffix", "suffix")}.get(c["m"])
    if not f or len(a) != 2:
        continue
    try:
        # Trim(s, cutset); TrimLeft(cutset, s); TrimRight(cutset, s); TrimPrefix(prefix, s);
        # TrimSuffix(suffix, s)
        if c["m"] == "strings.Trim":
            s, arg = a[0], a[1]
        else:
            s, arg = a[1], a[0]
        if arg["t"] not in ("string", "template.HTML"):
            continue
        emit("strings", f[0], s, {f[1]: go_str(arg)}, c["r"], f"host/strings_binary#{i}")
    except Skip:
        pass

for i, c, a in host_cases("strings_slice.json.gz"):
    if c["m"] != "strings.Substr" or len(a) not in (2, 3):
        continue
    if any(x["t"] not in INT_TYPES for x in a[1:]):
        continue
    kw = {"start": int(a[1]["v"])}
    if len(a) == 3:
        kw["length"] = int(a[2]["v"])
    if any(abs(v) >= 2**62 for v in kw.values()):
        continue  # Go overflows start + length
    emit("strings", "substr", a[0], kw, c["r"], f"host/strings_slice#{i}")

for i, c, a in host_cases("strings_regexp.json.gz"):
    if c["m"] != "strings.FindRE" or len(a) not in (2, 3):
        continue
    try:
        if a[0]["t"] != "string":
            continue
        kw = {"pattern": go_str(a[0])}
        if len(a) == 3:
            if a[2]["t"] not in INT_TYPES:
                continue
            kw["limit"] = int(a[2]["v"])
        emit("strings", "regex_find", a[1], kw, c["r"], f"host/strings_regexp#{i}")
    except Skip:
        pass

# pad_*: printf "%5s" (pad_start) and "%-5s" has no fixture; "%5s" only
for i, c, args in data_cases("fmt_printf.json.gz"):
    fmt = args[0]
    if fmt.get("t") == "string" and fmt.get("s") == "%5s" and len(args) == 2:
        if args[1]["t"] not in ("string", "template.HTML"):
            continue
        emit("pad", "pad_start", args[1], {"width": 5}, c["r"], f"data/fmt_printf#{i}")

OUT_CASES.sort(key=lambda c: (c["fam"], c["f"], c["src"]))
import io
with gzip.GzipFile(OUT, "wb", mtime=0) as raw:
    out = io.TextIOWrapper(raw, encoding="utf-8")
    for c in OUT_CASES:
        out.write(json.dumps(c, ensure_ascii=False, sort_keys=True) + "\n")
    out.flush()
import collections
print(collections.Counter(c["fam"] for c in OUT_CASES))

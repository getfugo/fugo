#!/usr/bin/env python3
"""Converts Go-oracle fixtures to plain JSON with the fixture schema (Python stdlib only).

Usage:
  fixtures2json.py convert <src> <dst>   # a file or a directory tree; <dst> may equal <src> (in place)
  fixtures2json.py count <path>...       # print the record counts of fixture files
  fixtures2json.py t00 <old-crates-dir> <testdata-dir>
                                         # the one-off T00 move (see T00_MAP); writes COUNTS.json

The Go oracles (tools/go-oracle, frozen at 44529028) wrote the typed formats below; re-run
`convert <dir> <dir>` after regenerating an oracle there (conversion is idempotent).

The fixture schema
------------------
Every file keeps its name and compression (`.json`, `.json.gz`, `.jsonl`, `.jsonl.gz`; gzip is
written with mtime 0, so re-runs are byte-identical). Files that are not JSON are copied verbatim.
Inside a JSON document the type tags of the Go oracles are unwrapped into plain JSON values:

  goval / tval (tools/go-oracle/nh-common/goval, nh-parser/tval)
    {"t":"nil"}, {"t":"nil:<type>"}             -> null
    {"t":"bool","v":b}                           -> b
    {"t":"int64","v":"-12"} (any int/uint kind)  -> -12
    {"t":"float64","v":"<16 hex bits>"}          -> JSON number (1.0 stays 1.0); NaN/±Inf -> {"$nh:float":"NaN"|"+Inf"|"-Inf"}
    {"t":"string"|"template.*","s":s}            -> s; invalid UTF-8 ({"hex":..}) -> {"$nh:bytes":"<hex>"}
    {"hex":"<hex>"} (goval.Str outside a tag), {"b64":"<base64>"} (absurl, collector, convert)
                                                 -> the string, or {"$nh:bytes":"<hex>"} if not UTF-8
    {"t":"time.Time","unix":..,"nsec":..,"loc":..,"abbr":..,"off":..}
                                                 -> {"$nh:time":"2006-01-02T15:04:05.999999999+07:00[Asia/Bangkok]"}
                                                    (RFC 9557; the [zone] suffix only for IANA names; "Z" for UTC)
    {"t":"toml.Local*","s":s,...}                -> {"$nh:local":s}  (date, time or date-time by its shape)
    {"t":"named","name":..,"under":v}            -> v
    {"t":<slice type>,"items":[..]}              -> [..]
    {"t":<map type>,"entries":[[k,v],..]}        -> {k: v, ..}  (keys stay in the oracle's sorted order)
    {"t":<type>,"keys":[..]} / {"t":<type>,"len":n}   (goval.Shallow) -> {"$nh:keys":[..]} / {"$nh:len":n}

  the "tv" dialect of the i18n / resources / resource-transformers oracles
    {"t":T,"v":v} with T one of nil, bool, string, int*, uint*, float*, map, slice, []string, []any,
    []interface {}, map[string]interface {}, map[string]any, maps.Params          -> v
      (a float v may be Go's %v text, a uint64 v a decimal string, and a map/maps.Params v a
      list of sorted [key, value] pairs, as tools/go-oracle/nh-resources/rsupport.Enc writes them)
    {"t":"nilmap"}                                                                -> null

Any other object with a "t" key is an oracle *record* (a page reference, a collector element, a
Go type under test such as time.Duration or css.QuotedString) and is kept as an object; its
fields are converted recursively. Go int kinds, typed nils and maps.Params vs map[string]any are
not kept: tests compare semantic fields only.

Record counts
-------------
`records` of a file: JSONL -> number of lines; a top-level array -> its length; a top-level
object -> the sum over its members of (length of an array/object member, else 1). `values`:
the number of JSON values in the document, where a tagged Go value counts once. Both are computed
on the source (tag-aware) and on the result and must be equal; `t00` records them in
<testdata>/COUNTS.json.
"""
import base64
import binascii
import datetime
import gzip
import io
import json
import math
import os
import shutil
import struct
import sys

INTS = {"int", "int8", "int16", "int32", "int64", "uint", "uint8", "uint16", "uint32", "uint64", "uintptr"}
FLOATS = {"float32", "float64"}
TIME_KEYS = {"t", "unix", "nsec", "loc", "abbr", "off"}
TV_TYPES = INTS | FLOATS | {"nil", "bool", "string", "map", "slice", "[]string", "[]any", "[]interface {}",
                            "map[string]interface {}", "map[string]any", "maps.Params"}


class ConvertError(Exception):
    pass


def _gostr(s):
    if isinstance(s, str):
        return s
    if isinstance(s, dict) and set(s) == {"hex"}:
        b = binascii.unhexlify(s["hex"])
        try:
            return b.decode("utf-8")
        except UnicodeDecodeError:
            return {"$nh:bytes": s["hex"]}
    raise ConvertError(f"bad goval string {s!r}")


def _is_hex(s):
    return isinstance(s, str) and len(s) % 2 == 0 and all(c in "0123456789abcdef" for c in s)


def _key(k):
    k = _gostr(k)
    if not isinstance(k, str):
        raise ConvertError(f"map key is not valid UTF-8: {k!r}")
    return k


def _float(v):
    if isinstance(v, str) and len(v) == 16 and all(c in "0123456789abcdef" for c in v):
        f = struct.unpack(">d", binascii.unhexlify(v))[0]  # goval: IEEE bits
    elif isinstance(v, str):
        f = float(v)  # tv dialect: Go's %v ("2.5", "1e+21", "NaN", "+Inf")
    else:
        f = float(v)
    if math.isnan(f):
        return {"$nh:float": "NaN"}
    if math.isinf(f):
        return {"$nh:float": "+Inf" if f > 0 else "-Inf"}
    return f


def _civil(days):
    """(year, month, day) of a day count since 1970-01-01, proleptic Gregorian, any year."""
    z = days + 719468
    era = z // 146097
    doe = z - era * 146097
    yoe = (doe - doe // 1460 + doe // 36524 - doe // 146096) // 365
    doy = doe - (365 * yoe + yoe // 4 - yoe // 100)
    mp = (5 * doy + 2) // 153
    d = doy - (153 * mp + 2) // 5 + 1
    m = mp + 3 if mp < 10 else mp - 9
    return yoe + era * 400 + (m <= 2), m, d


def _time(d):
    unix, nsec, off, loc = d["unix"], d["nsec"], d["off"], d["loc"]
    days, secs = divmod(unix + off, 86400)
    y, mo, da = _civil(days)
    # ISO 8601 expanded years (±YYYYYY) outside 0000-9999, e.g. Go's time.Unix(1<<62, 0).
    ys = f"{y:04d}" if 0 <= y <= 9999 else f"{'-' if y < 0 else '+'}{abs(y):06d}"
    s = f"{ys}-{mo:02d}-{da:02d}T{secs // 3600:02d}:{secs % 3600 // 60:02d}:{secs % 60:02d}"
    if nsec:
        s += "." + f"{nsec:09d}".rstrip("0")
    if off == 0 and loc == "UTC":
        s += "Z"
    else:
        sign = "-" if off < 0 else "+"
        a = abs(off)
        s += f"{sign}{a // 3600:02d}:{a % 3600 // 60:02d}" + (f":{a % 60:02d}" if a % 60 else "")
    if "/" in loc:
        s += f"[{loc}]"
    return {"$nh:time": s}


def tagged(d):
    """Returns the kind of Go tag of object `d`, or None for a record."""
    if set(d) == {"hex"} and _is_hex(d["hex"]):
        return "hex"
    if set(d) == {"b64"} and isinstance(d["b64"], str):
        return "b64"
    t = d.get("t")
    if not isinstance(t, str):
        return None
    k = set(d)
    if k == {"t"}:
        return "nil" if t == "nil" or t.startswith("nil:") or t == "nilmap" else None
    if k == {"t", "v"}:
        if t == "bool" or t in INTS:
            return "int" if t in INTS else "bool"
        if t in FLOATS:
            return "float"
        if t in TV_TYPES:
            return "tv"
        return None
    if k == {"t", "s"} and (t == "string" or t.startswith("template.")):
        return "string"
    if t.startswith("toml.Local") and "s" in k and k <= {"t", "s", "f", "utc", "ict"}:
        return "local"
    if k == TIME_KEYS and t == "time.Time":
        return "time"
    if k == {"t", "items"} and isinstance(d["items"], list):
        return "items"
    if k == {"t", "entries"} and isinstance(d["entries"], list) and all(
            isinstance(e, list) and len(e) == 2 for e in d["entries"]):
        return "entries"
    if k == {"t", "name", "under"} and t == "named":
        return "named"
    gotype = t.startswith("[]") or t.startswith("map[") or "." in t
    if gotype and k == {"t", "keys"}:
        return "keys"
    if gotype and k == {"t", "len"}:
        return "len"
    return None


def plain(v):
    """Converts one JSON value (see the module docstring)."""
    if isinstance(v, list):
        return [plain(x) for x in v]
    if not isinstance(v, dict):
        return v
    kind = tagged(v)
    if kind is None:
        return {k: plain(x) for k, x in v.items()}
    if kind == "nil":
        return None
    if kind == "bool":
        return bool(v["v"])
    if kind == "int":
        return int(v["v"])
    if kind == "float":
        return _float(v["v"])
    if kind == "tv":
        t = v["t"]
        x = v["v"]
        if t == "nil":
            return None
        if t in ("map", "maps.Params") and isinstance(x, list):
            return {_key(k): plain(y) for k, y in x}  # rsupport.Enc: sorted [key, value] pairs
        return plain(x)
    if kind == "string":
        return _gostr(v["s"])
    if kind == "hex":
        return _gostr(v)
    if kind == "b64":
        return _gostr({"hex": binascii.hexlify(base64.b64decode(v["b64"], validate=True)).decode()})
    if kind == "local":
        return {"$nh:local": v["s"]}
    if kind == "time":
        return _time(v)
    if kind == "items":
        return [plain(x) for x in v["items"]]
    if kind == "entries":
        return {_key(k): plain(x) for k, x in v["entries"]}
    if kind == "named":
        return plain(v["under"])
    if kind == "keys":
        return {"$nh:keys": [_gostr(k) for k in v["keys"]]}
    if kind == "len":
        return {"$nh:len": v["len"]}
    raise AssertionError(kind)


# ------------------------------------------------------------------------------------------------
# Counting


def values(v, source):
    """Number of JSON values; on a source document a Go-tagged value counts as one value plus the
    values of its elements (slices, maps)."""
    if isinstance(v, list):
        return 1 + sum(values(x, source) for x in v)
    if not isinstance(v, dict):
        return 1
    if len(v) == 1 and next(iter(v)).startswith("$nh:"):
        return 1  # an already converted {"$nh:time": ..} etc.
    if source:
        kind = tagged(v)
        if kind == "items":
            return 1 + sum(values(x, True) for x in v["items"])
        if kind == "entries":
            return 1 + sum(values(x, True) for _, x in v["entries"])
        if kind == "named":
            return values(v["under"], True)
        if kind == "tv" and v["t"] in ("map", "maps.Params") and isinstance(v["v"], list):
            return 1 + sum(values(x, True) for _, x in v["v"])
        if kind == "tv" and v["t"] != "nil":
            return values(v["v"], True)
        if kind is not None:
            return 1
    return 1 + sum(values(x, source) for x in v.values())


def _len(v, source):
    if source and isinstance(v, dict):
        kind = tagged(v)
        if kind == "items":
            return len(v["items"])
        if kind == "entries":
            return len(v["entries"])
    if isinstance(v, (list, dict)):
        return len(v)
    return 1


def records(doc, jsonl, source):
    if jsonl or isinstance(doc, list):
        return len(doc)
    if isinstance(doc, dict):
        return sum(_len(x, source) for x in doc.values())
    return 1


# ------------------------------------------------------------------------------------------------
# Files


def json_kind(path):
    base = path[:-3] if path.endswith(".gz") else path
    if base.endswith(".jsonl"):
        return "jsonl"
    if base.endswith(".json"):
        return "json"
    return None


def read_doc(path):
    raw = open(path, "rb").read()
    if path.endswith(".gz"):
        raw = gzip.decompress(raw)
    text = raw.decode("utf-8")
    if json_kind(path) == "jsonl":
        return [json.loads(line) for line in text.split("\n") if line.strip()]
    return json.loads(text)


def write_doc(path, doc, kind):
    if kind == "jsonl":
        text = "".join(json.dumps(x, ensure_ascii=False, separators=(",", ":")) + "\n" for x in doc)
    else:
        text = json.dumps(doc, ensure_ascii=False, separators=(",", ":")) + "\n"
    # Unicode line breaks only occur inside JSON strings; escape them so that no line-based
    # reader (Python's splitlines, editors) splits a JSONL record.
    for c in ("\u0085", " ", " "):
        text = text.replace(c, f"\\u{ord(c):04x}")
    data = text.encode("utf-8")
    if path.endswith(".gz"):
        buf = io.BytesIO()
        with gzip.GzipFile(filename="", mode="wb", fileobj=buf, compresslevel=9, mtime=0) as gz:
            gz.write(data)
        data = buf.getvalue()
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    with open(path, "wb") as fh:
        fh.write(data)


def convert_file(src, dst):
    """Converts (or copies) one file; returns its counts, or None for a non-JSON file."""
    kind = json_kind(src)
    if kind is None:
        if os.path.abspath(src) != os.path.abspath(dst):
            os.makedirs(os.path.dirname(dst) or ".", exist_ok=True)
            shutil.copyfile(src, dst)
        return None
    doc = read_doc(src)
    try:
        out = plain(doc)
    except (ConvertError, ValueError) as e:
        raise SystemExit(f"{src}: {e}")
    before = {"records": records(doc, kind == "jsonl", True), "values": values(doc, True)}
    after = {"records": records(out, kind == "jsonl", False), "values": values(out, False)}
    if before != after:
        raise SystemExit(f"{src}: counts differ after conversion: {before} vs {after}")
    write_doc(dst, out, kind)
    return after


def walk_files(root):
    if os.path.isfile(root):
        yield ""
        return
    for dp, dns, fns in os.walk(root):
        dns.sort()
        for fn in sorted(fns):
            yield os.path.relpath(os.path.join(dp, fn), root)


def convert_tree(src, dst):
    counts = {}
    for rel in walk_files(src):
        s = os.path.join(src, rel) if rel else src
        d = os.path.join(dst, rel) if rel else dst
        c = convert_file(s, d)
        if c is not None:
            counts[rel or os.path.basename(src)] = c
    return counts


# ------------------------------------------------------------------------------------------------
# T00: old crates/ layout -> testdata (REWRITE_PLAN.md §6.3). Paths are relative to crates/
# on the source side and to testdata on the destination side. A directory maps recursively.

T00_MAP = [
    # oracle/<area>: the engine-neutral Go-oracle fixtures (area = old crate name without "nh-";
    # the old site-build crate's, named after its Go package, went to oracle/sitebuild). The
    # source paths are the directory names of the old checkout and stay as it named them.
    *[(f"nh-hugolib/tests/fixtures/{d}", f"oracle/sitebuild/{d}")
      for d in ("capture", "assemble", "content", "site", "build", "data", "funcnames", "hookrec")],
    ("nh-page/tests/fixtures", "oracle/page"),
    *[(f"nh-tplimpl/tests/fixtures/{d}", f"oracle/tplimpl/{d}") for d in ("lookup", "store", "probe")],
    ("nh-allconfig/tests/fixtures/load", "oracle/allconfig/load"),
    ("nh-media/tests/fixtures", "oracle/media"),
    *[(f"nh-parser/tests/fixtures/{d}", f"oracle/parser/{d}") for d in ("pageparser", "metadecoders")],
    *[(f"nh-markup/tests/fixtures/{d}", f"oracle/markup/{d}") for d in ("hooks", "autoid", "convert")],
    *[(f"nh-i18n/tests/fixtures/{d}", f"oracle/i18n/{d}") for d in ("plural", "translate", "parse", "matcher")],
    ("nh-resources/tests/fixtures", "oracle/resources"),
    *[(f"nh-resource-transformers/tests/fixtures/{d}", f"oracle/resource-transformers/{d}")
      for d in ("jsbuild", "postcss", "tocss", "getremote", "site", "t16site")],
    *[(f"nh-images/tests/fixtures/{d}", f"oracle/images/{d}") for d in ("config", "process", "exif")],
    ("nh-publisher/tests/fixtures/collector", "oracle/publisher/collector"),
    ("nh-transform/tests/fixtures/absurl", "oracle/transform/absurl"),
    *[(f"nh-commands/tests/fixtures/{d}", f"oracle/commands/{d}") for d in ("e2e", "cli", "staticcopy", "smoke")],
    ("nh-helpers/tests/fixtures/pathspec", "oracle/helpers/pathspec"),
    *[(f"nh-common/tests/fixtures/{d}", f"oracle/common/{d}")
      for d in ("paths", "urls", "flect", "prose", "locales", "cast", "glob")],
    # corpus/: inputs without Go types
    ("go-time/tests/fixtures/format.txt.gz", "corpus/time/format.txt.gz"),
    ("xtext-collate/tests/fixtures/enum-thai.txt", "corpus/collate/enum-thai.txt"),
    # site-assets/: images sites.py and the image oracles read
    ("go-image/tests/fixtures/site", "site-assets/site"),
    ("go-png/tests/fixtures/repo", "site-assets/repo"),
    ("go-png/tests/fixtures/golden", "site-assets/golden"),
]


def t00(crates, testdata):
    counts = {}
    for src, dst in T00_MAP:
        s = os.path.join(crates, src)
        d = os.path.join(testdata, dst)
        if not os.path.exists(s):
            raise SystemExit(f"missing {s}")
        for rel, c in convert_tree(s, d).items():
            one = os.path.isfile(s)
            counts[dst if one else f"{dst}/{rel}"] = {"from": "crates/" + (src if one else f"{src}/{rel}"), **c}
    # One line per fixture: {"<path under testdata>": {"from": .., "records": .., "values": ..}}
    lines = [f"{json.dumps(k)}: {json.dumps(v)}" for k, v in sorted(counts.items())]
    with open(os.path.join(testdata, "COUNTS.json"), "w", encoding="utf-8") as fh:
        fh.write("{\n" + ",\n".join(lines) + "\n}\n")
    total = sum(c["records"] for c in counts.values())
    print(f"{len(counts)} JSON fixtures converted, {total} records")


def main(argv):
    if len(argv) >= 3 and argv[1] == "convert" and len(argv) == 4:
        for rel, c in convert_tree(argv[2], argv[3]).items():
            print(f"{c['records']:>9} {c['values']:>11}  {rel}")
    elif len(argv) >= 3 and argv[1] == "count":
        for p in argv[2:]:
            for rel in walk_files(p):
                f = os.path.join(p, rel) if rel else p
                kind = json_kind(f)
                if kind:
                    doc = read_doc(f)
                    print(f"{records(doc, kind == 'jsonl', True):>9} {values(doc, True):>11}  {f}")
    elif len(argv) == 4 and argv[1] == "t00":
        t00(argv[2], argv[3])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv)

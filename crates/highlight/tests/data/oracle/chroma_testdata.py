#!/usr/bin/env python3
"""Writes tests/data/chroma-testdata.json.gz from Chroma's own lexer test suite.

    python3 chroma_testdata.py <chroma dir> <cases.jsonl>
    <oracle> analyse < <cases.jsonl> > <analyse.txt>
    python3 chroma_testdata.py <chroma dir> <cases.jsonl> <analyse.txt> <output.json.gz>

<chroma dir> is github.com/alecthomas/chroma/v2@v2.19.0 (e.g. from the Go module cache);
<oracle> is the Go program of `oracle/chroma.go.txt` (crate README). Each
`lexers/testdata/**/*.actual` input becomes a case with the lexer Chroma's `TestLexers` picks for
it (the file's base name, or the directory name) and the FNV-1a hash of its `*.expected` tokens
(type and text of each coalesced token, joined by NUL: the hash `tests/it/lexers.rs` computes);
the inputs of `lexers/testdata/analysis/` are added without tokens. Every case also records the
lexer Chroma's `lexers.Analyse` picks for its text (the Go implementation's `guessSyntax`), from the oracle.
"""
import gzip
import json
import os
import sys


def fnv(parts):
    h = 0xCBF29CE484222325
    for i, part in enumerate(parts):
        data = (b"" if i == 0 else b"\0") + part.encode("utf-8")
        for b in data:
            h ^= b
            h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def read(path):
    return open(path, encoding="utf-8", newline="").read()


def cases(chroma):
    root = os.path.join(chroma, "lexers", "testdata")
    out = []
    for name in sorted(os.listdir(root)):
        path = os.path.join(root, name)
        if name == "analysis":
            continue
        if os.path.isdir(path):
            files = [(name, os.path.join(path, f), f"{name}/{f}") for f in sorted(os.listdir(path)) if f.endswith(".actual")]
        elif name.endswith(".actual"):
            files = [(name[: -len(".actual")].split(".")[0], path, name)]
        else:
            continue
        for lexer, actual, rel in files:
            expected = json.load(open(actual[: -len(".actual")] + ".expected", encoding="utf-8"))
            parts = []
            for t in expected:
                parts += [t["type"], t["value"]]
            out.append({"file": rel, "lexer": lexer, "code": read(actual), "hash": fnv(parts)})
    analysis = os.path.join(root, "analysis")
    for name in sorted(os.listdir(analysis)):
        if name.endswith(".actual"):
            out.append({"file": f"analysis/{name}", "lexer": name.split(".")[0], "code": read(os.path.join(analysis, name))})
    return out


def main():
    all_cases = cases(sys.argv[1])
    if len(sys.argv) == 3:
        with open(sys.argv[2], "w", encoding="utf-8") as f:
            for c in all_cases:
                f.write(json.dumps({"lang": c["lexer"], "code": c["code"]}) + "\n")
        return
    picks = open(sys.argv[3], encoding="utf-8").read().split("\n")
    for c, pick in zip(all_cases, picks):
        c["analyse"] = pick
    with gzip.open(sys.argv[4], "wt", encoding="utf-8", compresslevel=9) as f:
        json.dump(all_cases, f, ensure_ascii=False, separators=(",", ":"))
    print(len(all_cases), "cases")


if __name__ == "__main__":
    main()

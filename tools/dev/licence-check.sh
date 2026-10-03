#!/bin/sh
# Checks the licences of every package in the workspace's dependency graph against the
# policy in deny.toml (docs/rust-port/REWRITE_PLAN.md §5). Python stdlib only.
#
#   tools/dev/licence-check.sh [-v]
#
# The graph is `cargo metadata --filter-platform x86_64-unknown-linux-gnu --all-features`
# (normal, build and dev dependencies). Each package's `license` field is evaluated as an SPDX
# expression: OR passes if any branch passes, AND needs every branch, `A WITH exception` counts
# as A, and the legacy `A/B` form means `A OR B`. Workspace members must be Apache-2.0. Packages
# named in [bans].deny fail whatever their licence. Exit status 1 on any failure.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
ws="$here/../.."
meta=$(mktemp)
trap 'rm -f "$meta"' EXIT
(cd "$ws" && cargo metadata --format-version 1 --locked --all-features \
	--filter-platform x86_64-unknown-linux-gnu) >"$meta"

python3 - "$ws/deny.toml" "$meta" "${1:-}" <<'EOF'
import collections
import json
import re
import sys
import tomllib

deny_path, meta_path, verbose = sys.argv[1], sys.argv[2], sys.argv[3] == "-v"
policy = tomllib.load(open(deny_path, "rb"))
allow = set(policy["licenses"]["allow"])
exceptions = {e.get("crate", e.get("name")): set(e["allow"])
              for e in policy["licenses"].get("exceptions", [])}
banned = {b["name"] if isinstance(b, dict) else b for b in policy.get("bans", {}).get("deny", [])}
meta = json.load(open(meta_path))


def tokens(expr):
    expr = re.sub(r"\s*/\s*", " OR ", expr)  # legacy "MIT/Apache-2.0"
    return re.findall(r"\(|\)|[^\s()]+", expr)


def evaluate(expr, allowed):
    """True if the SPDX expression can be satisfied with licences from `allowed`."""
    toks = tokens(expr)
    pos = 0

    def peek():
        return toks[pos] if pos < len(toks) else None

    def take():
        nonlocal pos
        pos += 1
        return toks[pos - 1]

    def factor():
        t = take()
        if t == "(":
            v = disjunction()
            if take() != ")":
                raise ValueError("unbalanced parentheses")
            return v
        if t in ("AND", "OR", "WITH", ")"):
            raise ValueError(f"unexpected {t}")
        lic = t.rstrip("+")
        if peek() == "WITH":
            take()
            take()  # the exception only adds permissions
        return lic in allowed

    def conjunction():
        v = factor()
        while peek() == "AND":
            take()
            v = factor() and v
        return v

    def disjunction():
        v = conjunction()
        while peek() == "OR":
            take()
            v = conjunction() or v
        return v

    v = disjunction()
    if pos != len(toks):
        raise ValueError(f"trailing {toks[pos:]}")
    return v


members = set(meta["workspace_members"])
in_graph = {n["id"] for n in meta["resolve"]["nodes"]}
failures, seen = [], collections.Counter()
for p in sorted(meta["packages"], key=lambda p: (p["name"], p["version"])):
    if p["id"] not in in_graph:
        continue
    name, lic = p["name"], p.get("license")
    where = f'{name} {p["version"]}'
    if name in banned:
        failures.append(f"{where}: banned crate")
        continue
    if p["id"] in members:
        if lic != "Apache-2.0":
            failures.append(f"{where}: workspace member must be Apache-2.0, is {lic!r}")
        continue
    if lic is None:
        if name in exceptions:
            seen[" AND ".join(sorted(exceptions[name])) + " (exception)"] += 1
            continue
        failures.append(f"{where}: no SPDX license field (license-file {p.get('license_file')!r})")
        continue
    try:
        ok = evaluate(lic, allow | exceptions.get(name, set()))
    except (ValueError, IndexError) as e:
        failures.append(f"{where}: cannot parse {lic!r}: {e}")
        continue
    seen[lic] += 1
    if not ok:
        failures.append(f"{where}: {lic} is not allowed")

total = sum(seen.values())
if verbose:
    for lic, n in sorted(seen.items(), key=lambda x: (-x[1], x[0])):
        print(f"{n:5}  {lic}")
print(f"licence-check: {total} third-party packages, {len(members)} workspace members, "
      f"{len(failures)} failures")
for f in failures:
    print("  FAIL", f)
sys.exit(1 if failures else 0)
EOF

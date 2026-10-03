# Ratchet changes (`tools/dev/changes/<task-id>.md`)

The acceptance comparison (`tools/dev/compare.sh`, `tools/dev/structdiff.py`;
docs/rust-port/REWRITE_PLAN.md §7.2) compares the Rust build of a site with the golden data of the
Go build (`testdata/golden/`, frozen at `44529028`), file by file and level by level, and
checks the result against the site's **baseline** (`testdata/baselines/<label>.json`). A task
may change the baseline only by listing every change in its own changes file,
`tools/dev/changes/<task-id>.md`, with exactly one triage class and a one-line reason. **An
unlisted new or changed difference fails the run.**

```sh
tools/dev/compare.sh docs-reduced --task T66            # compare; fails on unlisted changes
tools/dev/compare.sh docs-reduced --task T66 --update   # also write the baseline (listed changes)
tools/dev/compare.sh docs-reduced --report-only         # the numbers, never fails
tools/dev/structdiff.py changes                      # validate every changes file
```

## Entries

One Markdown list item per entry; every other line (headings, prose) is free text:

```markdown
# T66: docs parity

- docs-reduced L3 `tags/foo/index.html` accepted-deviation: the collision winner is the later term (D5)
- docs-reduced L3,L2 `blog/*/index.html` engine-difference: dates of undated pages are empty, not Go's year 1
- docs-reduced S `record en /about page html` bug-fixed: the `seo` type's layout was not looked up
```

`- <site> <levels> `<key>` <class>: <reason>`

- `<site>`: the label (`testsite`, `docs-i01`, `docs-reduced`, `docs-live`).
- `<levels>`: one or more of `L1`, `L2`, `L3`, `L4`, `S`, comma-separated.
- `<key>`: what changed, in backticks: a file (its path below `publishDir` after the L1
  normalisation, e.g. `images/x_hu_H.jpg`; `project:<name>` for a project file, the stats file
  under the Go build's name, `GO_STATS_KEY` of `manifest.py`, which `project:*` matches; `dir/**`
  for a collapsed collision directory at L1) or a structure fact (`record <lang> <path> <kind> <format>`,
  `alias <file> <lang> <path> <format>`, `pager <file> <lang> <path> <format>`,
  `resource <lang> <path> <name>`, `page <lang> <path> <kind>`), exactly as the report and
  `structdiff.json` print them. Shell-style wildcards (`*`, `?`, `[…]`; `*` also matches `/`)
  list many keys at once.
- `<class>`, exactly one of:
  - `engine-difference`: the engines differ by design (a D5 decision, Tera vs Go templates,
    another minifier or highlighter);
  - `bug-fixed`: a difference is gone (the only class that may mark an improvement as such; a
    new difference is never `bug-fixed`);
  - `accepted-deviation`: a difference the gate accepts (REWRITE_PLAN.md §7.3's allowed
    differences, or one reviewed and accepted in the task).
- `<reason>`: one line.

A malformed entry (a list item that starts like one but does not parse, or has another class)
is an error. An entry that matches no change is reported as unused.

## The baseline

`testdata/baselines/<label>.json` (schema `ssg-baseline/1`; gzipped when over 256 KiB):
per file (`files`) and per structure fact (`structure`), per level, the status of the last
accepted run: `"ok"`, or `{"status": "diff" | "missing" | "extra", "fp": "<diff fingerprint>",
"class": "…", "task": "…", "reason": "…"}` with the class, task and reason of the entry that
accepted it. The fingerprint (12 hex digits) hashes both sides' normalised values of that file
and level, so the same difference keeps its fingerprint and any change to it is a new one.

A run compares every (key, level) with the baseline:

| Baseline → now | Listed in the task's changes file | Not listed |
|---|---|---|
| ok/none → ok, or ok → gone | – (written by `--update`) | – (written by `--update`) |
| ok/none → difference, or difference → another difference | written by `--update` | **fails** |
| difference → ok or gone | written by `--update` (`bug-fixed`) | reported as improved; the baseline keeps the old entry |

Without a baseline file every difference is new. `--update` needs `--task`; it writes the
listed changes (and new or vanished `ok` keys) and keeps every unlisted entry as it was.
Baselines are reviewed at every phase end (§7.2).

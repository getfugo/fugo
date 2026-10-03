# Archive: the byte-for-byte port

Documents of the first Rust port of the Go version, which aimed at byte-identical output with
line-by-line ports of Go packages (`crates/`, deleted in T00 of
[`REWRITE_PLAN.md`](../REWRITE_PLAN.md); recoverable at commit `be02933a`, local tag
`go-parity-final`). They are kept for history only: **nothing here describes the current code**,
and their byte-parity rules, crate names and paths are obsolete. The current state is
[`../HANDOFF.md`](../HANDOFF.md).

| File | What it was |
|---|---|
| `HANDOFF-old-port.md` | The first session's handoff (2026-09-27): the byte-parity acceptance test, the old port's crate status, the darwin/arm64 FMA caveats |
| `TERA_PLAN.md` | The plan to switch the old port to Tera templates, superseded by `REWRITE_PLAN.md` |
| `LAYER_CRITIQUE.md` | A review of the old port's site-layer design (its layer design document in `crates/`) |

The research specs of that port stay in [`../specs/`](../specs/): tests and the Go oracles cite
them, and `specs/architecture-core-data/` is oracle input. Each carries a "byte-parity sections
obsolete" banner.

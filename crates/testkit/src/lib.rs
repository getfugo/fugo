//! Dev-only test support for the workspace (docs/rust-port/REWRITE_PLAN.md §2.1, §7.1).
//!
//! - [`fixture`]: reads the plain-JSON Go-oracle fixtures under `testdata/` (`.json`,
//!   `.jsonl`, either optionally gzipped) and decodes the `$nh:` tagged leaves of the fixture
//!   schema (see `tools/dev/fixtures2json.py`).
//! - [`txtar`]: Go's txtar archive format, used for small test sites.
//! - [`snapshot`]: the shared insta settings.
//! - [`contract`]: the template contract (REWRITE_PLAN.md §4.8): converted templates load against
//!   `ssg_funcs::spec::FUNCS` and call only declared kwargs.
//! - [`registry`]: a local npm registry (package documents and tarballs over HTTP), for the
//!   package installer's tests.
//! - [`tera_value`]: a JSON value as templates see it.

#![forbid(unsafe_code)]

pub mod contract;
pub mod fixture;
pub mod registry;
pub mod snapshot;
pub mod txtar;

/// A JSON value as a template value: numbers as numbers and maps with sorted keys, as this port
/// gives templates data. (Serializing a `serde_json::Value` into Tera does neither once
/// serde_json's `arbitrary_precision` and `preserve_order` are on, which rolldown turns on.)
#[must_use]
pub fn tera_value(json: &serde_json::Value) -> tera::Value {
    ssg_base::Value::from_json(json.clone()).to_tera()
}

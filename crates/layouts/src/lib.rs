//! Layout templates (REWRITE_PLAN.md §4.1, §4.3, §4.5): the scan of v0.146 layout names, the Go
//! implementation's lookup scorer, base template resolution, escaping by output format and
//! loading into one Tera instance.
//!
//! - [`LayoutStore::scan`] reads the layouts component of the [`Vfs`](ssg_vfs::Vfs) (the
//!   project, then the themes) plus the embedded templates, classifies every file by its
//!   v0.146 name ([`TemplateRole`]) and refuses legacy names (`_default/`, `partials/`,
//!   `shortcodes/`, `taxonomy/list`, `term/term`, `X-baseof`, `index`) with the name to use, and
//!   files with Go-template syntax.
//! - [`LayoutStore::select`] is Go's scorer over the templates from the root down to a
//!   page's path, followed by base resolution for layouts that begin with
//!   `{% extends "baseof.html" %}`; [`LayoutStore::shortcode`], [`LayoutStore::hook`] and
//!   [`LayoutStore::partial`] look up the other roles.
//! - [`load`] builds the Tera instance: fallback prefixes (`_theme<N>/`, `_embedded/`), the
//!   caller's registrations, then one `add_raw_templates` call with every template, the
//!   escaping aliases and the base variants (`<layout>@@<base>`) the selections need.
//!
//! Tera names: a user template is its path in the layouts component (lower case), a theme's is
//! prefixed with `_theme<N>/`, an embedded one's with `_embedded/`; user templates win over
//! theme and embedded ones of the same path because Tera tries the unprefixed name first.

#![forbid(unsafe_code)]

mod classify;
pub mod embedded;
mod env;
mod error;
mod load;
mod lookup;
mod name;
mod score;
mod source;
mod store;

pub use env::{EmbeddedHooks, HookUse, LayoutEnv};
pub use error::{IssueKind, LayoutIssue, TemplateError};
pub use load::{Selections, Templates, load};
pub use lookup::{HookQuery, LayoutQuery, Selection, ShortcodeMiss, ShortcodeQuery};
pub use name::{HookKind, Origin, StandaloneKind, TemplateName, TemplateRole};
pub use score::Score;
pub use source::go_marker;
pub use store::{LayoutSource, LayoutStore, TemplateInfo};

/// The Tera fallback prefix of the embedded templates (the same as
/// `ssg_funcs::spec::EMBEDDED_PREFIX`).
pub const EMBEDDED_PREFIX: &str = "_embedded/";

/// The name suffixes Tera autoescapes (§4.5); formats decide through alias names.
pub const AUTOESCAPE_SUFFIXES: [&str; 4] = [".html", ".htm", ".xml", ".svg"];

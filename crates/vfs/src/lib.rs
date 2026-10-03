//! Mounts → one union file view per component, walkers, ignore rules and the [`PathParser`]
//! (file → language, output format, bundle kind, content key).
//!
//! - [`Vfs::new`] turns the configuration into [`Mount`]s (phase A2): `[[module.mounts]]`, the
//!   default mount of each unconfigured component, the JS config files and the themes.
//! - [`Vfs::walk`] and [`Vfs::open`] give the union view of a component: the project's mounts
//!   before the themes', the first mount holding a path wins (content: per language; data and
//!   i18n keep every file; static: the last mount of the first module, symbolic links
//!   followed).
//! - [`PathParser::parse`] gives a file's [`PathInfo`].
//! - [`Vfs::discover_content`] is phase A3: the content files with their path info, leaf
//!   bundles resolved and duplicates settled.

#![forbid(unsafe_code)]

mod component;
mod discover;
mod error;
mod filter;
mod mount;
mod parser;
mod vfs;

pub use component::Component;
pub use discover::{ContentFile, Discovery, Duplicate};
pub use error::VfsError;
pub use mount::{Module, Mount};
pub use parser::{
    BundleKind, FormatSpec, LayoutParts, LayoutRole, Original, Parsed, PathInfo, PathParser,
    PathParserSpec,
};
pub use vfs::{FileRef, NFC_NAMES, Vfs, entry_name};

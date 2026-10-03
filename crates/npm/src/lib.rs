//! The project's npm packages, without Node.js or npm: [`ensure_installed`] installs the
//! dependencies of `package.json` into `node_modules` before a build, for `js_build` and Sass
//! imports and for mounts of their files.
//!
//! The installer is Deno's, as a library (`deno_npm_installer`): registry resolution,
//! `.npmrc`, tarballs checked against their integrity, an npm-style hoisted `node_modules`, the
//! platform's optional packages. Lifecycle scripts (`postinstall`) never run.
//!
//! **Who owns `node_modules`.** The installer manages a `node_modules` it created (it holds
//! Deno's `.deno` directory) or one that does not exist yet. One that npm, pnpm or yarn wrote
//! (their state files), or a link, is the user's: it is left alone. A stamp of what was
//! installed (the hash of `package.json` and the lock file) skips the work when nothing changed.
//!
//! **The lock file** is [`LOCK_FILE`] next to `package.json`, in Deno's lockfile format. When it
//! does not exist, a `package-lock.json` seeds it, so a project moving from npm keeps its
//! versions.

#![forbid(unsafe_code)]

mod http;
mod install;

pub use install::{InstallError, Installed, LOCK_FILE, ensure_installed};

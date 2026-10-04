//! `cargo metadata --format-version 1` of the workspace, as serde types of the fields the licence
//! check and the notices read. (The `cargo_metadata` crate turns on serde_json's
//! `unbounded_depth`, which the rest of the workspace does not have: `cargo dev` would build a
//! second serde_json and everything above it.)

use std::path::PathBuf;

use serde::Deserialize;

use crate::{Fail, fail};

#[derive(Debug, Deserialize)]
pub struct Metadata {
    pub packages: Vec<Package>,
    /// Package ids.
    pub workspace_members: Vec<String>,
    pub resolve: Resolve,
}

#[derive(Debug, Deserialize)]
pub struct Package {
    pub id: String,
    pub name: String,
    #[serde(deserialize_with = "version")]
    pub version: semver::Version,
    /// An SPDX expression.
    pub license: Option<String>,
    pub license_file: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    pub repository: Option<String>,
    pub manifest_path: PathBuf,
    /// The native library a `-sys` package links.
    pub links: Option<String>,
    pub targets: Vec<Target>,
}

#[derive(Debug, Deserialize)]
pub struct Target {
    pub name: String,
    /// `lib`, `bin`, `proc-macro` …
    pub kind: Vec<String>,
}

/// The resolved dependency graph.
#[derive(Debug, Deserialize)]
pub struct Resolve {
    pub nodes: Vec<Node>,
}

#[derive(Debug, Deserialize)]
pub struct Node {
    pub id: String,
    pub deps: Vec<NodeDep>,
}

#[derive(Debug, Deserialize)]
pub struct NodeDep {
    /// The dependency's package id.
    pub pkg: String,
    pub dep_kinds: Vec<DepKind>,
}

#[derive(Debug, Deserialize)]
pub struct DepKind {
    /// `dev` or `build`; none for a normal dependency.
    pub kind: Option<String>,
}

fn version<'de, D: serde::Deserializer<'de>>(d: D) -> Result<semver::Version, D::Error> {
    let s = String::deserialize(d)?;
    semver::Version::parse(&s).map_err(serde::de::Error::custom)
}

impl Package {
    /// The order of listings: by name, then by version.
    #[must_use]
    pub fn key(&self) -> (&str, &semver::Version) {
        (&self.name, &self.version)
    }

    #[must_use]
    pub fn has_target(&self, kind: &str) -> bool {
        self.targets
            .iter()
            .any(|t| t.kind.iter().any(|k| k == kind))
    }
}

impl Metadata {
    /// `cargo metadata` of the workspace (`--locked`) with `options`.
    ///
    /// # Errors
    /// cargo fails or prints something else.
    pub fn read(options: &[&str]) -> Result<Metadata, Fail> {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let out = std::process::Command::new(cargo)
            .args(["metadata", "--format-version", "1", "--locked"])
            .args(options)
            .current_dir(crate::root())
            .stderr(std::process::Stdio::inherit())
            .output()
            .map_err(|e| fail!("cargo metadata: {e}"))?;
        if !out.status.success() {
            return Err(fail!("cargo metadata failed ({})", out.status));
        }
        serde_json::from_slice(&out.stdout).map_err(|e| fail!("cargo metadata: {e}"))
    }

    /// The workspace member named `name`.
    #[must_use]
    pub fn member(&self, name: &str) -> Option<&Package> {
        self.packages
            .iter()
            .find(|p| p.name == name && self.workspace_members.contains(&p.id))
    }
}

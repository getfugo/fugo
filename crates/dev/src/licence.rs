//! The licence check (docs/rust-port/REWRITE_PLAN.md §5): the licences of every package in the
//! workspace's dependency graph against the policy in deny.toml.
//!
//! The graph is `cargo metadata --filter-platform x86_64-unknown-linux-gnu --all-features`
//! (normal, build and dev dependencies). Each package's `license` field is an SPDX expression
//! (the `spdx` crate, lax: the legacy `A/B` form means `A OR B`): OR passes if any branch
//! passes, AND needs every branch, `A WITH exception` counts as A and `A+` as A. Workspace
//! members must be Apache-2.0. Packages named in `[bans].deny` fail whatever their licence.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::Deserialize;

use crate::metadata::Metadata;
use crate::{Fail, fail};

/// deny.toml, as far as the check reads it.
#[derive(Deserialize)]
struct Policy {
    licenses: Licenses,
    #[serde(default)]
    bans: Bans,
}

#[derive(Deserialize)]
struct Licenses {
    allow: Vec<String>,
    /// Per-crate additions to the allowlist.
    #[serde(default)]
    exceptions: Vec<Exception>,
}

#[derive(Deserialize)]
struct Exception {
    #[serde(rename = "crate", alias = "name")]
    name: String,
    allow: Vec<String>,
}

#[derive(Default, Deserialize)]
struct Bans {
    #[serde(default)]
    deny: Vec<Ban>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Ban {
    Name(String),
    Entry { name: String },
}

impl Ban {
    fn name(&self) -> &str {
        match self {
            Ban::Name(n) | Ban::Entry { name: n } => n,
        }
    }
}

/// Whether the SPDX expression can be satisfied with licences from `allowed`.
///
/// # Errors
/// An expression that does not parse.
pub fn evaluate(expr: &str, allowed: &HashSet<String>) -> Result<bool, String> {
    let expr =
        spdx::Expression::parse_mode(expr, spdx::ParseMode::LAX).map_err(|e| e.to_string())?;
    Ok(expr.evaluate(|req| match &req.license {
        spdx::LicenseItem::Spdx { id, .. } => allowed.contains(id.name),
        spdx::LicenseItem::Other(_) => false,
    }))
}

/// Runs the check, printing a summary (with `verbose`, the count per licence expression) and
/// the failures; the exit status.
///
/// # Errors
/// An unreadable deny.toml or failing `cargo metadata`.
pub fn run(verbose: bool) -> Result<i32, Fail> {
    let deny_path = crate::root().join("deny.toml");
    let text =
        std::fs::read_to_string(&deny_path).map_err(|e| fail!("{}: {e}", deny_path.display()))?;
    let policy: Policy =
        toml::from_str(&text).map_err(|e| fail!("{}: {e}", deny_path.display()))?;
    let allow: HashSet<String> = policy.licenses.allow.iter().cloned().collect();
    let exceptions: HashMap<&str, &[String]> = policy
        .licenses
        .exceptions
        .iter()
        .map(|e| (e.name.as_str(), e.allow.as_slice()))
        .collect();
    let banned: HashSet<&str> = policy.bans.deny.iter().map(Ban::name).collect();

    let meta = Metadata::read(&[
        "--all-features",
        "--filter-platform",
        "x86_64-unknown-linux-gnu",
    ])?;
    let members: HashSet<_> = meta.workspace_members.iter().collect();
    let in_graph: HashSet<_> = meta.resolve.nodes.iter().map(|n| &n.id).collect();
    let mut packages: Vec<_> = meta
        .packages
        .iter()
        .filter(|p| in_graph.contains(&p.id))
        .collect();
    packages.sort_by(|a, b| a.key().cmp(&b.key()));
    let mut failures = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for p in packages {
        let at = format!("{} {}", p.name, p.version);
        let licence = p.license.as_deref();
        if banned.contains(p.name.as_str()) {
            failures.push(format!("{at}: banned crate"));
        } else if members.contains(&p.id) {
            if licence != Some("Apache-2.0") {
                failures.push(format!(
                    "{at}: workspace member must be Apache-2.0, is {licence:?}"
                ));
            }
        } else if let Some(licence) = licence {
            let mut allowed = allow.clone();
            allowed.extend(
                exceptions
                    .get(p.name.as_str())
                    .copied()
                    .unwrap_or_default()
                    .iter()
                    .cloned(),
            );
            match evaluate(licence, &allowed) {
                Err(e) => failures.push(format!("{at}: cannot parse {licence:?}: {e}")),
                Ok(ok) => {
                    *seen.entry(licence.to_owned()).or_default() += 1;
                    if !ok {
                        failures.push(format!("{at}: {licence} is not allowed"));
                    }
                }
            }
        } else if let Some(allowed) = exceptions.get(p.name.as_str()) {
            let names: BTreeSet<&str> = allowed.iter().map(String::as_str).collect();
            *seen
                .entry(format!(
                    "{} (exception)",
                    names.into_iter().collect::<Vec<_>>().join(" AND ")
                ))
                .or_default() += 1;
        } else {
            failures.push(format!(
                "{at}: no SPDX license field (license-file {:?})",
                p.license_file
            ));
        }
    }
    let total: usize = seen.values().sum();
    if verbose {
        let mut counts: Vec<(&String, &usize)> = seen.iter().collect();
        counts.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (licence, n) in counts {
            println!("{n:5}  {licence}");
        }
    }
    println!(
        "licence-check: {total} third-party packages, {} workspace members, {} failures",
        members.len(),
        failures.len()
    );
    for f in &failures {
        println!("  FAIL {f}");
    }
    Ok(i32::from(!failures.is_empty()))
}

//! The licence check (docs/rust-port/REWRITE_PLAN.md §5): the licences of every package in the
//! workspace's dependency graph against the policy in deny.toml.
//!
//! The graph is `cargo metadata --filter-platform x86_64-unknown-linux-gnu --all-features`
//! (normal, build and dev dependencies). Each package's `license` field is evaluated as an SPDX
//! expression: OR passes if any branch passes, AND needs every branch, `A WITH exception`
//! counts as A, and the legacy `A/B` form means `A OR B`. Workspace members must be Apache-2.0.
//! Packages named in `[bans].deny` fail whatever their licence.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::Value;

use crate::{Fail, fail};

/// The tokens of an SPDX expression (`/` read as `OR`).
fn tokens(expr: &str) -> Vec<String> {
    let mut s = String::new();
    let mut rest = expr;
    while let Some(i) = rest.find('/') {
        s.push_str(rest[..i].trim_end_matches(char::is_whitespace));
        s.push_str(" OR ");
        rest = rest[i + 1..].trim_start_matches(char::is_whitespace);
    }
    s.push_str(rest);
    let mut out = Vec::new();
    let mut word = String::new();
    for c in s.chars() {
        if c == '(' || c == ')' || c.is_whitespace() {
            if !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
            if !c.is_whitespace() {
                out.push(c.to_string());
            }
        } else {
            word.push(c);
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

struct Expr<'a> {
    toks: Vec<String>,
    pos: usize,
    allowed: &'a HashSet<String>,
}

impl Expr<'_> {
    fn peek(&self) -> Option<&str> {
        self.toks.get(self.pos).map(String::as_str)
    }

    fn take(&mut self) -> Result<String, String> {
        let t = self
            .toks
            .get(self.pos)
            .cloned()
            .ok_or_else(|| "list index out of range".to_owned())?;
        self.pos += 1;
        Ok(t)
    }

    fn factor(&mut self) -> Result<bool, String> {
        let t = self.take()?;
        if t == "(" {
            let v = self.disjunction()?;
            if self.take()? != ")" {
                return Err("unbalanced parentheses".into());
            }
            return Ok(v);
        }
        if matches!(t.as_str(), "AND" | "OR" | "WITH" | ")") {
            return Err(format!("unexpected {t}"));
        }
        let licence = t.trim_end_matches('+').to_owned();
        if self.peek() == Some("WITH") {
            self.take()?;
            self.take()?; // the exception only adds permissions
        }
        Ok(self.allowed.contains(&licence))
    }

    fn conjunction(&mut self) -> Result<bool, String> {
        let mut v = self.factor()?;
        while self.peek() == Some("AND") {
            self.take()?;
            v = self.factor()? && v;
        }
        Ok(v)
    }

    fn disjunction(&mut self) -> Result<bool, String> {
        let mut v = self.conjunction()?;
        while self.peek() == Some("OR") {
            self.take()?;
            v = self.conjunction()? || v;
        }
        Ok(v)
    }
}

/// Whether the SPDX expression can be satisfied with licences from `allowed`.
///
/// # Errors
/// An expression that does not parse.
pub fn evaluate(expr: &str, allowed: &HashSet<String>) -> Result<bool, String> {
    let mut e = Expr {
        toks: tokens(expr),
        pos: 0,
        allowed,
    };
    let v = e.disjunction()?;
    if e.pos != e.toks.len() {
        let rest: Vec<String> = e.toks[e.pos..]
            .iter()
            .map(|t| crate::py::repr_str(t))
            .collect();
        return Err(format!("trailing [{}]", rest.join(", ")));
    }
    Ok(v)
}

/// `cargo metadata` of the workspace.
///
/// # Errors
/// cargo fails or prints no JSON.
pub fn cargo_metadata(args: &[&str]) -> Result<Value, Fail> {
    let out =
        std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["metadata", "--format-version", "1", "--locked"])
            .args(args)
            .current_dir(crate::root())
            .stderr(std::process::Stdio::inherit())
            .output()
            .map_err(|e| fail!("cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(fail!("cargo metadata failed ({})", out.status));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| fail!("cargo metadata: {e}"))
}

fn strings(v: &Value) -> impl Iterator<Item = String> + '_ {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
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
    let policy: toml::Table =
        toml::from_str(&text).map_err(|e| fail!("{}: {e}", deny_path.display()))?;
    let licenses = policy.get("licenses").and_then(toml::Value::as_table);
    let allow: HashSet<String> = licenses
        .and_then(|l| l.get("allow"))
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    let mut exceptions: HashMap<String, HashSet<String>> = HashMap::new();
    for e in licenses
        .and_then(|l| l.get("exceptions"))
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = e
            .get("crate")
            .or_else(|| e.get("name"))
            .and_then(toml::Value::as_str)
            .unwrap_or("")
            .to_owned();
        let allowed = e
            .get("allow")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_owned));
        exceptions.insert(name, allowed.collect());
    }
    let banned: HashSet<String> = policy
        .get("bans")
        .and_then(|b| b.get("deny"))
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|b| {
            b.get("name")
                .and_then(toml::Value::as_str)
                .or_else(|| b.as_str())
                .map(str::to_owned)
        })
        .collect();

    let meta = cargo_metadata(&[
        "--all-features",
        "--filter-platform",
        "x86_64-unknown-linux-gnu",
    ])?;
    let members: HashSet<String> = strings(&meta["workspace_members"]).collect();
    let in_graph: HashSet<&str> = meta["resolve"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| n["id"].as_str())
        .collect();
    let mut packages: Vec<&Value> = meta["packages"].as_array().into_iter().flatten().collect();
    let key = |p: &&Value| {
        (
            p["name"].as_str().unwrap_or("").to_owned(),
            p["version"].as_str().unwrap_or("").to_owned(),
        )
    };
    packages.sort_by_key(key);
    let mut failures = Vec::new();
    let mut seen: BTreeMap<String, i64> = BTreeMap::new();
    for p in packages {
        let id = p["id"].as_str().unwrap_or("");
        if !in_graph.contains(id) {
            continue;
        }
        let (name, version) = key(&p);
        let licence = p["license"].as_str();
        let at = format!("{name} {version}");
        if banned.contains(&name) {
            failures.push(format!("{at}: banned crate"));
            continue;
        }
        if members.contains(id) {
            if licence != Some("Apache-2.0") {
                let shown = licence.map_or_else(|| "None".to_owned(), crate::py::repr_str);
                failures.push(format!(
                    "{at}: workspace member must be Apache-2.0, is {shown}"
                ));
            }
            continue;
        }
        let Some(licence) = licence else {
            if let Some(allowed) = exceptions.get(&name) {
                let mut names: Vec<&String> = allowed.iter().collect();
                names.sort();
                let names: Vec<&str> = names.into_iter().map(String::as_str).collect();
                *seen
                    .entry(format!("{} (exception)", names.join(" AND ")))
                    .or_default() += 1;
                continue;
            }
            let file = p["license_file"]
                .as_str()
                .map_or_else(|| "None".to_owned(), crate::py::repr_str);
            failures.push(format!("{at}: no SPDX license field (license-file {file})"));
            continue;
        };
        let mut allowed = allow.clone();
        allowed.extend(exceptions.get(&name).into_iter().flatten().cloned());
        match evaluate(licence, &allowed) {
            Err(e) => failures.push(format!(
                "{at}: cannot parse {}: {e}",
                crate::py::repr_str(licence)
            )),
            Ok(ok) => {
                *seen.entry(licence.to_owned()).or_default() += 1;
                if !ok {
                    failures.push(format!("{at}: {licence} is not allowed"));
                }
            }
        }
    }
    let total: i64 = seen.values().sum();
    if verbose {
        let mut counts: Vec<(&String, &i64)> = seen.iter().collect();
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

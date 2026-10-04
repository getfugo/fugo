//! The changes files of the tasks (tools/dev/changes/<task>.md): their entries, parsed and
//! validated (`structdiff changes`).

use super::*;

/// tools/dev/changes, the changes files of the tasks.
#[must_use]
pub fn changes_dir() -> PathBuf {
    crate::root().join("tools/dev/changes")
}

/// An entry of a changes file: `- <site> <levels> `<key>` <class>: <reason>`.
#[derive(Clone, Debug)]
pub struct Change {
    pub task: String,
    pub site: String,
    pub levels: Vec<Level>,
    pub pattern: String,
    matcher: GlobMatcher,
    pub class: String,
    pub reason: String,
    /// `<file>:<line>`.
    pub at: String,
    pub used: usize,
}

impl Change {
    #[must_use]
    pub fn matches(&self, site: &str, level: Level, key: &str) -> bool {
        site == self.site && self.levels.contains(&level) && self.matcher.is_match(key)
    }
}

pub(super) fn entry_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^- (?P<site>[A-Za-z0-9_.-]+) (?P<levels>(?:L[1-4]|S)(?:,(?:L[1-4]|S))*) `(?P<key>[^`]+)` (?P<class>[a-z-]+): (?P<reason>\S.*)$",
        )
        .expect("a valid expression")
    })
}

pub(super) fn entry_start_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^- [A-Za-z0-9_.-]+ (?:L[1-4]|S)[ ,]").expect("a valid expression")
    })
}

/// The entries of one changes file, and its format errors.
///
/// # Errors
/// An unreadable file.
pub fn parse_changes(path: &Path, task: &str) -> Result<(Vec<Change>, Vec<String>), Fail> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let text = std::fs::read_to_string(path).map_err(|e| fail!("{}: {e}", path.display()))?;
    let (mut changes, mut errors) = (Vec::new(), Vec::new());
    for (i, line) in text.lines().enumerate() {
        let at = format!("{name}:{}", i + 1);
        let Some(m) = entry_re().captures(line) else {
            if entry_start_re().is_match(line) {
                errors.push(format!(
                    "{at}: not `- <site> <level>[,<level>] `<key>` <class>: <reason>`: {line}"
                ));
            }
            continue;
        };
        if !CLASSES.contains(&&m["class"]) {
            errors.push(format!(
                "{at}: triage class {:?} is not one of {}",
                &m["class"],
                CLASSES.join(", ")
            ));
            continue;
        }
        let matcher = match Glob::new(&m["key"]) {
            Ok(g) => g.compile_matcher(),
            Err(e) => {
                errors.push(format!("{at}: `{}`: {e}", &m["key"]));
                continue;
            }
        };
        changes.push(Change {
            task: task.to_owned(),
            site: m["site"].to_owned(),
            levels: m["levels"].split(',').filter_map(Level::parse).collect(),
            pattern: m["key"].to_owned(),
            matcher,
            class: m["class"].to_owned(),
            reason: m["reason"].trim().to_owned(),
            at,
            used: 0,
        });
    }
    Ok((changes, errors))
}

/// The entries of the changes files of `tasks`, and the errors.
///
/// # Errors
/// An unreadable file.
pub fn load_changes(dir: &Path, tasks: &[String]) -> Result<(Vec<Change>, Vec<String>), Fail> {
    let (mut changes, mut errors) = (Vec::new(), Vec::new());
    for task in tasks {
        let path = dir.join(format!("{task}.md"));
        if !path.is_file() {
            errors.push(format!(
                "{}: no changes file for task {task}",
                path.display()
            ));
            continue;
        }
        let (c, e) = parse_changes(&path, task)?;
        changes.extend(c);
        errors.extend(e);
    }
    Ok((changes, errors))
}

/// `structdiff changes`: validates every changes file; the exit status.
///
/// # Errors
/// An unreadable directory or file.
pub fn cmd_changes(dir: &Path) -> Result<i32, Fail> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| fail!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".md") && n != "README.md")
        .collect();
    names.sort();
    let (mut count, mut errors) = (0, Vec::new());
    for name in names {
        let (c, e) = parse_changes(&dir.join(&name), name.trim_end_matches(".md"))?;
        count += c.len();
        errors.extend(e);
    }
    for e in &errors {
        eprintln!("structdiff changes: {e}");
    }
    let tail = if errors.is_empty() {
        String::new()
    } else {
        format!(", {} errors", errors.len())
    };
    println!("{count} change entries in {}{tail}", dir.display());
    Ok(i32::from(!errors.is_empty()))
}

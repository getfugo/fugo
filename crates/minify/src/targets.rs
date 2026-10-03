//! Browser targets for CSS from the project's browserslist configuration, the one autoprefixer
//! reads: a `.browserslistrc` or `browserslist` file, or the `browserslist` key of `package.json`,
//! in the project directory. With targets, the CSS minifier adds the vendor prefixes and lowers the
//! syntax those browsers need (and drops prefixes none of them needs); without a configuration it
//! leaves prefixes as written.
//!
//! The queries of the section named like the build environment (`[production]`,
//! `[development]`) apply, else the default ones; `BROWSERSLIST` and `BROWSERSLIST_CONFIG` are
//! honoured as browserslist defines them.

use std::path::Path;

use lightningcss::targets::Browsers;

use crate::MinifyError;

/// The targets of the browserslist configuration in `project_dir` for `environment`, or `None`
/// when the project has none.
///
/// # Errors
/// [`MinifyError::Browserslist`] for a configuration browserslist rejects.
pub fn project_browsers(
    project_dir: &Path,
    environment: &str,
) -> Result<Option<Browsers>, MinifyError> {
    if !has_config(project_dir) {
        return Ok(None);
    }
    let opts = browserslist::Opts {
        path: Some(project_dir.to_string_lossy().into_owned()),
        env: Some(environment.to_owned()),
        ..browserslist::Opts::default()
    };
    let distribs = browserslist::execute(&opts)
        .map_err(|e| MinifyError::Browserslist(format!("{}: {e}", project_dir.display())))?;
    Ok(browsers(distribs.iter().map(|d| (d.name(), d.version()))))
}

/// Whether `dir` holds a browserslist configuration.
fn has_config(dir: &Path) -> bool {
    if dir.join(".browserslistrc").is_file() || dir.join("browserslist").is_file() {
        return true;
    }
    std::fs::read(dir.join("package.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .is_some_and(|pkg| pkg.get("browserslist").is_some())
}

/// lightningcss's targets for browserslist's `(name, version)` pairs: the oldest version of each
/// browser lightningcss knows; `None` when there is none.
fn browsers<'a>(distribs: impl Iterator<Item = (&'a str, &'a str)>) -> Option<Browsers> {
    let mut b = Browsers::default();
    let mut any = false;
    for (name, version) in distribs {
        let slot = match name {
            "android" => &mut b.android,
            "chrome" | "and_chr" => &mut b.chrome,
            "edge" => &mut b.edge,
            "firefox" | "and_ff" => &mut b.firefox,
            "ie" => &mut b.ie,
            "ios_saf" => &mut b.ios_saf,
            "opera" | "op_mob" => &mut b.opera,
            "safari" => &mut b.safari,
            "samsung" => &mut b.samsung,
            _ => continue,
        };
        let Some(v) = version_code(version) else {
            continue;
        };
        if slot.is_none_or(|old| v < old) {
            *slot = Some(v);
            any = true;
        }
    }
    any.then_some(b)
}

/// lightningcss's version number: `major << 16 | minor << 8 | patch`, of the first version of a
/// range (`15.2-15.3`). `None` for `TP`, `all` and other non-numeric versions.
fn version_code(version: &str) -> Option<u32> {
    let first = version.split('-').next()?;
    let mut parts = first.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next().map_or(Ok(0), str::parse).ok()?;
    let patch: u32 = parts.next().map_or(Ok(0), str::parse).ok()?;
    Some((major << 16) | (minor << 8) | patch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(version_code("12"), Some(12 << 16));
        assert_eq!(version_code("15.2-15.3"), Some((15 << 16) | (2 << 8)));
        assert_eq!(version_code("4.4.3"), Some((4 << 16) | (4 << 8) | 3));
        assert_eq!(version_code("TP"), None);
    }

    #[test]
    fn oldest_version_per_browser() {
        let b = browsers(
            [
                ("chrome", "120"),
                ("and_chr", "60"),
                ("safari", "12.1"),
                ("kaios", "3"),
            ]
            .into_iter(),
        )
        .expect("targets");
        assert_eq!(b.chrome, Some(60 << 16));
        assert_eq!(b.safari, Some((12 << 16) | (1 << 8)));
        assert_eq!(b.firefox, None);
        assert_eq!(browsers([("kaios", "3")].into_iter()), None);
    }

    #[test]
    fn configuration_files() {
        let dir = tempfile::tempdir().expect("tmp");
        assert_eq!(project_browsers(dir.path(), "production").unwrap(), None);

        std::fs::write(dir.path().join("package.json"), r#"{"name": "x"}"#).unwrap();
        assert_eq!(project_browsers(dir.path(), "production").unwrap(), None);

        std::fs::write(
            dir.path().join(".browserslistrc"),
            "Safari >= 12\n\n[development]\nlast 1 chrome version\n",
        )
        .unwrap();
        let prod = project_browsers(dir.path(), "production")
            .unwrap()
            .expect("targets");
        assert_eq!(prod.safari, Some(12 << 16));
        assert_eq!(prod.chrome, None);
        let dev = project_browsers(dir.path(), "development")
            .unwrap()
            .expect("targets");
        assert_eq!(dev.safari, None);
        assert!(dev.chrome.is_some());

        std::fs::write(dir.path().join(".browserslistrc"), "not a browser 7\n").unwrap();
        let err = project_browsers(dir.path(), "production").unwrap_err();
        assert!(matches!(err, MinifyError::Browserslist(_)), "{err}");
    }

    #[test]
    fn package_json_key() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"browserslist": ["Firefox >= 60"]}"#,
        )
        .unwrap();
        let b = project_browsers(dir.path(), "production")
            .unwrap()
            .expect("targets");
        assert_eq!(b.firefox, Some(60 << 16));
    }
}

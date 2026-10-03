//! The `npm` feature: the project's npm packages, installed before a build reads the project
//! ([`ssg_build::Prepare`]; `ssg-npm` says when it installs and when it leaves `node_modules`
//! alone).

use ssg_build::Prepare;
use ssg_config::Config;
use ssg_npm::Installed;

/// Installs the dependencies of the project's `package.json`, downloading into the cache
/// directory.
#[derive(Debug)]
pub(crate) struct Install {
    /// `--quiet`: no line when packages were installed.
    pub(crate) quiet: bool,
}

impl Prepare for Install {
    fn prepare(&self, cfg: &Config) -> Result<(), String> {
        match ssg_npm::ensure_installed(&cfg.project_dir, &cfg.cache_dir) {
            Ok(Installed::Installed { elapsed }) => {
                if !self.quiet {
                    println!(
                        "Installed the npm packages of package.json in {} ms",
                        elapsed.as_millis()
                    );
                }
                Ok(())
            }
            Ok(Installed::NoPackages | Installed::External(_) | Installed::UpToDate) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

//! [`ensure_installed`]: the dependencies of `package.json` into `node_modules`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use deno_config::deno_json::{NodeModulesDirMode, NodeModulesLinkerMode};
use deno_npm_cache::NpmCacheSetting;
use deno_npm_installer::graph::NpmCachingStrategy;
use deno_npm_installer::lifecycle_scripts::NullLifecycleScriptsExecutor;
use deno_npm_installer::{
    LifecycleScriptsConfig, LogReporter, NpmInstallerFactory, NpmInstallerFactoryOptions,
    PackageCaching,
};
use deno_resolver::factory::{
    ConfigDiscoveryOption, ResolverFactory, ResolverFactoryOptions, WorkspaceFactory,
    WorkspaceFactoryOptions,
};
use sha2::{Digest, Sha256};
use sys_traits::impls::RealSys;

use crate::http::Http;

/// The lock file written next to `package.json` (Deno's lockfile format).
pub const LOCK_FILE: &str = "npm.lock";

/// The record of what was installed, in Deno's directory of `node_modules`.
const STAMP: &str = ".deno/.install-stamp";

/// Part of the stamp: a new installer (or layout) installs again.
const INSTALLER: &str = "deno_npm_installer 0.54.0, hoisted";

/// The state files other package managers keep in the `node_modules` they write.
const OTHER_MANAGERS: [(&str, &str); 4] = [
    (".package-lock.json", "npm"),
    (".modules.yaml", "pnpm"),
    (".yarn-state.yml", "yarn"),
    (".yarn-integrity", "yarn"),
];

/// What [`ensure_installed`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Installed {
    /// No `package.json`, or one without dependencies.
    NoPackages,
    /// `node_modules` is someone else's (the package manager that wrote it, or "a link"): left
    /// alone.
    External(&'static str),
    /// The stamp says `package.json` and the lock file are installed.
    UpToDate,
    /// Installed now.
    Installed { elapsed: Duration },
}

/// Why the packages could not be installed.
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{}: {message}", path.display())]
    PackageJson { path: PathBuf, message: String },
    #[error("installing the npm packages of {}: {message}", path.display())]
    Install { path: PathBuf, message: String },
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> InstallError + '_ {
    move |source| InstallError::Io {
        path: path.to_owned(),
        source,
    }
}

/// Installs the dependencies, dev dependencies and optional dependencies of
/// `<project>/package.json` into `<project>/node_modules`, downloading into `<cache>/packages`,
/// unless they are installed already or another package manager owns `node_modules` (see the
/// crate docs). Writes [`LOCK_FILE`].
///
/// # Errors
/// A `package.json` that does not parse, a registry that cannot be reached, a version that
/// does not exist.
pub fn ensure_installed(project: &Path, cache: &Path) -> Result<Installed, InstallError> {
    let package_json = project.join("package.json");
    let text = match std::fs::read(&package_json) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Installed::NoPackages),
        Err(e) => return Err(io(&package_json)(e)),
    };
    if !has_dependencies(&package_json, &text)? {
        return Ok(Installed::NoPackages);
    }
    let node_modules = project.join("node_modules");
    if let Some(owner) = other_owner(&node_modules)? {
        return Ok(Installed::External(owner));
    }
    let stamp_path = node_modules.join(STAMP);
    if std::fs::read_to_string(&stamp_path).is_ok_and(|s| s == stamp(project, &text)) {
        return Ok(Installed::UpToDate);
    }
    let started = Instant::now();
    install(project, cache).map_err(|e| InstallError::Install {
        path: package_json.clone(),
        message: format!("{e:#}"),
    })?;
    std::fs::write(&stamp_path, stamp(project, &text)).map_err(io(&stamp_path))?;
    Ok(Installed::Installed {
        elapsed: started.elapsed(),
    })
}

/// Whether `package.json` declares packages to install.
fn has_dependencies(path: &Path, text: &[u8]) -> Result<bool, InstallError> {
    let json: serde_json::Value =
        serde_json::from_slice(text).map_err(|e| InstallError::PackageJson {
            path: path.to_owned(),
            message: e.to_string(),
        })?;
    Ok(["dependencies", "devDependencies", "optionalDependencies"]
        .iter()
        .any(|k| {
            json.get(k)
                .and_then(serde_json::Value::as_object)
                .is_some_and(|m| !m.is_empty())
        }))
}

/// Who else owns `node_modules`, if anyone: a link, another package manager's state file, or
/// a non-empty directory without Deno's `.deno`.
fn other_owner(node_modules: &Path) -> Result<Option<&'static str>, InstallError> {
    let meta = match std::fs::symlink_metadata(node_modules) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io(node_modules)(e)),
    };
    if meta.file_type().is_symlink() {
        return Ok(Some("a link"));
    }
    if let Some((_, manager)) = OTHER_MANAGERS
        .iter()
        .find(|(file, _)| node_modules.join(file).exists())
    {
        return Ok(Some(manager));
    }
    if node_modules.join(".deno").is_dir() {
        return Ok(None);
    }
    let empty = std::fs::read_dir(node_modules)
        .map_err(io(node_modules))?
        .next()
        .is_none();
    Ok((!empty).then_some("another package manager"))
}

/// The stamp of an installation: the installer, the platform (optional packages differ), the
/// bytes of `package.json` and of the lock file.
fn stamp(project: &Path, package_json: &[u8]) -> String {
    let lock = std::fs::read(project.join(LOCK_FILE)).unwrap_or_default();
    let mut h = Sha256::new();
    for part in [
        INSTALLER.as_bytes(),
        std::env::consts::OS.as_bytes(),
        std::env::consts::ARCH.as_bytes(),
        package_json,
        &lock,
    ] {
        h.update((part.len() as u64).to_le_bytes());
        h.update(part);
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Deno's installer on a current-thread runtime: resolution (seeded by the lock file, else by
/// `package-lock.json`), the tarballs into the cache, the hoisted `node_modules`, the lock file.
fn install(project: &Path, cache: &Path) -> anyhow::Result<()> {
    // Deno's workspace discovery caches package.json files per thread: a server installs
    // again on the thread that installed before, after package.json changed.
    node_resolver::PackageJsonThreadLocalCache::clear();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let workspace = Arc::new(WorkspaceFactory::new(
            RealSys,
            project.to_owned(),
            WorkspaceFactoryOptions {
                config_discovery: ConfigDiscoveryOption::Discover {
                    start_paths: vec![project.to_owned()],
                },
                // "manual" with a package manager command is a managed install into the
                // project's node_modules; the hoisted linker lays it out as npm does.
                node_modules_dir: Some(NodeModulesDirMode::Manual),
                node_modules_linker: Some(NodeModulesLinkerMode::Hoisted),
                is_package_manager_subcommand: true,
                lock_arg: Some(project.join(LOCK_FILE)),
                import_npm_lockfile: true,
                maybe_custom_deno_dir_root: Some(cache.join("packages")),
                ..WorkspaceFactoryOptions::default()
            },
        ));
        let resolver = Arc::new(ResolverFactory::new(
            workspace,
            ResolverFactoryOptions::default(),
        ));
        let factory = NpmInstallerFactory::new(
            resolver,
            Arc::new(Http::new()),
            Arc::new(NullLifecycleScriptsExecutor),
            LogReporter,
            None,
            NpmInstallerFactoryOptions {
                cache_setting: NpmCacheSetting::Use,
                caching_strategy: NpmCachingStrategy::Eager,
                clean_on_install: true,
                dedup_lockfile_peer_variants: true,
                lifecycle_scripts_config: LifecycleScriptsConfig {
                    initial_cwd: project.to_owned(),
                    root_dir: project.to_owned(),
                    explicit_install: true,
                    ..LifecycleScriptsConfig::default()
                },
                production: false,
                skip_types: false,
                resolve_npm_resolution_snapshot: Box::new(|| Ok(None)),
            },
        );
        let installer = factory.npm_installer().await?;
        installer.ensure_no_pkg_json_dep_errors()?;
        installer.ensure_top_level_package_json_install().await?;
        installer.cache_packages(PackageCaching::All).await?;
        if let Some(lock) = factory.maybe_lockfile().await? {
            lock.write_if_changed()?;
        }
        Ok(())
    })
}

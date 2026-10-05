//! `ssg-dev`: the repository's tools (`cargo dev <command>`).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ssg_dev::{
    Fail, bench, file_length, licence, manifest, notices, package, ratchet, selftest, sites,
    structdiff,
};

#[derive(Parser)]
#[command(
    name = "ssg-dev",
    about = "The repository's tools: test sites, build manifests, structdiff, licences, release archives and file lengths"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// The end-to-end sites of the acceptance harness, written outside the repository
    #[command(subcommand)]
    Sites(Sites),
    /// Manifests of a site build: one extractor for the Go and the Rust output alike
    #[command(subcommand)]
    Manifest(Manifest),
    /// Structural comparison of two builds of one site, and the ratchet
    #[command(subcommand)]
    Structdiff(Structdiff),
    /// structdiff's self-test: perturbations of a Go build's output must be classified exactly
    Selftest {
        /// A Go build's publish directory (default: Go's testsite output plus a Thai page and a PNG)
        #[arg(long)]
        go_out: Option<PathBuf>,
        /// The site directory of --go-out
        #[arg(long, requires = "go_out")]
        project: Option<PathBuf>,
        /// Keep the temporary directory
        #[arg(long)]
        keep: bool,
    },
    /// Checks the licences of the workspace's dependency graph against deny.toml
    LicenceCheck {
        /// Print the number of packages per licence expression
        #[arg(short)]
        v: bool,
    },
    /// Fails when a file of code is longer than 500 lines (AGENTS.md)
    FileLength,
    /// Measures a binary: build time and peak memory of the docs site and of generated sites
    Bench {
        /// The binary to measure
        binary: PathBuf,
        /// The results, as github-action-benchmark's custom JSON
        out: PathBuf,
        /// Measured builds per site (the median counts), after a warm-up build
        #[arg(long, default_value_t = 5)]
        runs: usize,
        /// The Go implementation's binary: also build the generated sites with it
        #[arg(long)]
        go: Option<PathBuf>,
    },
    /// Writes the licence notices of the packages linked into the binary for a target
    Notices {
        /// A Rust target triple
        target: String,
        out: PathBuf,
        /// Do not fail when a package ships no licence file and has no known licence text
        #[arg(long)]
        allow_missing: bool,
    },
    /// Packages a release build for one target: the archive and its .sha256
    Package {
        binary: PathBuf,
        /// A Rust target triple
        target: String,
        out_dir: PathBuf,
        /// The licence notices (THIRD_PARTY_NOTICES.txt in the archive)
        notices: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum Sites {
    /// Every site name
    List,
    /// Writes a site into <DIR>, which must not exist (for mini its name must be mini)
    Make {
        site: String,
        dir: PathBuf,
        /// The docs patch variant
        #[arg(long, value_parser = ["i01", "reduced", "live"])]
        docs_patches: Option<String>,
        /// The Tera overlay (sites/<site>)
        #[arg(long)]
        overlay: Option<PathBuf>,
    },
    /// The --cacheDir contents a site needs (its <site> directory)
    Cache { site: String, dir: PathBuf },
    /// Rewrites patches.json in its canonical form and checks it against the Tera patch files
    Patches {
        /// Only check
        #[arg(long)]
        check: bool,
    },
}

#[derive(Subcommand)]
enum Manifest {
    /// The manifest of every file below a publish directory
    Extract(Box<manifest::Extract>),
    /// One line about each manifest or structure dump
    Summary {
        #[arg(required = true)]
        manifests: Vec<PathBuf>,
    },
}

#[derive(Subcommand)]
enum Structdiff {
    /// Compares a reference with a candidate (manifest files or publish directories)
    Compare(Box<structdiff::CompareArgs>),
    /// Validates every changes file
    Changes {
        #[arg(long, default_value_os_t = ratchet::changes_dir())]
        changes: PathBuf,
    },
}

fn run(cli: Cli) -> Result<i32, Fail> {
    match cli.command {
        Command::Sites(Sites::List) => {
            for name in sites::list()? {
                println!("{name}");
            }
            Ok(0)
        }
        Command::Sites(Sites::Make {
            site,
            dir,
            docs_patches,
            overlay,
        }) => {
            sites::make(&site, &dir, docs_patches.as_deref(), overlay.as_deref())?;
            Ok(0)
        }
        Command::Sites(Sites::Cache { site, dir }) => {
            sites::cache(&site, &dir)?;
            Ok(0)
        }
        Command::Sites(Sites::Patches { check }) => sites::patches(check),
        Command::Manifest(Manifest::Extract(a)) => {
            manifest::run_extract(&a)?;
            Ok(0)
        }
        Command::Manifest(Manifest::Summary { manifests }) => {
            for m in manifests {
                println!("{}", manifest::summary(&m)?);
            }
            Ok(0)
        }
        Command::Structdiff(Structdiff::Compare(a)) => structdiff::cmd_compare(&a),
        Command::Structdiff(Structdiff::Changes { changes }) => ratchet::cmd_changes(&changes),
        Command::Selftest {
            go_out,
            project,
            keep,
        } => {
            let failures = selftest::run(go_out.as_deref(), project.as_deref(), keep)?;
            Ok(i32::from(failures > 0))
        }
        Command::LicenceCheck { v } => licence::run(v),
        Command::FileLength => file_length::run(),
        Command::Bench {
            binary,
            out,
            runs,
            go,
        } => bench::run(&binary, &out, runs, go.as_deref()),
        Command::Notices {
            target,
            out,
            allow_missing,
        } => notices::run(&target, &out, allow_missing),
        Command::Package {
            binary,
            target,
            out_dir,
            notices,
        } => {
            package::run(&binary, &target, &out_dir, notices.as_deref())?;
            Ok(0)
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(e) => {
            eprintln!("ssg-dev: {e}");
            ExitCode::FAILURE
        }
    }
}

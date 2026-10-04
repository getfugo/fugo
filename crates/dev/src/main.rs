//! `ssg-dev`: the repository's tools (`cargo dev <command>`).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ssg_dev::{Fail, licence, manifest, notices, package, selftest, sites, structdiff};

#[derive(Parser)]
#[command(
    name = "ssg-dev",
    about = "The repository's tools: test sites, build manifests, structdiff, licences and release archives"
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
    Extract {
        publish: PathBuf,
        /// The site directory (static/ and, without --base-url, the base URLs of config.toml)
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long = "base-url")]
        base_url: Vec<String>,
        #[arg(long, default_value = "L1,L2,L3,L4")]
        levels: String,
        #[arg(long, default_value = "")]
        site: String,
        #[arg(long = "pass", default_value = "")]
        pass: String,
        /// Record the visible text of every page
        #[arg(long)]
        full_text: bool,
        /// The output file (gzipped when it ends in .gz; default: stdout)
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// One line about each manifest or structure dump
    Summary {
        #[arg(required = true)]
        manifests: Vec<PathBuf>,
    },
}

#[derive(Subcommand)]
enum Structdiff {
    /// Compares a reference with a candidate (manifest files or publish directories)
    Compare(Box<CompareArgs>),
    /// Validates every changes file
    Changes {
        #[arg(long)]
        changes: Option<PathBuf>,
    },
}

#[derive(clap::Args)]
struct CompareArgs {
    #[arg(long)]
    site: String,
    #[arg(long)]
    ref_min: Option<PathBuf>,
    #[arg(long)]
    ref_unmin: Option<PathBuf>,
    #[arg(long)]
    ref_structure: Option<PathBuf>,
    #[arg(long)]
    ref_project: Option<PathBuf>,
    #[arg(long, default_value = "golden")]
    ref_name: String,
    #[arg(long)]
    cand_min: Option<PathBuf>,
    #[arg(long)]
    cand_unmin: Option<PathBuf>,
    #[arg(long)]
    cand_structure: Option<PathBuf>,
    #[arg(long)]
    cand_project: Option<PathBuf>,
    #[arg(long, default_value = "rust")]
    cand_name: String,
    /// A directory whose files are compared only by their presence at L1
    #[arg(long)]
    collision_dir: Vec<String>,
    /// Write the whole result as JSON
    #[arg(long)]
    json: Option<PathBuf>,
    /// Write the report
    #[arg(long)]
    report: Option<PathBuf>,
    /// The number of differences listed per level
    #[arg(long, default_value_t = 40)]
    show: usize,
    /// The ratchet's baseline (testdata/baselines/<site>.json)
    #[arg(long)]
    baseline: Option<PathBuf>,
    /// The task whose changes file lists the changes (tools/dev/changes/<task>.md)
    #[arg(long)]
    task: Vec<String>,
    #[arg(long)]
    changes: Option<PathBuf>,
    /// Write the baseline with the listed changes applied
    #[arg(long)]
    update: bool,
    /// Never fail
    #[arg(long)]
    report_only: bool,
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
        Command::Manifest(Manifest::Extract {
            publish,
            project,
            base_url,
            levels,
            site,
            pass,
            full_text,
            out,
        }) => {
            manifest::run_extract(&manifest::Extract {
                publish,
                project,
                base_urls: base_url,
                levels,
                site,
                pass,
                full_text,
                out,
            })?;
            Ok(0)
        }
        Command::Manifest(Manifest::Summary { manifests }) => {
            for m in manifests {
                println!("{}", manifest::summary(&m)?);
            }
            Ok(0)
        }
        Command::Structdiff(Structdiff::Compare(a)) => {
            structdiff::cmd_compare(&structdiff::CompareArgs {
                site: a.site,
                ref_min: a.ref_min,
                ref_unmin: a.ref_unmin,
                ref_structure: a.ref_structure,
                ref_project: a.ref_project,
                ref_name: a.ref_name,
                cand_min: a.cand_min,
                cand_unmin: a.cand_unmin,
                cand_structure: a.cand_structure,
                cand_project: a.cand_project,
                cand_name: a.cand_name,
                collision_dir: a.collision_dir,
                json: a.json,
                report: a.report,
                show: a.show,
                baseline: a.baseline,
                task: a.task,
                changes: a.changes.unwrap_or_else(structdiff::changes_dir),
                update: a.update,
                report_only: a.report_only,
            })
        }
        Command::Structdiff(Structdiff::Changes { changes }) => {
            structdiff::cmd_changes(&changes.unwrap_or_else(structdiff::changes_dir))
        }
        Command::Selftest {
            go_out,
            project,
            keep,
        } => {
            let failures = selftest::run(go_out.as_deref(), project.as_deref(), keep)?;
            Ok(i32::from(failures > 0))
        }
        Command::LicenceCheck { v } => licence::run(v),
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

//! `build`: flags → [`BuildRequest`] → [`ssg_build::build`] → the report.

use std::sync::Arc;
use std::time::Duration;

use ssg_build::{BuildReport, BuildRequest, Prepare, SinkKind};

use crate::Exit;
use crate::args::BuildArgs;
use crate::report;

/// The build request of the flags.
pub(crate) fn request(a: &BuildArgs) -> anyhow::Result<BuildRequest> {
    let p = &a.project;
    let mut cli = p.overrides();
    cli.minify = a.minify;
    cli.no_times = a.output.no_times;
    cli.no_chmod = a.output.no_chmod;
    Ok(BuildRequest {
        source: p.source_dir()?,
        destination: a.output.destination.clone(),
        config_files: p.config.clone(),
        cli,
        clock: p.clock,
        sink: if a.output.render_to_memory {
            SinkKind::Memory
        } else {
            SinkKind::Disk
        },
        clean_destination: a.output.clean_destination_dir,
        threads: a.threads,
        config: None,
        live_reload: None,
        server: false,
        prepare: prepare(a.quiet),
    })
}

/// The project's npm packages are installed before the build (`npm` feature).
#[cfg(feature = "npm")]
fn prepare(quiet: bool) -> Option<Arc<dyn Prepare>> {
    Some(Arc::new(crate::npm::Install { quiet }))
}

#[cfg(not(feature = "npm"))]
fn prepare(_quiet: bool) -> Option<Arc<dyn Prepare>> {
    None
}

pub(crate) fn run(a: &BuildArgs) -> anyhow::Result<Exit> {
    match ssg_build::build(request(a)?) {
        Ok(r) => {
            report::diagnostics(&r.diagnostics);
            if !a.quiet {
                print_summary(&r);
                let total: Duration = r.timings.iter().map(|(_, d)| *d).sum();
                println!("Total in {} ms", total.as_millis());
            }
            if std::env::var_os(ssg_base::env_var!("TIMINGS")).is_some() {
                print_timings(&r);
            }
            Ok(Exit::Success)
        }
        Err(e) => {
            report::build_failed(&e);
            Ok(Exit::Failure)
        }
    }
}

/// The build's phase timings on stderr (`FUGO_TIMINGS` set; for profiling, T70).
fn print_timings(r: &BuildReport) {
    let phases: Vec<String> = r
        .timings
        .iter()
        .map(|(name, d)| format!("{name} {} ms", d.as_millis()))
        .collect();
    eprintln!("timings: {}", phases.join(" | "));
}

/// What a build produced (`build`, and `server`'s first build).
pub(crate) fn print_summary(r: &BuildReport) {
    println!(
        "pages {} | files {} (aliases {}) | resources {} | processed images {} | static files {}",
        r.pages, r.outputs, r.aliases, r.resources, r.images, r.static_files
    );
    if !r.collisions.is_empty() {
        println!("target collisions {}", r.collisions.len());
    }
}

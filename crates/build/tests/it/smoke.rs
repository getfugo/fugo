//! Manual smoke builds of real sites (ignored): `FUGO_SITES=<dir>[:<dir>…] cargo test -p
//! ssg-build smoke -- --ignored --nocapture` with directories from
//! `tools/rust-port/i01/sites.py make <site> <dir>` (Tera layouts copied over for the sites
//! that have them in `sites/<site>`). Prints the report or the error of each build.

use std::path::PathBuf;

use ssg_build::{BuildError, BuildRequest, SinkKind, build};

#[test]
#[ignore = "needs FUGO_SITES"]
fn smoke() {
    let Some(sites) = std::env::var_os("FUGO_SITES") else {
        return;
    };
    for dir in std::env::split_paths(&sites) {
        let r = build(BuildRequest {
            source: PathBuf::from(&dir),
            sink: SinkKind::Memory,
            clock: Some("2026-09-27T12:00:00Z".parse().expect("clock")),
            ..BuildRequest::default()
        });
        match r {
            Ok(r) => println!(
                "{}: pages {}, outputs {}, aliases {}, resources {}, images {}, static {}, \
                 collisions {}, warnings {}\n  timings {:?}",
                dir.display(),
                r.pages,
                r.outputs,
                r.aliases,
                r.resources,
                r.images,
                r.static_files,
                r.collisions.len(),
                r.diagnostics.len(),
                r.timings
            ),
            Err(BuildError::Diagnostics(ds)) => {
                println!("{}: {} diagnostics", dir.display(), ds.len());
                for d in ds.iter().take(20) {
                    println!("  {:?} {}", d.severity, d.message);
                }
            }
            Err(e) => println!("{}: {e}", dir.display()),
        }
    }
}

//! `server` (alias `serve`): flags → [`ServeOptions`] → [`Server`], its events printed like
//! Go's server prints them (build errors and warnings as `build` prints them).

use std::path::PathBuf;
use std::sync::Arc;

use ssg_serve::{
    ChangeKind, Event, HttpCache, LiveReloadOptions, Port, Reporter, ServeError, ServeOptions,
    Server, Target, Watch,
};

use crate::Exit;
use crate::args::ServerArgs;
use crate::{build, report};

/// The options of the flags; `Err` with a message for flags that do not go together.
pub(crate) fn options(a: &ServerArgs) -> anyhow::Result<Result<ServeOptions, String>> {
    let target = if a.serving.render_to_disk {
        Target::Disk
    } else {
        Target::Memory
    };
    if target == Target::Memory && a.build.output.destination.is_some() {
        return Ok(Err(
            "--destination needs --render-to-disk (the server renders into memory)".to_owned(),
        ));
    }
    let mut request = build::request(&a.build)?;
    // The server's environment is `development` unless `--environment` says otherwise.
    request.cli.environment = request
        .cli
        .environment
        .clone()
        .or_else(|| Some("development".to_owned()));
    Ok(Ok(ServeOptions {
        build: request,
        bind: a.listen.bind.clone(),
        port: a.listen.port.map_or(Port::Preferred(1313), Port::Exact),
        append_port: a.listen.append_port,
        live_reload: (!a.live_reload.disable_live_reload).then_some(LiveReloadOptions {
            port: a.live_reload.live_reload_port,
            navigate_to_changed: a.live_reload.navigate_to_changed,
        }),
        target,
        watch: match (a.watch.watch, a.watch.poll) {
            (false, _) => Watch::Off,
            (true, None) => Watch::Native,
            (true, Some(interval)) => Watch::Poll(interval),
        },
        http_cache: if a.serving.no_http_cache {
            HttpCache::Disabled
        } else {
            HttpCache::Default
        },
    }))
}

pub(crate) fn run(a: &ServerArgs) -> anyhow::Result<Exit> {
    let opts = match options(a)? {
        Ok(o) => o,
        Err(message) => {
            eprintln!("error: {message}");
            return Ok(Exit::Usage);
        }
    };
    let printer: Arc<dyn Reporter> = Arc::new(Printer {
        quiet: a.build.quiet,
    });
    let server = match Server::start(&opts, &printer) {
        Ok(s) => s,
        Err(ServeError::Build(e)) => {
            report::build_failed(&e);
            return Ok(Exit::Failure);
        }
        Err(e) => return Err(e.into()),
    };
    println!("Press Ctrl+C to stop");
    server.wait()?;
    Ok(Exit::Success)
}

/// Prints the server's events: progress on stdout, problems on stderr.
struct Printer {
    quiet: bool,
}

impl Reporter for Printer {
    fn report(&self, event: &Event<'_>) {
        match *event {
            Event::Starting {
                environment,
                target,
            } => {
                println!("Environment: {environment:?}");
                let from = match target {
                    Target::Memory => "memory",
                    Target::Disk => "disk",
                };
                println!("Serving pages from {from}");
            }
            Event::Watching { sources, config } => {
                println!("Watching for changes in {}", group(sources));
                println!("Watching for config changes in {}", group(config));
            }
            Event::PortInUse { port } => {
                println!("port {port} already in use, attempting to use an available port");
            }
            Event::Built {
                report: r,
                elapsed,
                first,
            } => {
                report::diagnostics(&r.diagnostics);
                if first {
                    if !self.quiet {
                        build::print_summary(r);
                    }
                    println!("Built in {} ms", elapsed.as_millis());
                } else {
                    println!("Total in {} ms", elapsed.as_millis());
                }
            }
            Event::BuildFailed { error } => report::build_failed(error),
            Event::ChangeDetected {
                number,
                kind,
                paths,
            } => {
                let what = match kind {
                    ChangeKind::Config => "Change of config file detected",
                    ChangeKind::Static => "Change of Static files detected",
                    ChangeKind::Site => "Change detected",
                };
                println!("\n{what}, rebuilding site (#{number}).");
                println!(
                    "{}",
                    jiff::Zoned::now().strftime("%Y-%m-%d %H:%M:%S%.3f %z")
                );
                for p in paths {
                    println!("Source changed {}", p.display());
                }
            }
            Event::StaticSynced { files, elapsed } => {
                println!(
                    "Synced {files} static file(s) in {} ms",
                    elapsed.as_millis()
                );
            }
            Event::ConfigFailed { error } => {
                eprintln!("ERROR Failed to reload config: {error}");
            }
            Event::Listening { url, bind } => {
                println!("Web Server is available at {url} (bind address {bind})");
            }
            Event::Error { message } => eprintln!("ERROR {message}"),
            // Go logs the LiveReload commands at the info level only.
            _ => {}
        }
    }
}

/// Paths with a common parent as `parent/{a,b}` (Go's grouping of watched directories).
fn group(paths: &[PathBuf]) -> String {
    let mut groups: Vec<(PathBuf, Vec<String>)> = Vec::new();
    for p in paths {
        let (parent, name) = match (p.parent(), p.file_name()) {
            (Some(parent), Some(name)) => {
                (parent.to_path_buf(), name.to_string_lossy().into_owned())
            }
            _ => (p.clone(), String::new()),
        };
        match groups.iter_mut().find(|(g, _)| *g == parent) {
            Some((_, names)) => names.push(name),
            None => groups.push((parent, vec![name])),
        }
    }
    groups
        .into_iter()
        .map(|(parent, names)| match names.as_slice() {
            [one] => parent.join(one).display().to_string(),
            _ => format!("{}/{{{}}}", parent.display(), names.join(",")),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

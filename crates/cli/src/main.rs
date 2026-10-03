//! The binary (see the library for the commands).

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::process::ExitCode;

use clap::Parser;
use ssg_cli::{Cli, Exit};

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    // Flags may come before the command, as in the Go build (`args::command_first`).
    match Cli::try_parse_from(ssg_cli::args::command_first(args)) {
        Ok(cli) => ssg_cli::run(cli).into(),
        Err(e) => {
            // --help and --version print and succeed; usage errors exit with 2.
            let _ = e.print();
            if e.use_stderr() {
                Exit::Usage.into()
            } else {
                Exit::Success.into()
            }
        }
    }
}

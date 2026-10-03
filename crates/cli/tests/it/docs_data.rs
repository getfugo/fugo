//! `docs/data/commands.json`: the command reference of the documentation site, generated from
//! the clap definitions of [`ssg_cli::Cli`] (schema `ssg-commands/1`). `INSTA_UPDATE=always`
//! rewrites it (reviewed with `git diff`); otherwise the test fails when it is stale.

use clap::{Arg, ArgAction, CommandFactory};
use serde_json::{Value, json};

/// A visible argument: its spellings, value, help and default.
fn arg(a: &Arg) -> Value {
    let takes_value = !matches!(
        a.get_action(),
        ArgAction::SetTrue
            | ArgAction::SetFalse
            | ArgAction::Count
            | ArgAction::Help
            | ArgAction::Version
    );
    json!({
        "id": a.get_id().as_str(),
        "long": a.get_long(),
        "short": a.get_short().map(|c| c.to_string()),
        "aliases": a.get_visible_aliases().unwrap_or_default(),
        "positional": a.is_positional(),
        "value_name": a.get_value_names().and_then(|v| v.first()).map(ToString::to_string),
        "takes_value": takes_value,
        "help": a.get_help().map(ToString::to_string),
        "long_help": a.get_long_help().map(ToString::to_string),
        "default": a.get_default_values().iter().map(|v| v.to_string_lossy().into_owned()).collect::<Vec<_>>(),
        "possible_values": a.get_possible_values().iter().filter(|p| !p.is_hide_set()).map(|p| p.get_name().to_owned()).collect::<Vec<_>>(),
        "heading": a.get_help_heading(),
        "global": a.is_global_set(),
    })
}

/// A command with its visible arguments and subcommands; `path` is how it is typed.
fn command(c: &mut clap::Command, path: &str) -> Value {
    let usage = c.render_usage().to_string();
    let args: Vec<Value> = c
        .get_arguments()
        .filter(|a| !a.is_hide_set() && a.get_id() != "help" && a.get_id() != "version")
        .map(arg)
        .collect();
    let mut subs = Vec::new();
    for s in c.get_subcommands_mut().filter(|s| !s.is_hide_set()) {
        if s.get_name() == "help" {
            continue;
        }
        let sub_path = format!("{path} {}", s.get_name());
        subs.push(command(s, &sub_path));
    }
    json!({
        "name": c.get_name(),
        "path": path,
        "about": c.get_about().map(ToString::to_string),
        "long_about": c.get_long_about().map(ToString::to_string),
        "aliases": c.get_visible_aliases().collect::<Vec<_>>(),
        "usage": usage.trim_start_matches("Usage: "),
        "args": args,
        "subcommands": subs,
    })
}

#[test]
fn docs_commands_json_matches_the_cli() {
    let mut cli = ssg_cli::Cli::command();
    cli.build();
    let name = cli.get_name().to_owned();
    let root = command(&mut cli, &name);
    let doc = json!({
        "schema": "ssg-commands/1",
        "about": "GENERATED from the clap definitions of crates/cli/src/args.rs; do not edit. Regenerate: INSTA_UPDATE=always cargo test -p ssg-cli --test it docs_commands_json",
        "root": root,
    });
    let want = serde_json::to_string_pretty(&doc).expect("JSON") + "\n";
    let path = ssg_testkit::fixture::repo_dir().join("docs/data/commands.json");
    if std::env::var("INSTA_UPDATE").is_ok_and(|v| v == "always") {
        std::fs::create_dir_all(path.parent().expect("dir")).expect("mkdir");
        std::fs::write(&path, &want).expect("write");
    }
    let have = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        have == want,
        "{} is stale; regenerate with INSTA_UPDATE=always cargo test -p ssg-cli --test it docs_commands_json",
        path.display()
    );
}

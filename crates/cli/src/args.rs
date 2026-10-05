//! The command line (clap). Flags are kebab-case; Go's camelCase spellings are aliases
//! (`--clean-destination-dir` / `--cleanDestinationDir`, `--base-url` / `--baseURL`).
//!
//! As in the Go build (cobra), flags may come before the command ([`command_first`]), and the Go
//! build's persistent flags (`-s`, `-d`, `-e`, `--config`, `--config-dir`, `--themes-dir`,
//! `--clock`, `-q`, `-M`) are accepted by every command (`global`); the commands that do not
//! use one ignore it. The Go build's logging and housekeeping flags (`--gc`, `--logLevel`, …)
//! are not: they are usage errors.
//!
//! A boolean flag takes pflag's explicit value (`--minify=false`, `-D=1`, with Go's
//! `strconv.ParseBool` spellings, `parse_bool`). The flags that set a configuration key
//! (`-D`, `-E`, `-F`, `--minify`, `--ignoreCache`, `--cleanDestinationDir`, `--noTimes`,
//! `--noChmod`) and `--watch` and `--appendPort` read the value themselves (`Option<bool>`, so
//! that `=false` overrides the configuration, as in Go); [`command_first`] rewrites it for the
//! others.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use clap::{
    Arg, ArgAction, Args, Command as ClapCommand, CommandFactory, Parser, Subcommand, ValueEnum,
};

mod parse;
pub use parse::*;

/// Builds a site with Tera layouts.
#[derive(Debug, Parser)]
#[command(
    name = ssg_base::app_name!(),
    // `--version` prints the name and this: the line of `version`.
    version = crate::version::line().strip_prefix(concat!(ssg_base::app_name!(), " ")).unwrap_or_default(),
    about,
    args_conflicts_with_subcommands = true,
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    /// Without a command: `build`.
    #[command(flatten)]
    pub build: BuildArgs,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Builds the site into the publish directory (the default command).
    Build(BuildArgs),
    /// Builds the site into memory, serves it with live reload, and rebuilds it when files
    /// change (Go's development server; environment `development` by default).
    #[command(alias = "serve")]
    Server(ServerArgs),
    /// Template tooling.
    #[command(subcommand)]
    Templates(TemplatesCommand),
    /// Prints the resolved configuration.
    Config(ConfigArgs),
    /// Content editor (`[cms]`) tooling.
    #[command(subcommand)]
    Cms(CmsCommand),
    /// Prints the version.
    Version,
}

#[derive(Debug, Subcommand)]
pub enum TemplatesCommand {
    /// Checks the layouts: Tera parse and name errors, context names, lookup coverage and the
    /// conversion lints (REWRITE_PLAN.md §4.8).
    Check(CheckArgs),
}

#[derive(Debug, Subcommand)]
pub enum CmsCommand {
    /// Prints the editor's fields as `[cms.fields]` settings to start from: every front matter
    /// key of the content, with the settings it has, else a label and the widget its values
    /// suggest.
    Fields(CmsFieldsArgs),
}

/// `cms fields`.
#[derive(Clone, Debug, Default, Args)]
pub struct CmsFieldsArgs {
    #[command(flatten)]
    pub project: ProjectArgs,
}

/// What every command that loads the project takes.
#[derive(Clone, Debug, Default, Args)]
pub struct ProjectArgs {
    /// The project directory (default: the working directory).
    #[arg(short = 's', long, value_name = "DIR", global = true)]
    pub source: Option<PathBuf>,
    /// Configuration files, relative to the source (comma-separated; the first wins). Default:
    /// the first of `config.{toml,yaml,yml,json}`.
    #[arg(long, value_name = "FILES", value_delimiter = ',', global = true)]
    pub config: Vec<PathBuf>,
    /// The configuration directory (default `config`).
    #[arg(long, alias = "configDir", value_name = "DIR", global = true)]
    pub config_dir: Option<PathBuf>,
    /// The build environment (default `production`, `development` for `server`); also picks
    /// the project's `.env.<environment>` file.
    #[arg(short = 'e', long, value_name = "ENV", global = true)]
    pub environment: Option<String>,
    /// The site's base URL.
    #[arg(short = 'b', long, alias = "baseURL", value_name = "URL")]
    pub base_url: Option<String>,
    /// Themes to use (comma-separated).
    #[arg(short = 't', long, value_delimiter = ',', value_name = "THEMES")]
    pub theme: Vec<String>,
    /// The themes directory.
    #[arg(long, alias = "themesDir", value_name = "DIR", global = true)]
    pub themes_dir: Option<PathBuf>,
    /// The cache directory.
    #[arg(long, alias = "cacheDir", value_name = "DIR")]
    pub cache_dir: Option<PathBuf>,
    /// Ignores the cache directory.
    #[arg(
        long,
        alias = "ignoreCache",
        value_name = "BOOL",
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = parse_bool
    )]
    pub ignore_cache: Option<bool>,
    #[command(flatten)]
    pub include: IncludeArgs,
    /// The build's "now" (RFC 3339, e.g. `2026-09-27T12:00:00Z`), for dates and `now()`.
    #[arg(long, value_name = "TIME", value_parser = parse_clock, global = true)]
    pub clock: Option<jiff::Timestamp>,
}

/// Which content the build policy lets in besides published content (`None`: as configured;
/// `-D=false` overrides a configured `buildDrafts = true`).
#[derive(Clone, Copy, Debug, Default, Args)]
pub struct IncludeArgs {
    /// Includes content marked as draft.
    #[arg(
        short = 'D',
        long,
        alias = "buildDrafts",
        value_name = "BOOL",
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = parse_bool
    )]
    pub build_drafts: Option<bool>,
    /// Includes expired content.
    #[arg(
        short = 'E',
        long,
        alias = "buildExpired",
        value_name = "BOOL",
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = parse_bool
    )]
    pub build_expired: Option<bool>,
    /// Includes content with a publish date in the future.
    #[arg(
        short = 'F',
        long,
        alias = "buildFuture",
        value_name = "BOOL",
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = parse_bool
    )]
    pub build_future: Option<bool>,
}

/// `build` (and the command line without a command).
#[derive(Clone, Debug, Default, Args)]
pub struct BuildArgs {
    #[command(flatten)]
    pub project: ProjectArgs,
    #[command(flatten)]
    pub output: OutputArgs,
    /// Minifies the supported output formats.
    #[arg(
        long,
        value_name = "BOOL",
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = parse_bool
    )]
    pub minify: Option<bool>,
    /// The render thread count (default: `RAYON_NUM_THREADS`, else the CPUs). The output does
    /// not depend on it.
    #[arg(long, value_name = "N")]
    pub threads: Option<usize>,
    /// Prints only warnings and errors.
    #[arg(short = 'q', long, global = true)]
    pub quiet: bool,
}

/// Where a build writes.
#[derive(Clone, Debug, Default, Args)]
pub struct OutputArgs {
    /// The publish directory, relative to the source.
    #[arg(short = 'd', long, value_name = "DIR", global = true)]
    pub destination: Option<PathBuf>,
    /// Removes files from the publish directory that the static directories do not have.
    #[arg(
        long,
        alias = "cleanDestinationDir",
        value_name = "BOOL",
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = parse_bool
    )]
    pub clean_destination_dir: Option<bool>,
    /// Renders into memory only (a dry run: nothing is written; what `server` does by default).
    #[arg(short = 'M', long, alias = "renderToMemory", global = true)]
    pub render_to_memory: bool,
    /// Does not copy the static files' modification times (config `noTimes`).
    #[arg(
        long,
        alias = "noTimes",
        value_name = "BOOL",
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = parse_bool
    )]
    pub no_times: Option<bool>,
    /// Does not copy the static files' permissions (config `noChmod`).
    #[arg(
        long,
        alias = "noChmod",
        value_name = "BOOL",
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = parse_bool
    )]
    pub no_chmod: Option<bool>,
}

/// `server` (alias `serve`): `build`'s flags and the server's.
#[derive(Clone, Debug, Args)]
pub struct ServerArgs {
    #[command(flatten)]
    pub build: BuildArgs,
    #[command(flatten)]
    pub listen: ListenArgs,
    #[command(flatten)]
    pub live_reload: LiveReloadArgs,
    #[command(flatten)]
    pub serving: ServingArgs,
    #[command(flatten)]
    pub watch: WatchArgs,
}

/// Where the server listens.
#[derive(Clone, Debug, Args)]
#[command(next_help_heading = "Server")]
pub struct ListenArgs {
    /// The port to listen on [default: 1313, or a free port when it is taken; 0: a free port].
    #[arg(short = 'p', long, value_name = "PORT")]
    pub port: Option<u16>,
    /// The interface to listen on.
    #[arg(long, value_name = "INTERFACE", default_value = "127.0.0.1")]
    pub bind: String,
    /// Puts the server's port into the base URL (`--append-port=false`: the configured or
    /// `--base-url` port).
    #[arg(
        long,
        alias = "appendPort",
        value_name = "BOOL",
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_value_t = true,
        default_missing_value = "true",
        value_parser = parse_bool
    )]
    pub append_port: bool,
}

/// The LiveReload script and WebSocket.
#[derive(Clone, Debug, Args)]
#[command(next_help_heading = "Live reload")]
pub struct LiveReloadArgs {
    /// Serves the pages without the LiveReload script and endpoints.
    #[arg(long, alias = "disableLiveReload")]
    pub disable_live_reload: bool,
    /// The port the browsers' LiveReload connects to (e.g. 443 behind an HTTPS proxy).
    #[arg(long, alias = "liveReloadPort", value_name = "PORT")]
    pub live_reload_port: Option<u16>,
    /// Sends the browsers to the page whose content file changed.
    #[arg(short = 'N', long, alias = "navigateToChanged")]
    pub navigate_to_changed: bool,
}

/// What is served from where.
#[derive(Clone, Debug, Args)]
#[command(next_help_heading = "Serving")]
pub struct ServingArgs {
    /// Builds into the publish directory (`--destination`) and serves it from there, instead of
    /// memory.
    #[arg(long, conflicts_with = "render_to_memory")]
    pub render_to_disk: bool,
    /// Sends headers that keep browsers from caching (`Cache-Control: no-store`, …).
    #[arg(long, alias = "noHTTPCache")]
    pub no_http_cache: bool,
}

/// How changes are noticed.
#[derive(Clone, Debug, Args)]
#[command(next_help_heading = "Watching")]
pub struct WatchArgs {
    /// Watches the project and rebuilds on changes (`--watch=false`: build once).
    #[arg(
        short = 'w',
        long,
        value_name = "BOOL",
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_value_t = true,
        default_missing_value = "true",
        value_parser = parse_bool
    )]
    pub watch: bool,
    /// Polls for changes at this interval instead of using file notifications (`700ms`, `2s`;
    /// a number is milliseconds). Polling reads the watched files each time.
    #[arg(long, value_name = "INTERVAL", value_parser = parse_poll)]
    pub poll: Option<Duration>,
}

/// `templates check`.
#[derive(Clone, Debug, Default, Args)]
pub struct CheckArgs {
    #[command(flatten)]
    pub project: ProjectArgs,
    /// How much of the lookup coverage to print.
    #[arg(long, value_enum, default_value_t = Coverage::Summary)]
    pub coverage: Coverage,
    /// Exits with 1 when there are warnings, too.
    #[arg(long)]
    pub deny_warnings: bool,
}

/// The lookup coverage listing of `templates check`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Coverage {
    /// Templates with their query counts, every miss.
    #[default]
    Summary,
    /// Every (page, format) query, shortcode and hook lookup.
    Full,
    /// No coverage (the site's content is not loaded).
    None,
}

/// `config`.
#[derive(Clone, Debug, Default, Args)]
pub struct ConfigArgs {
    #[command(flatten)]
    pub project: ProjectArgs,
    #[arg(long, value_enum, default_value_t = ConfigFormat::Json)]
    pub format: ConfigFormat,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum ConfigFormat {
    #[default]
    Json,
    Toml,
}

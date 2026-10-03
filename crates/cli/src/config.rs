//! `config`: the resolved configuration (every file, environment and flag merged; one entry per
//! language), as JSON or TOML.

use anyhow::Context as _;

use crate::Exit;
use crate::args::{ConfigArgs, ConfigFormat};

pub(crate) fn run(a: &ConfigArgs) -> anyhow::Result<Exit> {
    let cfg = ssg_config::load(&a.project.load_options()?)?;
    let text = match a.format {
        ConfigFormat::Json => serde_json::to_string_pretty(&cfg)?,
        ConfigFormat::Toml => toml::to_string(&cfg).context("the configuration as TOML")?,
    };
    println!("{}", text.trim_end());
    Ok(Exit::Success)
}

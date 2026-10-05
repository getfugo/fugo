//! `cms fields`: the editor's fields as `[cms.fields]` settings to start from.

use crate::Exit;
use crate::args::CmsFieldsArgs;

pub(crate) fn run(a: &CmsFieldsArgs) -> anyhow::Result<Exit> {
    let cfg = ssg_config::load(&a.project.load_options()?)?;
    let Some(cms) = ssg_cms::settings(&cfg)? else {
        anyhow::bail!("the configuration has no [cms]: the editor and its fields need one");
    };
    print!("{}", ssg_cms::fields::toml(&cfg, &cms)?);
    Ok(Exit::Success)
}

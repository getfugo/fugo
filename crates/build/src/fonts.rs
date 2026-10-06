//! Phase E6a, `[fonts]`: the published fonts cut down to the characters the site uses
//! (`ssg_fonts`). From E2 on, the site's files are written through a [`Recorder`], which notes
//! the characters of its HTML and CSS; the static files, copied in E1 around it, are noted in
//! E6a.

use std::sync::Arc;

use ssg_base::Sink;
use ssg_base::diag::Diagnostics;
use ssg_base::paths::OutputPath;
use ssg_config::Config;
use ssg_fonts::{Cut, FontsConfig, Recorder};
use ssg_publish::StaticSyncOptions;
use ssg_vfs::{Component, Vfs};

use crate::{BuildError, RenderPool};

/// `[fonts]`, and the recorder the site's files are written through.
pub(crate) struct Fonts {
    config: FontsConfig,
    recorder: Arc<Recorder>,
}

impl Fonts {
    /// The configuration's `[fonts]`, recording what is written into `sink`; `None` without
    /// `[[fonts.subset]]` entries.
    pub(crate) fn new(cfg: &Config, sink: &Arc<dyn Sink>) -> Result<Option<Self>, BuildError> {
        Ok(ssg_fonts::settings(cfg)?.map(|config| Self {
            recorder: Arc::new(Recorder::new(Arc::clone(sink), config.needs_text())),
            config,
        }))
    }

    /// The sink the site's files are written through.
    pub(crate) fn sink(&self) -> Arc<dyn Sink> {
        Arc::clone(&self.recorder) as Arc<dyn Sink>
    }

    /// E6a: cuts the fonts down; returns those cut, and pushes the warnings.
    pub(crate) fn run(
        &self,
        vfs: &Vfs,
        sync: &StaticSyncOptions,
        pool: &RenderPool,
        diagnostics: &Diagnostics,
    ) -> Result<Vec<Cut>, BuildError> {
        let published: Vec<OutputPath> = vfs
            .walk(Component::Static)?
            .iter()
            .map(|f| OutputPath::new(&sync.target(f)))
            .collect();
        let done =
            pool.run(|| ssg_fonts::subset_fonts(&self.config, &self.recorder, &published))?;
        for w in done.warnings {
            diagnostics.push(w);
        }
        Ok(done.cuts)
    }
}

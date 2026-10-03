//! What the scan and the lookups need to know about the project.

use std::sync::Arc;

use ssg_base::{FormatId, Idx, LangIdx, MediaTypeId};
use ssg_config::markup::UseEmbedded;
use ssg_config::output::Escaping;
use ssg_config::{Config, MediaTypes, OutputFormats, SiteConfig};
use ssg_vfs::PathParser;

/// The project facts the layout store depends on: the path parser (languages and output
/// formats in file names), the output formats and media types, and the default output format.
#[derive(Clone, Debug)]
pub struct LayoutEnv {
    pub(crate) parser: PathParser,
    pub(crate) formats: Arc<OutputFormats>,
    pub(crate) media: Arc<MediaTypes>,
    pub(crate) default_format: Option<FormatId>,
}

impl LayoutEnv {
    /// The environment of a loaded project.
    #[must_use]
    pub fn from_config(cfg: &Config) -> Self {
        Self::new(
            PathParser::from_config(cfg),
            Arc::clone(&cfg.output_formats),
            Arc::clone(&cfg.media_types),
            cfg.output_formats.by_name(&cfg.default_output_format),
        )
    }

    /// An environment from its parts (`default_format`: the site's `defaultOutputFormat`).
    #[must_use]
    pub fn new(
        parser: PathParser,
        formats: Arc<OutputFormats>,
        media: Arc<MediaTypes>,
        default_format: Option<FormatId>,
    ) -> Self {
        Self {
            parser,
            formats,
            media,
            default_format,
        }
    }

    /// The default content language (languages are in configuration order, the default first).
    pub(crate) fn default_lang() -> LangIdx {
        LangIdx::from_index(0)
    }

    pub(crate) fn defaults(&self) -> crate::score::Defaults {
        crate::score::Defaults {
            lang: Self::default_lang(),
            format: self.default_format,
        }
    }

    /// The media type and whether it is plain text for a format (Go's
    /// `resolveOutputFormatAndOrMediaType`): the named format's media type; else the format
    /// that is the only one with suffix `ext`; else the first media type with that suffix, with
    /// the format named like its sub type deciding plain text.
    pub(crate) fn resolve(
        &self,
        format: Option<FormatId>,
        ext: &str,
    ) -> (Option<MediaTypeId>, Option<Escaping>) {
        if let Some(f) = format {
            let f = self.formats.get(f);
            return (Some(f.media_type), Some(f.escaping));
        }
        if ext.is_empty() {
            return (None, None);
        }
        let by_suffix: Vec<FormatId> = self
            .formats
            .iter()
            .filter(|(_, f)| self.media.get(f.media_type).has_suffix(ext))
            .map(|(id, _)| id)
            .collect();
        if let [only] = by_suffix[..] {
            let f = self.formats.get(only);
            return (Some(f.media_type), Some(f.escaping));
        }
        match self.media.by_suffix(ext) {
            Some(mt) => {
                let sub = &self.media.get(mt).sub;
                let escaping = self
                    .formats
                    .by_name(sub)
                    .map(|f| self.formats.get(f).escaping);
                (Some(mt), escaping)
            }
            None => (None, None),
        }
    }

    /// The path parser.
    #[must_use]
    pub fn parser(&self) -> &PathParser {
        &self.parser
    }

    /// The output formats.
    #[must_use]
    pub fn formats(&self) -> &OutputFormats {
        &self.formats
    }

    /// The media types.
    #[must_use]
    pub fn media_types(&self) -> &MediaTypes {
        &self.media
    }
}

/// When the embedded link and image render hooks are used (`useEmbedded`, resolved per
/// language).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmbeddedHooks {
    pub link: HookUse,
    pub image: HookUse,
}

/// The resolved `useEmbedded` of one hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookUse {
    /// Only the embedded hook.
    Always,
    /// The embedded hook unless the project or a theme has one.
    Fallback,
    /// Never the embedded hook.
    Never,
}

impl EmbeddedHooks {
    /// Both hooks with the same policy.
    #[must_use]
    pub const fn both(u: HookUse) -> Self {
        Self { link: u, image: u }
    }

    /// The policy of a language: `auto` is `fallback` for multilingual single-host sites and
    /// `never` otherwise; unset is `auto`.
    #[must_use]
    pub fn of(site: &SiteConfig, cfg: &Config) -> Self {
        let auto = if cfg.sites.len() > 1 && !cfg.multihost {
            HookUse::Fallback
        } else {
            HookUse::Never
        };
        let hooks = &site.markup.goldmark.render_hooks;
        let resolve = |u: Option<UseEmbedded>| match u.unwrap_or(UseEmbedded::Auto) {
            UseEmbedded::Always => HookUse::Always,
            UseEmbedded::Fallback => HookUse::Fallback,
            UseEmbedded::Never => HookUse::Never,
            UseEmbedded::Auto => auto,
        };
        Self {
            link: resolve(hooks.link.use_embedded),
            image: resolve(hooks.image.use_embedded),
        }
    }

    /// The policy of hook kind `k` (`Fallback` for kinds other than link and image).
    #[must_use]
    pub fn get(self, k: crate::HookKind) -> HookUse {
        match k {
            crate::HookKind::Link => self.link,
            crate::HookKind::Image => self.image,
            _ => HookUse::Fallback,
        }
    }
}

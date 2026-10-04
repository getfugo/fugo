//! The views of the build and the site configuration: services, privacy and the build's settings.

use super::*;

/// `build`: the program, its version and the build environment.
#[derive(Clone, Debug, Serialize)]
pub struct BuildView {
    /// The program's name (`build.name`), e.g. for a feed's `<generator>`.
    pub name: &'static str,
    /// The program's version (`build.version`).
    pub version: &'static str,
    pub environment: String,
    pub is_production: bool,
    pub is_development: bool,
    pub is_server: bool,
    pub generator: tera::Value,
}

impl BuildView {
    /// `server`: the build runs in the `server` command (`build.is_server`).
    #[must_use]
    pub fn new(cfg: &Config, server: bool) -> Self {
        Self {
            name: ssg_base::APP_NAME,
            version: ssg_base::VERSION,
            environment: cfg.environment.clone(),
            is_production: cfg.environment == "production",
            is_development: cfg.environment == "development",
            is_server: server,
            generator: tera::Value::safe_string(&format!(
                r#"<meta name="generator" content="{} {}">"#,
                ssg_base::APP_NAME,
                ssg_base::VERSION
            )),
        }
    }
}

/// `site.config`: the configuration the embedded templates read, in snake case.
#[derive(Clone, Debug, Serialize)]
pub struct SiteConfigView {
    pub services: ServicesView,
    /// Every service has every switch (false when the service has none).
    pub privacy: PrivacyView,
}

#[derive(Clone, Debug, Serialize)]
pub struct ServicesView {
    pub rss: RssView,
    pub google_analytics: GoogleAnalyticsView,
    pub disqus: DisqusView,
    pub instagram: InlineCssView,
    pub x: InlineCssView,
    pub twitter: InlineCssView,
}

#[derive(Clone, Debug, Serialize)]
pub struct RssView {
    /// `-1`: no limit.
    pub limit: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct GoogleAnalyticsView {
    pub id: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DisqusView {
    pub shortname: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct InlineCssView {
    pub disable_inline_css: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct PrivacyView {
    pub disqus: PrivacyServiceView,
    pub google_analytics: PrivacyServiceView,
    pub instagram: PrivacyServiceView,
    pub twitter: PrivacyServiceView,
    pub vimeo: PrivacyServiceView,
    pub x: PrivacyServiceView,
    pub youtube: PrivacyServiceView,
}

/// The privacy switches of one service.
#[derive(Clone, Debug, Default, Serialize)]
#[allow(clippy::struct_excessive_bools)] // configuration switches, as Go names them
pub struct PrivacyServiceView {
    pub disable: bool,
    pub simple: bool,
    pub enable_dnt: bool,
    pub respect_do_not_track: bool,
    pub privacy_enhanced: bool,
}

impl SiteConfigView {
    #[must_use]
    pub fn new(cfg: &Config, site: &SiteConfig) -> Self {
        let s = &site.services;
        let p = &cfg.privacy;
        let x = |v: &ssg_config::global::XPrivacy| PrivacyServiceView {
            disable: v.disable,
            simple: v.simple,
            enable_dnt: v.enable_dnt,
            ..PrivacyServiceView::default()
        };
        Self {
            services: ServicesView {
                rss: RssView { limit: s.rss.limit },
                google_analytics: GoogleAnalyticsView {
                    id: s.google_analytics.id.clone(),
                },
                disqus: DisqusView {
                    shortname: s.disqus.shortname.clone(),
                },
                instagram: InlineCssView {
                    disable_inline_css: s.instagram.disable_inline_css,
                },
                x: InlineCssView {
                    disable_inline_css: s.x.disable_inline_css,
                },
                twitter: InlineCssView {
                    disable_inline_css: s.twitter.disable_inline_css,
                },
            },
            privacy: PrivacyView {
                disqus: PrivacyServiceView {
                    disable: p.disqus.disable,
                    ..PrivacyServiceView::default()
                },
                google_analytics: PrivacyServiceView {
                    disable: p.google_analytics.disable,
                    respect_do_not_track: p.google_analytics.respect_do_not_track,
                    ..PrivacyServiceView::default()
                },
                instagram: PrivacyServiceView {
                    disable: p.instagram.disable,
                    simple: p.instagram.simple,
                    ..PrivacyServiceView::default()
                },
                twitter: x(&p.twitter),
                vimeo: x(&p.vimeo),
                x: x(&p.x),
                youtube: PrivacyServiceView {
                    disable: p.youtube.disable,
                    privacy_enhanced: p.youtube.privacy_enhanced,
                    ..PrivacyServiceView::default()
                },
            },
        }
    }
}

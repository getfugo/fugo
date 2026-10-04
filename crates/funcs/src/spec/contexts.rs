//! The render contexts: what each kind of template is given, and the fields of the render hooks.

/// The kinds of render, each with its own set of top-level names (REWRITE_PLAN.md §4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RenderRole {
    LayoutJob,
    Shortcode,
    RenderHook,
    Partial,
    Component,
    Deferred,
    ExecuteAsTemplate,
    Alias,
    Standalone,
    SitemapIndex,
    ContentAdapter,
}

/// One top-level name of a render context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContextName {
    pub name: &'static str,
    pub doc: &'static str,
}

/// The top-level names of one kind of render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContextSpec {
    pub role: RenderRole,
    pub title: &'static str,
    pub names: &'static [ContextName],
    /// Further names that depend on the call (kwargs, flattened hook fields, component arguments).
    pub note: &'static str,
}

pub(super) const fn cn(name: &'static str, doc: &'static str) -> ContextName {
    ContextName { name, doc }
}

pub(super) const SITE: ContextName = cn("site", "`SiteView` of the current language");

pub(super) const BUILD: ContextName = cn("build", "`BuildView`: version, environment, generator");

pub(super) const LANG: ContextName = cn("lang", "the language code of the page");

pub(super) const OUTPUT_FORMAT: ContextName =
    cn("output_format", "`OutputFormatView` being rendered");

pub(super) const NH: ContextName = cn(
    "__nh",
    "the render scope (`RenderScope`); read by site-bound functions",
);

pub(super) const DATA: ContextName = cn("data", "the `data=` value of the call");

/// The top-level names of every render (REWRITE_PLAN.md §4.2).
pub const CONTEXTS: &[ContextSpec] = &[
    ContextSpec {
        role: RenderRole::LayoutJob,
        title: "Layout job",
        names: &[
            cn("page", "the full page value of the Full generation"),
            SITE,
            BUILD,
            LANG,
            OUTPUT_FORMAT,
            NH,
        ],
        note: "",
    },
    ContextSpec {
        role: RenderRole::Shortcode,
        title: "Shortcode",
        names: &[
            cn(
                "page",
                "the full page value of the Meta generation: relations yes, content fields no",
            ),
            SITE,
            BUILD,
            LANG,
            cn(
                "shortcode",
                "`ShortcodeView`: name, args, params, is_named_params, ordinal, parent, position",
            ),
            cn("inner", "the inner content (safe)"),
            cn(
                "inner_deindent",
                "the inner content without common indentation",
            ),
            NH,
        ],
        note: "",
    },
    ContextSpec {
        role: RenderRole::RenderHook,
        title: "Render hook",
        names: &[
            cn("page", "the page being rendered (Meta generation)"),
            cn(
                "page_inner",
                "the page whose source holds the hooked node (differs inside `render_shortcodes`)",
            ),
            SITE,
            BUILD,
            LANG,
            NH,
        ],
        note: "plus the hook's fields, flattened (below)",
    },
    ContextSpec {
        role: RenderRole::Partial,
        title: "`partial(name=…, …)`",
        names: &[
            cn("page", "the caller's page"),
            SITE,
            BUILD,
            LANG,
            OUTPUT_FORMAT,
            cn(
                "__nh",
                "a child scope: same page, format and pager; new frame; depth + 1",
            ),
        ],
        note: "plus the call's kwargs as top-level names",
    },
    ContextSpec {
        role: RenderRole::Component,
        title: "Component",
        names: &[],
        note: "only its declared arguments; `@page`, `@site`, `@build`, `@lang` and `@__nh` may be declared as implicit arguments (looked up in the caller's scope)",
    },
    ContextSpec {
        role: RenderRole::Deferred,
        title: "`defer` template",
        names: &[
            DATA,
            SITE,
            BUILD,
            cn("__nh", "the render scope, phase `Deferred`"),
        ],
        note: "",
    },
    ContextSpec {
        role: RenderRole::ExecuteAsTemplate,
        title: "`execute_as_template`",
        names: &[DATA, SITE, BUILD, NH],
        note: "",
    },
    ContextSpec {
        role: RenderRole::Alias,
        title: "Alias",
        names: &[
            cn("permalink", "the target URL"),
            cn("page", "the target page (link value)"),
            SITE,
            BUILD,
        ],
        note: "",
    },
    ContextSpec {
        role: RenderRole::Standalone,
        title: "Sitemap, robots, 404",
        names: &[
            cn("page", "the standalone page; its `pages` is `site.pages`"),
            SITE,
            BUILD,
            LANG,
            NH,
        ],
        note: "",
    },
    ContextSpec {
        role: RenderRole::SitemapIndex,
        title: "Sitemapindex",
        names: &[
            cn("page", "the standalone page"),
            SITE,
            BUILD,
            LANG,
            NH,
            cn("sites", "`[{language, sitemap_abs_url, last_mod}]`"),
        ],
        note: "",
    },
    ContextSpec {
        role: RenderRole::ContentAdapter,
        title: "Content adapter (`content/**/_content.html`)",
        names: &[
            SITE,
            BUILD,
            cn("lang", "the language the adapter runs for"),
            cn("__nh", "the render scope, phase `Adapter`"),
        ],
        note: "`site` has no page lists (`home`, `pages`, `regular_pages`, `all_pages`, `sections`, `main_sections`, `taxonomies`, `menus`): the model is not built yet",
    },
];

/// The fields a render hook sees flattened into its context, per hook kind (REWRITE_PLAN.md §4.2).
pub const HOOK_FIELDS: &[(&str, &[&str])] = &[
    (
        "link, image",
        &[
            "destination",
            "title",
            "text",
            "plain_text",
            "is_block",
            "attributes",
            "ordinal",
            "position",
        ],
    ),
    (
        "heading",
        &["level", "anchor", "text", "plain_text", "attributes"],
    ),
    (
        "codeblock",
        &[
            "type",
            "inner",
            "options",
            "attributes",
            "ordinal",
            "position",
        ],
    ),
    (
        "blockquote",
        &[
            "type",
            "alert_type",
            "alert_title",
            "alert_sign",
            "text",
            "attributes",
            "ordinal",
        ],
    ),
    ("table", &["thead", "tbody", "attributes", "ordinal"]),
    (
        "passthrough",
        &["type", "inner", "attributes", "ordinal", "position"],
    ),
];

//! `[markup]`: the Markdown renderer, highlighting and table of contents settings.

use serde::{Deserialize, Deserializer, Serialize};
use ssg_base::anchor;
use ssg_base::{Map, Value};

/// `[markup]`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MarkupConfig {
    /// `goldmark` (the only handler this port implements).
    pub default_markdown_handler: String,
    pub goldmark: GoldmarkConfig,
    pub highlight: HighlightConfig,
    pub table_of_contents: TocConfig,
    /// `[markup.asciidocExt]` (AsciiDoc is rendered by an external program).
    pub asciidoc_ext: AsciidocConfig,
}

impl Default for MarkupConfig {
    fn default() -> Self {
        Self {
            default_markdown_handler: "goldmark".to_owned(),
            goldmark: GoldmarkConfig::default(),
            highlight: HighlightConfig::default(),
            table_of_contents: TocConfig::default(),
            asciidoc_ext: AsciidocConfig::default(),
        }
    }
}

/// `[markup.asciidocExt]`: the options of the external `asciidoctor` program.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors asciidoctor's command-line switches"
)]
pub struct AsciidocConfig {
    pub backend: String,
    pub extensions: Vec<String>,
    #[serde(deserialize_with = "crate::de_map")]
    pub attributes: Map,
    pub no_header_or_footer: bool,
    pub safe_mode: String,
    pub section_numbers: bool,
    pub verbose: bool,
    pub trace: bool,
    pub failure_level: String,
    pub working_folder_current: bool,
    #[serde(rename = "preserveTOC")]
    pub preserve_toc: bool,
}

impl Default for AsciidocConfig {
    fn default() -> Self {
        Self {
            backend: "html5".to_owned(),
            extensions: Vec::new(),
            attributes: Map::new(),
            no_header_or_footer: true,
            safe_mode: "unsafe".to_owned(),
            section_numbers: false,
            verbose: false,
            trace: false,
            failure_level: "fatal".to_owned(),
            working_folder_current: false,
            preserve_toc: false,
        }
    }
}

/// `[markup.goldmark]`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GoldmarkConfig {
    pub extensions: Extensions,
    pub parser: ParserConfig,
    pub renderer: RendererConfig,
    pub render_hooks: RenderHooks,
    /// Whether each language publishes its own copy of shared bundle resources.
    pub duplicate_resource_files: bool,
}

/// `[markup.goldmark.extensions]`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one switch per Markdown extension, as configured"
)]
pub struct Extensions {
    pub typographer: Typographer,
    pub footnote: Footnote,
    pub definition_list: bool,
    pub table: bool,
    pub strikethrough: bool,
    pub linkify: bool,
    pub linkify_protocol: String,
    pub task_list: bool,
    pub passthrough: Passthrough,
    pub cjk: Cjk,
    pub extras: Extras,
}

impl Default for Extensions {
    fn default() -> Self {
        Self {
            typographer: Typographer::default(),
            footnote: Footnote::default(),
            definition_list: true,
            table: true,
            strikethrough: true,
            linkify: true,
            linkify_protocol: "https".to_owned(),
            task_list: true,
            passthrough: Passthrough::default(),
            cjk: Cjk::default(),
            extras: Extras::default(),
        }
    }
}

/// `[markup.goldmark.extensions.typographer]`: the replacement of each typographic
/// construct. A boolean is the legacy form: `false` disables it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Typographer {
    pub disable: bool,
    pub left_single_quote: String,
    pub right_single_quote: String,
    pub left_double_quote: String,
    pub right_double_quote: String,
    pub en_dash: String,
    pub em_dash: String,
    pub ellipsis: String,
    pub left_angle_quote: String,
    pub right_angle_quote: String,
    pub apostrophe: String,
}

impl Default for Typographer {
    fn default() -> Self {
        Self {
            disable: false,
            left_single_quote: "&lsquo;".to_owned(),
            right_single_quote: "&rsquo;".to_owned(),
            left_double_quote: "&ldquo;".to_owned(),
            right_double_quote: "&rdquo;".to_owned(),
            en_dash: "&ndash;".to_owned(),
            em_dash: "&mdash;".to_owned(),
            ellipsis: "&hellip;".to_owned(),
            left_angle_quote: "&laquo;".to_owned(),
            right_angle_quote: "&raquo;".to_owned(),
            apostrophe: "&rsquo;".to_owned(),
        }
    }
}

impl<'de> Deserialize<'de> for Typographer {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(default, rename_all = "camelCase")]
        struct Table {
            disable: bool,
            left_single_quote: String,
            right_single_quote: String,
            left_double_quote: String,
            right_double_quote: String,
            en_dash: String,
            em_dash: String,
            ellipsis: String,
            left_angle_quote: String,
            right_angle_quote: String,
            apostrophe: String,
        }
        impl Default for Table {
            fn default() -> Self {
                let t = Typographer::default();
                Self {
                    disable: t.disable,
                    left_single_quote: t.left_single_quote,
                    right_single_quote: t.right_single_quote,
                    left_double_quote: t.left_double_quote,
                    right_double_quote: t.right_double_quote,
                    en_dash: t.en_dash,
                    em_dash: t.em_dash,
                    ellipsis: t.ellipsis,
                    left_angle_quote: t.left_angle_quote,
                    right_angle_quote: t.right_angle_quote,
                    apostrophe: t.apostrophe,
                }
            }
        }
        let v = Value::deserialize(d)?;
        if let Some(on) = crate::de::weak_bool(&v).filter(|_| !matches!(v, Value::Map(_))) {
            return Ok(if on {
                Self::default()
            } else {
                Self {
                    disable: true,
                    ..Self::empty()
                }
            });
        }
        let t: Table = crate::de::from_value(&v).map_err(serde::de::Error::custom)?;
        Ok(Self {
            disable: t.disable,
            left_single_quote: t.left_single_quote,
            right_single_quote: t.right_single_quote,
            left_double_quote: t.left_double_quote,
            right_double_quote: t.right_double_quote,
            en_dash: t.en_dash,
            em_dash: t.em_dash,
            ellipsis: t.ellipsis,
            left_angle_quote: t.left_angle_quote,
            right_angle_quote: t.right_angle_quote,
            apostrophe: t.apostrophe,
        })
    }
}

impl Typographer {
    fn empty() -> Self {
        Self {
            disable: false,
            left_single_quote: String::new(),
            right_single_quote: String::new(),
            left_double_quote: String::new(),
            right_double_quote: String::new(),
            en_dash: String::new(),
            em_dash: String::new(),
            ellipsis: String::new(),
            left_angle_quote: String::new(),
            right_angle_quote: String::new(),
            apostrophe: String::new(),
        }
    }
}

/// `[markup.goldmark.extensions.footnote]`. A boolean is the short form of `enable`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Footnote {
    pub enable: bool,
    /// The HTML of the link back from a footnote (`footnoteReturnLinkContents`); empty for
    /// the renderer's default.
    #[serde(rename = "backlinkHTML")]
    pub backlink_html: String,
}

impl Default for Footnote {
    fn default() -> Self {
        Self {
            enable: true,
            backlink_html: String::new(),
        }
    }
}

impl<'de> Deserialize<'de> for Footnote {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(default, rename_all = "camelCase")]
        struct Table {
            enable: bool,
            #[serde(rename = "backlinkHTML")]
            backlink_html: String,
        }
        impl Default for Table {
            fn default() -> Self {
                Self {
                    enable: true,
                    backlink_html: String::new(),
                }
            }
        }
        let v = Value::deserialize(d)?;
        if !matches!(v, Value::Map(_))
            && let Some(on) = crate::de::weak_bool(&v)
        {
            return Ok(Self {
                enable: on,
                backlink_html: String::new(),
            });
        }
        let t: Table = crate::de::from_value(&v).map_err(serde::de::Error::custom)?;
        Ok(Self {
            enable: t.enable,
            backlink_html: t.backlink_html,
        })
    }
}

/// `[markup.goldmark.extensions.passthrough]`: delimiters whose content is passed through
/// unrendered (math).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Passthrough {
    pub enable: bool,
    pub delimiters: PassthroughDelimiters,
}

/// Opening and closing delimiter pairs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct PassthroughDelimiters {
    pub inline: Vec<[String; 2]>,
    pub block: Vec<[String; 2]>,
}

/// `[markup.goldmark.extensions.cjk]`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Cjk {
    pub enable: bool,
    pub east_asian_line_breaks: bool,
    /// `simple` or `css3draft`.
    pub east_asian_line_breaks_style: String,
    pub escaped_space: bool,
}

impl Default for Cjk {
    fn default() -> Self {
        Self {
            enable: false,
            east_asian_line_breaks: false,
            east_asian_line_breaks_style: "simple".to_owned(),
            escaped_space: false,
        }
    }
}

/// `[markup.goldmark.extensions.extras]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Extras {
    pub delete: Toggle,
    pub insert: Toggle,
    pub mark: Toggle,
    pub subscript: Toggle,
    pub superscript: Toggle,
}

/// An extension that is off unless enabled.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Toggle {
    pub enable: bool,
}

/// `[markup.goldmark.parser]`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ParserConfig {
    #[serde(rename = "autoHeadingID")]
    pub auto_heading_id: bool,
    /// `autoHeadingIDType` (`github`, `github-ascii`, `blackfriday`).
    #[serde(rename = "autoHeadingIDType", deserialize_with = "anchor_style")]
    pub auto_id_type: AnchorStyle,
    #[serde(rename = "autoDefinitionTermID")]
    pub auto_definition_term_id: bool,
    pub wrap_standalone_image_within_paragraph: bool,
    #[serde(deserialize_with = "attribute")]
    pub attribute: Attribute,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            auto_heading_id: true,
            auto_id_type: AnchorStyle::Github,
            auto_definition_term_id: false,
            wrap_standalone_image_within_paragraph: true,
            attribute: Attribute {
                title: true,
                block: false,
            },
        }
    }
}

/// How heading ids are made from heading text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AnchorStyle {
    Github,
    GithubAscii,
    Blackfriday,
}

impl From<AnchorStyle> for anchor::Style {
    fn from(s: AnchorStyle) -> Self {
        match s {
            AnchorStyle::Github => Self::Github,
            AnchorStyle::GithubAscii => Self::GithubAscii,
            AnchorStyle::Blackfriday => Self::Blackfriday,
        }
    }
}

fn anchor_style<'de, D: Deserializer<'de>>(d: D) -> Result<AnchorStyle, D::Error> {
    let s = String::deserialize(d)?;
    match s.to_ascii_lowercase().as_str() {
        "github" | "" => Ok(AnchorStyle::Github),
        "github-ascii" => Ok(AnchorStyle::GithubAscii),
        "blackfriday" => Ok(AnchorStyle::Blackfriday),
        _ => Err(serde::de::Error::custom(format_args!(
            "unknown heading id style {s:?} (github, github-ascii or blackfriday)"
        ))),
    }
}

/// `[markup.goldmark.parser.attribute]`: where `{.class #id}` attributes are allowed. A
/// boolean is the legacy form for both.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Attribute {
    pub title: bool,
    pub block: bool,
}

impl Default for Attribute {
    fn default() -> Self {
        Self {
            title: true,
            block: false,
        }
    }
}

fn attribute<'de, D: Deserializer<'de>>(d: D) -> Result<Attribute, D::Error> {
    let v = Value::deserialize(d)?;
    if !matches!(v, Value::Map(_))
        && let Some(on) = crate::de::weak_bool(&v)
    {
        return Ok(Attribute {
            title: on,
            block: on,
        });
    }
    crate::de::from_value(&v).map_err(serde::de::Error::custom)
}

/// `[markup.goldmark.renderer]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RendererConfig {
    pub hard_wraps: bool,
    #[serde(rename = "xhtml")]
    pub xhtml: bool,
    /// Raw HTML in Markdown is rendered (otherwise replaced by a comment).
    #[serde(rename = "unsafe")]
    pub unsafe_html: bool,
}

/// `[markup.goldmark.renderHooks]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct RenderHooks {
    pub image: HookConfig,
    pub link: HookConfig,
}

/// When the embedded image or link render hook is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UseEmbedded {
    Always,
    /// `fallback` for multilingual single-host sites, otherwise `never`.
    Auto,
    /// When the project has no hook of its own.
    Fallback,
    Never,
}

/// `[markup.goldmark.renderHooks.image]` / `.link`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookConfig {
    /// `None` until the site default is applied.
    pub use_embedded: Option<UseEmbedded>,
}

impl<'de> Deserialize<'de> for HookConfig {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize, Default)]
        #[serde(default, rename_all = "camelCase")]
        struct Raw {
            use_embedded: Option<String>,
            enable_default: Option<bool>,
        }
        let r = Raw::deserialize(d)?;
        let use_embedded = match (r.use_embedded, r.enable_default) {
            (Some(s), _) => Some(match s.to_ascii_lowercase().as_str() {
                "always" => UseEmbedded::Always,
                "auto" => UseEmbedded::Auto,
                "fallback" => UseEmbedded::Fallback,
                "never" => UseEmbedded::Never,
                _ => {
                    return Err(serde::de::Error::custom(format_args!(
                        "unknown value {s:?} (always, auto, fallback or never)"
                    )));
                }
            }),
            (None, Some(true)) => Some(UseEmbedded::Fallback),
            (None, Some(false)) => Some(UseEmbedded::Never),
            (None, None) => None,
        };
        Ok(Self { use_embedded })
    }
}

/// `[markup.highlight]`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors the highlighter's options"
)]
pub struct HighlightConfig {
    pub style: String,
    pub code_fences: bool,
    pub no_classes: bool,
    pub line_nos: bool,
    pub line_numbers_in_table: bool,
    pub line_no_start: i64,
    pub anchor_line_nos: bool,
    pub line_anchors: String,
    #[serde(rename = "hl_Lines")]
    pub hl_lines: String,
    #[serde(rename = "hl_inline")]
    pub hl_inline: bool,
    pub tab_width: i64,
    pub guess_syntax: bool,
    pub wrapper_class: String,
}

impl Default for HighlightConfig {
    fn default() -> Self {
        Self {
            style: "monokai".to_owned(),
            code_fences: true,
            no_classes: true,
            line_nos: false,
            line_numbers_in_table: true,
            line_no_start: 1,
            anchor_line_nos: false,
            line_anchors: String::new(),
            hl_lines: String::new(),
            hl_inline: false,
            tab_width: 4,
            guess_syntax: false,
            wrapper_class: "highlight".to_owned(),
        }
    }
}

/// `[markup.tableOfContents]`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TocConfig {
    pub start_level: u8,
    /// The deepest heading level listed; `None` (Go's `endLevel = -1`) lists every level
    /// from `start_level` down.
    #[serde(deserialize_with = "de_end_level", serialize_with = "ser_end_level")]
    pub end_level: Option<u8>,
    pub ordered: bool,
}

impl Default for TocConfig {
    fn default() -> Self {
        Self {
            start_level: 2,
            end_level: Some(3),
            ordered: false,
        }
    }
}

/// `-1` is no end level; otherwise a level from 0 to 255.
fn de_end_level<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u8>, D::Error> {
    let level = i64::deserialize(d)?;
    if level == -1 {
        return Ok(None);
    }
    u8::try_from(level).map(Some).map_err(|_| {
        serde::de::Error::custom(format_args!(
            "expected a heading level or -1 (no end level), found {level}"
        ))
    })
}

/// Go's spelling: `-1` for no end level.
#[expect(
    clippy::ref_option,
    reason = "serde's serialize_with passes the field by reference"
)]
fn ser_end_level<S: serde::Serializer>(level: &Option<u8>, s: S) -> Result<S::Ok, S::Error> {
    match level {
        Some(l) => s.serialize_u8(*l),
        None => s.serialize_i8(-1),
    }
}

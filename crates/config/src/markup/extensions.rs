//! The Goldmark extensions: typographer, footnotes, passthrough, CJK and the rest.

use super::*;

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
    pub(super) fn empty() -> Self {
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

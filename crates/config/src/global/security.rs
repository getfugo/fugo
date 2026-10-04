//! The security policy: allowed executables, environment variables and URLs.

use super::*;

/// An allow-list of regular expressions (`[security]`).
#[derive(Clone, Debug)]
pub struct Whitelist {
    patterns: Vec<String>,
    compiled: Vec<regex::Regex>,
}

impl PartialEq for Whitelist {
    fn eq(&self, other: &Self) -> bool {
        self.patterns == other.patterns
    }
}

impl Eq for Whitelist {}

impl Serialize for Whitelist {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.patterns.serialize(s)
    }
}

impl Whitelist {
    pub(super) fn new(patterns: &[&str]) -> Self {
        Self::compile(patterns.iter().map(|&p| p.to_owned()).collect())
            .expect("built-in patterns compile")
    }

    pub(super) fn compile(patterns: Vec<String>) -> Result<Self, regex::Error> {
        let compiled = patterns
            .iter()
            .filter(|p| !p.eq_ignore_ascii_case("none") && !p.is_empty())
            .map(|p| regex::Regex::new(p))
            .collect::<Result<_, _>>()?;
        Ok(Self { patterns, compiled })
    }

    /// Whether nothing is allowed (`"none"` or an empty list).
    #[must_use]
    pub fn is_none(&self) -> bool {
        self.compiled.is_empty()
    }

    /// Whether `s` matches one of the patterns.
    #[must_use]
    pub fn accepts(&self, s: &str) -> bool {
        self.compiled.iter().any(|r| r.is_match(s))
    }

    /// The patterns as configured (`["none"]` when configured so).
    #[must_use]
    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }
}

pub(super) fn whitelist<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Whitelist>, D::Error> {
    let v = <Vec<String> as Deserialize>::deserialize(d)?;
    Whitelist::compile(v)
        .map(Some)
        .map_err(serde::de::Error::custom)
}

/// `[security]`: what templates and resource pipelines may execute, read or fetch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SecurityPolicy {
    /// External programs (`exec.allow`).
    pub exec_allow: Whitelist,
    /// Environment variables passed to them (`exec.osEnv`).
    pub exec_os_env: Whitelist,
    /// Environment variables `getenv` may read (`funcs.getenv`).
    pub getenv: Whitelist,
    /// URLs `resources.GetRemote` may fetch (`http.urls`).
    pub http_urls: Whitelist,
    /// HTTP methods (`http.methods`).
    pub http_methods: Whitelist,
    /// Media types of remote resources (`http.mediaTypes`); empty allows any.
    pub http_media_types: Whitelist,
    pub inline_shortcodes: InlineShortcodes,
}

/// Whether content may define inline shortcodes (`enableInlineShortcodes`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InlineShortcodes {
    #[default]
    Disabled,
    Enabled,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            exec_allow: Whitelist::new(&["^(dart-)?sass(-embedded)?$", "^go$", "^git$", "^npx$"]),
            exec_os_env: Whitelist::new(&[
                r"(?i)^((HTTPS?|NO)_PROXY|PATH(EXT)?|APPDATA|TE?MP|TERM|GO\w+|(XDG_CONFIG_)?HOME|USERPROFILE|SSH_AUTH_SOCK|DISPLAY|LANG|SYSTEMDRIVE)$",
            ]),
            getenv: Whitelist::new(&[concat!("^", ssg_base::env_var!("")), "^CI$"]),
            http_urls: Whitelist::new(&[".*"]),
            http_methods: Whitelist::new(&["(?i)GET|POST"]),
            http_media_types: Whitelist::new(&[]),
            inline_shortcodes: InlineShortcodes::Disabled,
        }
    }
}

impl SecurityPolicy {
    pub(crate) fn decode(m: &Map) -> Result<Self, crate::de::DeError> {
        #[derive(Deserialize, Default)]
        #[serde(default, rename_all = "camelCase")]
        struct Raw {
            exec: Exec,
            funcs: Funcs,
            http: Http,
            enable_inline_shortcodes: bool,
        }
        #[derive(Deserialize, Default)]
        #[serde(default, rename_all = "camelCase")]
        struct Exec {
            #[serde(deserialize_with = "whitelist")]
            allow: Option<Whitelist>,
            #[serde(deserialize_with = "whitelist")]
            os_env: Option<Whitelist>,
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Funcs {
            #[serde(deserialize_with = "whitelist")]
            getenv: Option<Whitelist>,
        }
        #[derive(Deserialize, Default)]
        #[serde(default, rename_all = "camelCase")]
        struct Http {
            #[serde(deserialize_with = "whitelist")]
            urls: Option<Whitelist>,
            #[serde(deserialize_with = "whitelist")]
            methods: Option<Whitelist>,
            #[serde(deserialize_with = "whitelist")]
            media_types: Option<Whitelist>,
        }
        let r: Raw = crate::de::from_map(m)?;
        let d = Self::default();
        Ok(Self {
            exec_allow: r.exec.allow.unwrap_or(d.exec_allow),
            exec_os_env: r.exec.os_env.unwrap_or(d.exec_os_env),
            getenv: r.funcs.getenv.unwrap_or(d.getenv),
            http_urls: r.http.urls.unwrap_or(d.http_urls),
            http_methods: r.http.methods.unwrap_or(d.http_methods),
            http_media_types: r.http.media_types.unwrap_or(d.http_media_types),
            inline_shortcodes: if r.enable_inline_shortcodes {
                InlineShortcodes::Enabled
            } else {
                InlineShortcodes::Disabled
            },
        })
    }
}

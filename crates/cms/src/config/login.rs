//! `[cms.login]`: how editors sign in. The Worker signs them in itself with a GitHub or Google
//! account, or Cloudflare Access signs them in before they reach the editor.

use serde::{Deserialize, Serialize};

use crate::CmsError;

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct RawLogin {
    /// One name, or a list of them.
    provider: Vec<String>,
    team: Option<String>,
    aud: Vec<String>,
}

/// An account the Worker signs people in with (OAuth).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OauthProvider {
    Github,
    Google,
}

/// How editors sign in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Login {
    /// The Worker sends people to sign in with an account of one of the providers (in this
    /// order on the sign-in page), and keeps them signed in with a cookie it signs.
    Oauth { providers: Vec<OauthProvider> },
    /// Cloudflare Access signs people in before they reach the editor, and the Worker checks
    /// the token Access adds to every request.
    CloudflareAccess {
        /// The team domain's URL (`https://<team>.cloudflareaccess.com`), the tokens' issuer.
        team: String,
        /// The Access applications' audience tags; a token must carry one of them.
        aud: Vec<String>,
    },
}

/// The names `cms.login.provider` takes.
const NAMES: &[&str] = &["github", "google", "cloudflare-access"];

impl RawLogin {
    pub(super) fn check(self) -> Result<Login, CmsError> {
        let mut providers = Vec::new();
        let mut access = false;
        for name in &self.provider {
            let provider = match name.trim().to_ascii_lowercase().as_str() {
                "github" => OauthProvider::Github,
                "google" => OauthProvider::Google,
                "cloudflare-access" => {
                    access = true;
                    continue;
                }
                _ => {
                    return Err(CmsError::config(
                        "cms.login.provider",
                        format!(
                            "{name:?} is not supported (supported: {})",
                            NAMES.join(", ")
                        ),
                    ));
                }
            };
            if !providers.contains(&provider) {
                providers.push(provider);
            }
        }
        if providers.is_empty() {
            // Without a provider, `team` and `aud` are Cloudflare Access's (the only sign-in of
            // the editor's first versions).
            if !access && self.team.is_none() && self.aud.is_empty() {
                return Err(CmsError::config(
                    "cms.login.provider",
                    "missing: give \"github\", \"google\" or both (a list), or \"cloudflare-access\" with its team and aud",
                ));
            }
            return self.access();
        }
        if access {
            return Err(CmsError::config(
                "cms.login.provider",
                "\"cloudflare-access\" signs everyone in before they reach the editor: it does not go with github or google",
            ));
        }
        for (key, given) in [
            ("cms.login.team", self.team.is_some()),
            ("cms.login.aud", !self.aud.is_empty()),
        ] {
            if given {
                return Err(CmsError::config(
                    key,
                    "only for provider = \"cloudflare-access\"",
                ));
            }
        }
        Ok(Login::Oauth { providers })
    }

    fn access(self) -> Result<Login, CmsError> {
        let team = self.team.ok_or_else(|| {
            CmsError::config(
                "cms.login.team",
                "missing: give the Cloudflare Access team name (<team>.cloudflareaccess.com)",
            )
        })?;
        let host = team
            .trim()
            .trim_start_matches("https://")
            .trim_end_matches('/');
        let host = if host.contains('.') {
            host.to_ascii_lowercase()
        } else {
            format!("{}.cloudflareaccess.com", host.to_ascii_lowercase())
        };
        if host.is_empty()
            || !host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.'))
        {
            return Err(CmsError::config(
                "cms.login.team",
                format!("{team:?} is not a team name or team domain"),
            ));
        }
        let aud: Vec<String> = self
            .aud
            .into_iter()
            .map(|a| a.trim().to_owned())
            .filter(|a| !a.is_empty())
            .collect();
        if aud.is_empty() {
            return Err(CmsError::config(
                "cms.login.aud",
                "missing: give the Access application's audience (AUD) tag",
            ));
        }
        Ok(Login::CloudflareAccess {
            team: format!("https://{host}"),
            aud,
        })
    }
}

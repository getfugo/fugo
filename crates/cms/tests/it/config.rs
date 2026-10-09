//! `[cms]` decoding and checks.

use ssg_cms::config::{Host, Login, OauthProvider, Workflow};

use crate::support::{cms_toml, load, write_files};

fn settings(toml: &str) -> Result<Option<ssg_cms::CmsConfig>, String> {
    let dir = tempfile::tempdir().expect("tempdir");
    write_files(
        dir.path(),
        &[(
            "config.toml",
            &format!("baseURL = \"https://example.org/\"\n{toml}"),
        )],
    );
    ssg_cms::settings(&load(dir.path(), "production")).map_err(|e| e.to_string())
}

#[test]
fn no_table_no_editor() {
    assert_eq!(settings("title = \"x\"").expect("ok"), None);
}

#[test]
fn defaults_and_normalisation() {
    let c = settings(&cms_toml("")).expect("ok").expect("cms");
    assert_eq!(c.path, "admin");
    assert_eq!(c.workflow, Workflow::Review);
    assert_eq!(c.git.host, Host::Github);
    assert_eq!(c.git.branch, "main");
    assert_eq!(c.git.dir, None);
    assert_eq!(
        c.login,
        Login::CloudflareAccess {
            team: "https://team.cloudflareaccess.com".to_owned(),
            aud: vec!["aud-1".to_owned()],
        }
    );
    assert_eq!(c.max_upload, 10 * 1024 * 1024);
    assert!(c.roles["owner"].publish);
}

#[test]
fn settings_in_any_case_and_a_team_domain() {
    let c = settings(
        r#"
[cms]
Path = "/edit/"
Workflow = "direct"
MaxUpload = 3
Media = "static/uploads"
[cms.git]
Repo = "o/r"
Branch = "release/1.x"
Dir = "site/"
[cms.login]
Team = "https://Team.CloudflareAccess.com/"
Aud = ["a", "b"]
[cms.roles.Writer]
Edit = ["content/**"]
[cms.fields.Price]
Widget = "select"
Options = ["1", "2"]
"#,
    )
    .expect("ok")
    .expect("cms");
    assert_eq!(c.path, "edit");
    assert_eq!(c.workflow, Workflow::Direct);
    assert_eq!(c.max_upload, 3 * 1024 * 1024);
    assert_eq!(c.media.as_deref(), Some("static/uploads"));
    assert_eq!(c.git.branch, "release/1.x");
    assert_eq!(c.git.dir.as_deref(), Some("site/"));
    assert_eq!(
        c.login,
        Login::CloudflareAccess {
            team: "https://team.cloudflareaccess.com".to_owned(),
            aud: vec!["a".to_owned(), "b".to_owned()],
        }
    );
    assert!(c.roles.contains_key("writer"));
    assert_eq!(
        c.fields["price"].options,
        vec!["1".to_owned(), "2".to_owned()]
    );
}

/// `cms_toml`, its Cloudflare Access settings replaced by `login`.
fn with_login(login: &str) -> String {
    cms_toml("").replace("team = \"team\"\naud = \"aud-1\"\n", login)
}

#[test]
fn accounts_the_worker_signs_people_in_with() {
    let login = |toml: &str| settings(&with_login(toml)).expect("ok").expect("cms").login;
    assert_eq!(
        login("provider = \"GitHub\"\n"),
        Login::Oauth {
            providers: vec![OauthProvider::Github]
        }
    );
    assert_eq!(
        login("provider = [\"google\", \"github\", \"google\"]\n"),
        Login::Oauth {
            providers: vec![OauthProvider::Google, OauthProvider::Github]
        }
    );
    assert_eq!(
        login("provider = \"cloudflare-access\"\nteam = \"t\"\naud = \"a\"\n"),
        Login::CloudflareAccess {
            team: "https://t.cloudflareaccess.com".to_owned(),
            aud: vec!["a".to_owned()],
        }
    );
    // The Worker's settings: the kind, and what it needs.
    let json = |l: &Login| serde_json::to_value(l).expect("json");
    assert_eq!(
        json(&login("provider = [\"github\", \"google\"]\n")),
        serde_json::json!({"kind": "oauth", "providers": ["github", "google"]})
    );
    assert_eq!(
        json(&login("team = \"t\"\naud = \"a\"\n")),
        serde_json::json!({"kind": "cloudflare-access", "team": "https://t.cloudflareaccess.com", "aud": ["a"]})
    );
}

#[test]
fn login_errors() {
    let cases = [
        ("", "cms.login.provider: missing"),
        ("provider = \"gitlab\"\n", "\"gitlab\" is not supported"),
        (
            "provider = [\"github\", \"cloudflare-access\"]\n",
            "does not go with github",
        ),
        (
            "provider = \"github\"\nteam = \"t\"\n",
            "cms.login.team: only for provider = \"cloudflare-access\"",
        ),
        (
            "provider = \"google\"\naud = \"a\"\n",
            "cms.login.aud: only for",
        ),
        (
            "provider = \"cloudflare-access\"\n",
            "cms.login.team: missing",
        ),
    ];
    for (toml, want) in cases {
        let err = settings(&with_login(toml)).expect_err(toml);
        assert!(err.contains(want), "{want:?} not in {err:?} for\n{toml}");
    }
}

#[test]
fn errors_name_the_setting() {
    let cases = [
        (
            "[cms]\n[cms.login]\nteam = \"t\"\naud = \"a\"\n[cms.roles.x]\nedit = [\"**\"]",
            "cms.git.repo",
        ),
        (
            &cms_toml("").replace("owner/site", "not-a-repo"),
            "cms.git.repo",
        ),
        (
            &cms_toml("").replace("[cms.git]\n", "[cms.git]\nhost = \"gitlab\"\n"),
            "not supported",
        ),
        (&cms_toml("[cms.git.x]\n"), "unknown field `x`"),
        (
            &cms_toml("").replace("team = \"team\"\n", ""),
            "cms.login.team",
        ),
        (
            &cms_toml("").replace("aud = \"aud-1\"\n", ""),
            "cms.login.aud",
        ),
        (
            &cms_toml("").replace("[cms]\n", "[cms]\nmaxUpload = 30\n"),
            "cms.maxUpload",
        ),
        (
            &cms_toml("").replace("[cms]\n", "[cms]\npath = \"../x\"\n"),
            "cms.path",
        ),
        (
            &cms_toml("").replace("[cms]\n", "[cms]\nworkflw = \"direct\"\n"),
            "workflw",
        ),
        (
            &cms_toml("[cms.roles.bad]\nedit = [\"content/[z-a]\"]"),
            "cms.roles.bad.edit",
        ),
        (
            &cms_toml("[cms.roles.bad]\nedit = [\"../content/**\"]"),
            "relative to the project",
        ),
        (
            &cms_toml("[cms.roles.idle]\npublish = false"),
            "may do nothing",
        ),
        (
            &cms_toml("[cms.fields.flavours]\nmultiple = true"),
            "cms.fields.flavours.multiple",
        ),
        (
            &cms_toml("[cms.fields.rating]\nmin = 5\nmax = 1"),
            "cms.fields.rating.min",
        ),
        (
            &cms_toml("")
                .replace("main", "x")
                .replace("[cms.git]\n", "[cms.git]\nbranch = \"a..b\"\n"),
            "cms.git.branch",
        ),
    ];
    for (toml, want) in cases {
        let err = settings(toml).expect_err(toml);
        assert!(err.contains(want), "{want:?} not in {err:?} for\n{toml}");
    }
}

#[test]
fn no_roles_is_an_error() {
    let err =
        settings("[cms]\n[cms.git]\nrepo = \"o/r\"\n[cms.login]\nteam = \"t\"\naud = \"a\"\n")
            .expect_err("roles");
    assert!(err.contains("cms.roles"), "{err}");
}

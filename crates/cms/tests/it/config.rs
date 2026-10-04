//! `[cms]` decoding and checks.

use ssg_cms::config::{Host, Provider, Workflow};

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
    assert_eq!(c.login.provider, Provider::CloudflareAccess);
    assert_eq!(c.login.team, "https://team.cloudflareaccess.com");
    assert_eq!(c.login.aud, vec!["aud-1".to_owned()]);
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
    assert_eq!(c.login.team, "https://team.cloudflareaccess.com");
    assert_eq!(c.login.aud, vec!["a".to_owned(), "b".to_owned()]);
    assert!(c.roles.contains_key("writer"));
    assert_eq!(
        c.fields["price"].options,
        vec!["1".to_owned(), "2".to_owned()]
    );
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

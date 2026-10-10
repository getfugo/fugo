//! The editor's API on this computer (`ssg_cms::local`): requests answered from a git
//! repository in a temporary directory, as `fugo server` answers them. Without git on `PATH`
//! the tests print `SKIPPED`.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use ssg_cms::local::Call;

use crate::support::{TestSink, load, write_files};

mod direct;
mod review;

const HOST: &str = "localhost:1313";

/// The original of `content/posts/hello.md`.
const HELLO: &str = "---\ntitle: Hello\n---\nOne.\nTwo.\nThree.\n";

/// git in `dir`: its output, trimmed.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Whether git is on `PATH`.
fn has_git() -> bool {
    let found = Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !found {
        eprintln!("SKIPPED local cms tests: no git on PATH");
    }
    found
}

/// A project with `[cms]` in a repository whose branch `main` has one commit, and its editor.
struct Site {
    dir: tempfile::TempDir,
    editor: ssg_cms::Editor,
}

fn site(workflow: &str) -> Site {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = format!(
        r#"baseURL = "https://example.org/"
title = "Snacks"
[cms]
workflow = "{workflow}"
[cms.git]
repo = "owner/site"
[cms.login]
provider = "github"
[cms.roles.writer]
edit = ["content/**"]
[cms.roles.owner]
edit = ["**"]
publish = true
"#
    );
    write_files(
        dir.path(),
        &[
            ("config.toml", &config),
            ("content/_index.md", "---\ntitle: Home\n---\n"),
            ("content/posts/hello.md", HELLO),
            ("content/posts/other.md", "---\ntitle: Other\n---\n"),
        ],
    );
    let d = dir.path();
    git(d, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("user.name", "Ann Tester"),
        ("user.email", "Ann@Example.org"),
        ("commit.gpgsign", "false"),
    ] {
        git(d, &["config", key, value]);
    }
    git(d, &["add", "-A"]);
    git(d, &["commit", "-qm", "init"]);
    let published = ssg_cms::publish(
        &load(d, "production"),
        &TestSink::default(),
        &BTreeMap::new(),
    )
    .expect("publish")
    .expect("cms");
    Site {
        dir,
        editor: published.editor,
    }
}

/// A request from this computer, to `localhost`.
fn local(method: &str, name: &str) -> Call {
    Call {
        method: method.to_owned(),
        name: name.to_owned(),
        host: Some(HOST.to_owned()),
        loopback: true,
        ..Call::default()
    }
}

/// A POST of the editor's page.
fn post_of(name: &str, body: &str) -> Call {
    Call {
        origin: Some(format!("http://{HOST}")),
        content_type: Some("application/json".to_owned()),
        body: body.as_bytes().to_vec(),
        ..local("POST", name)
    }
}

impl Site {
    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn call(&self, call: &Call) -> (u16, Value) {
        let a = self.editor.answer(call);
        (
            a.status,
            serde_json::from_str(&a.body).expect("a JSON answer"),
        )
    }

    fn get(&self, name: &str, query: &str) -> (u16, Value) {
        self.call(&Call {
            query: query.to_owned(),
            ..local("GET", name)
        })
    }

    fn post(&self, name: &str, body: &Value) -> (u16, Value) {
        self.call(&post_of(name, &body.to_string()))
    }

    fn git(&self, args: &[&str]) -> String {
        git(self.path(), args)
    }

    /// A file of the checkout.
    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.path().join(path)).expect(path)
    }

    fn write(&self, path: &str, text: &str) {
        std::fs::write(self.path().join(path), text).expect(path);
    }

    /// The blob of `path` on the branch (`base` of a change).
    fn base(&self, path: &str) -> String {
        let (status, file) = self.get("file", &format!("path={path}"));
        assert_eq!(status, 200, "{file}");
        file["sha"].as_str().expect("sha").to_owned()
    }

    /// Saves `content` to `path` of page `entry` (in draft `draft`).
    fn save(&self, entry: &str, path: &str, content: &str, draft: Option<&str>) -> (u16, Value) {
        let base = self.base(path);
        self.post(
            "save",
            &json!({
                "entry": entry,
                "title": "Hello",
                "draft": draft,
                "changes": [{ "path": path, "base": base, "content": content }],
            }),
        )
    }
}

#[test]
fn answers_this_computer_only() {
    if !has_git() {
        return;
    }
    let s = site("review");
    let status = |c: Call| s.editor.answer(&c).status;
    assert_eq!(status(local("GET", "me")), 200);
    let elsewhere = Call {
        loopback: false,
        ..local("GET", "me")
    };
    assert_eq!(status(elsewhere), 403, "a connection from another computer");
    for host in [
        Some("evil.example:1313"),
        Some("127.0.0.1.evil.example"),
        None,
    ] {
        let rebound = Call {
            host: host.map(str::to_owned),
            ..local("GET", "site")
        };
        assert_eq!(status(rebound), 403, "host {host:?}");
    }
    assert_eq!(status(local("PUT", "me")), 405);
    assert_eq!(status(local("GET", "nothing")), 404);
}

#[test]
fn a_post_comes_from_the_editors_page_as_json() {
    if !has_git() {
        return;
    }
    let s = site("review");
    let status = |c: Call| s.editor.answer(&c).status;
    let body = r#"{"id": "x"}"#;
    for origin in [
        None,
        Some("http://evil.example"),
        Some("https://localhost:1313"),
    ] {
        let c = Call {
            origin: origin.map(str::to_owned),
            ..post_of("discard", body)
        };
        assert_eq!(status(c), 403, "origin {origin:?}");
    }
    let form = Call {
        content_type: Some("text/plain".to_owned()),
        ..post_of("discard", body)
    };
    assert_eq!(status(form), 415);
    assert_eq!(status(post_of("discard", "[1]")), 400);
    assert_eq!(status(post_of("discard", "{")), 400);
    let large = "x".repeat(s.editor.body_limit() + 1);
    assert_eq!(status(post_of("save", &large)), 413);
}

#[test]
fn the_person_is_git_s_identity_with_every_role() {
    if !has_git() {
        return;
    }
    let s = site("review");
    let (status, me) = s.get("me", "");
    assert_eq!(status, 200, "{me}");
    let ident = s.git(&["var", "GIT_AUTHOR_IDENT"]);
    let email = &ident[ident.find('<').expect("<") + 1..ident.find('>').expect(">")];
    assert_eq!(me["email"], email.to_lowercase());
    assert_eq!(me["login"], "local");
    assert_eq!(me["roles"], json!(["owner", "writer"]));
    assert_eq!(me["publish"], true);
    assert_eq!(me["branch"], "main");
    assert_eq!(me["workflow"], "review");
    let (status, index) = s.get("site", "");
    assert_eq!(status, 200);
    assert_eq!(index["title"], "Snacks");
}

#[test]
fn files_are_read_from_the_branch() {
    if !has_git() {
        return;
    }
    let s = site("review");
    // An uncommitted change is not the branch's.
    s.write("content/posts/hello.md", "changed\n");
    let (status, file) = s.get("file", "path=content%2Fposts%2Fhello.md");
    assert_eq!(status, 200, "{file}");
    assert_eq!(file["size"], HELLO.len());
    assert_eq!(
        file["sha"],
        s.git(&["rev-parse", "main:content/posts/hello.md"])
    );
    assert_eq!(s.get("file", "path=content/posts/nothing.md").0, 404);
    assert_eq!(
        s.get("file", "path=content/posts").0,
        403,
        "no extension: not in an area"
    );
    assert_eq!(s.get("file", "path=config.toml").0, 403);
    assert_eq!(s.get("file", "path=content/../config.toml").0, 403);
}

#[test]
fn the_branch_checked_out_is_where_drafts_come_from() {
    if !has_git() {
        return;
    }
    let s = site("review");
    s.git(&["checkout", "-q", "-b", "next"]);
    assert_eq!(s.get("me", "").1["branch"], "next");
    s.git(&["checkout", "-q", "-b", "cms/x"]);
    assert_eq!(s.get("me", "").0, 409, "a draft checked out");
    s.git(&["checkout", "-q", "--detach"]);
    let (status, e) = s.get("me", "");
    assert_eq!(status, 409, "{e}");
    assert!(
        e["error"]
            .as_str()
            .is_some_and(|m| m.contains("check out a branch"))
    );
}

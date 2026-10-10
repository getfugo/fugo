//! Workflow `direct`: a save is a commit on the branch checked out, and its files; and the
//! checks of a save's changes.

use serde_json::json;

use super::{HELLO, has_git, site};

#[test]
fn a_save_is_a_commit_on_the_checkout() {
    if !has_git() {
        return;
    }
    let s = site("direct");
    assert_eq!(s.get("me", "").1["publish"], false, "nothing to publish");
    let (status, saved) = s.save("posts/hello", "content/posts/hello.md", "new\n", None);
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["draft"], json!(null));
    assert_eq!(s.git(&["rev-parse", "HEAD"]), saved["commit"]);
    assert_eq!(s.read("content/posts/hello.md"), "new\n");
    assert_eq!(s.git(&["status", "--porcelain"]), "");
    assert_eq!(
        s.git(&["log", "-1", "--format=%an <%ae>%n%B"]),
        "Ann Tester <ann@example.org>\nEdit posts/hello: Hello\n\nCMS-Entry: posts/hello\nCMS-Title: Hello"
    );
    let (status, e) = s.post("publish", &json!({ "id": "x" }));
    assert_eq!(status, 400, "{e}");
}

#[test]
fn a_move_and_deletions_are_one_commit() {
    if !has_git() {
        return;
    }
    let s = site("direct");
    let (status, saved) = s.post(
        "save",
        &json!({
            "entry": "posts/hello",
            "changes": [
                { "path": "content/posts/hi.md", "from": "content/posts/hello.md" },
                { "path": "content/posts/hello.md", "delete": true },
                { "path": "content/posts/other.md", "delete": true },
            ],
        }),
    );
    assert_eq!(status, 200, "{saved}");
    assert_eq!(s.read("content/posts/hi.md"), HELLO);
    assert!(!s.path().join("content/posts/hello.md").exists());
    assert!(!s.path().join("content/posts/other.md").exists());
    assert_eq!(s.git(&["log", "-1", "--format=%s"]), "Move posts/hello");
    assert_eq!(s.git(&["status", "--porcelain"]), "");

    let (status, e) = s.post(
        "save",
        &json!({ "entry": "posts/hi", "changes": [{ "path": "content/posts/x.md", "from": "content/posts/hi.md" }] }),
    );
    assert_eq!(status, 400, "a move keeps no copy: {e}");
    let (status, e) = s.post(
        "save",
        &json!({ "entry": "posts/hi", "changes": [
            { "path": "content/posts/y.md", "from": "content/posts/gone.md" },
            { "path": "content/posts/gone.md", "delete": true },
        ] }),
    );
    assert_eq!(
        (status, &e["stale"]),
        (409, &json!(["content/posts/gone.md"])),
        "{e}"
    );
}

#[test]
fn changes_are_checked() {
    if !has_git() {
        return;
    }
    let s = site("direct");
    let save = |changes: serde_json::Value| {
        s.post(
            "save",
            &json!({ "entry": "posts/hello", "changes": changes }),
        )
        .0
    };
    let write = |path: &str| json!({ "path": path, "content": "x" });
    assert_eq!(save(json!([write("layouts/single.html")])), 403);
    assert_eq!(save(json!([write("config.toml")])), 403);
    assert_eq!(save(json!([write("content/.hidden.md")])), 400);
    assert_eq!(save(json!([write("/content/a.md")])), 400);
    assert_eq!(
        save(json!([write("content/a.md"), write("content/a.md")])),
        400
    );
    assert_eq!(save(json!([{ "path": "content/a.md" }])), 400, "no content");
    assert_eq!(
        save(json!([{ "path": "content/a.png", "content": "a-b", "encoding": "base64" }])),
        400
    );
    assert_eq!(save(json!([])), 400);
    let many: Vec<_> = (0..31).map(|i| write(&format!("content/{i}.md"))).collect();
    assert_eq!(save(json!(many)), 413);
    let (status, _) = s.post(
        "save",
        &json!({ "entry": " ", "changes": [write("content/a.md")] }),
    );
    assert_eq!(status, 400, "no page");
    assert_eq!(
        s.git(&["rev-list", "--count", "HEAD"]),
        "1",
        "nothing was committed"
    );
}

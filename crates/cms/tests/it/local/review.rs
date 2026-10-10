//! Workflow `review`: a save goes to the page's draft, a local branch `cms/<id>`, until it is
//! published onto the branch checked out (and its files).

use serde_json::json;

use super::{HELLO, has_git, site};

const EDITED: &str = "---\ntitle: Hello\n---\nOne!\nTwo.\nThree.\n";

#[test]
fn a_save_is_a_draft_until_it_is_published() {
    if !has_git() {
        return;
    }
    let s = site("review");
    let base = s.base("content/posts/hello.md");
    let (status, saved) = s.post(
        "save",
        &json!({
            "entry": "posts/hello",
            "title": "Hello",
            "changes": [
                { "path": "content/posts/hello.md", "base": base, "content": EDITED },
                { "path": "content/posts/dot.png", "base": null, "content": "iVBORw0K\nGgo=", "encoding": "base64" },
            ],
        }),
    );
    assert_eq!(status, 200, "{saved}");
    let id = saved["draft"].as_str().expect("draft id").to_owned();
    assert!(id.starts_with("posts-hello-"), "{id}");
    // Neither the branch nor the checkout changed.
    assert_eq!(s.read("content/posts/hello.md"), HELLO);
    assert_eq!(s.git(&["status", "--porcelain"]), "");
    assert_eq!(s.git(&["rev-list", "--count", "main"]), "1");
    assert_eq!(s.git(&["rev-parse", &format!("cms/{id}")]), saved["commit"]);

    let (_, drafts) = s.get("drafts", "");
    let d = &drafts["drafts"][0];
    assert_eq!(
        (&d["id"], &d["entry"], &d["title"]),
        (&json!(id), &json!("posts/hello"), &json!("Hello"))
    );
    assert_eq!(
        d["author"],
        json!({ "name": "Ann Tester", "email": "ann@example.org" })
    );
    assert!(
        d["updated"].as_str().is_some_and(|t| t.ends_with('Z')),
        "{d}"
    );

    let (status, file) = s.get("file", &format!("path=content/posts/hello.md&draft={id}"));
    assert_eq!(status, 200, "{file}");
    assert_eq!(file["size"], EDITED.len());

    let (_, draft) = s.get("draft", &format!("id={id}"));
    let files: Vec<(&str, &str)> = draft["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| {
            (
                f["path"].as_str().unwrap_or(""),
                f["status"].as_str().unwrap_or(""),
            )
        })
        .collect();
    assert_eq!(
        files,
        [
            ("content/posts/dot.png", "added"),
            ("content/posts/hello.md", "modified")
        ]
    );
    assert!(
        draft["files"][1]["patch"]
            .as_str()
            .is_some_and(|p| p.starts_with("@@") && p.contains("-One.\n+One!")),
        "{draft}"
    );
    assert_eq!(
        draft["commits"],
        json!([{ "author": { "name": "Ann Tester", "email": "ann@example.org" }, "subject": "Edit posts/hello: Hello" }])
    );
    assert_eq!(draft["conflicts"], json!([]));

    // A save that started from the branch's file, which the draft changed since.
    let stale = json!({
        "entry": "posts/hello",
        "draft": id,
        "changes": [{ "path": "content/posts/hello.md", "base": base, "content": "x" }],
    });
    let (status, e) = s.post("save", &stale);
    assert_eq!(
        (status, &e["stale"]),
        (409, &json!(["content/posts/hello.md"])),
        "{e}"
    );

    let (status, published) = s.post("publish", &json!({ "id": id }));
    assert_eq!(status, 200, "{published}");
    assert_eq!(published["published"], true);
    assert_eq!(s.read("content/posts/hello.md"), EDITED);
    let png = std::fs::read(s.path().join("content/posts/dot.png")).expect("png");
    assert_eq!(png, b"\x89PNG\r\n\x1a\n");
    assert_eq!(s.git(&["status", "--porcelain"]), "");
    assert_eq!(s.git(&["rev-parse", "HEAD"]), published["commit"]);
    assert_eq!(
        s.git(&["log", "-1", "--format=%an <%ae>%n%B"]),
        "Ann Tester <ann@example.org>\nPublish posts/hello: Hello\n\nCMS-Entry: posts/hello\nCMS-Title: Hello\nCMS-Published-By: ann@example.org"
    );
    assert_eq!(
        s.git(&["branch", "--list", "cms/*"]),
        "",
        "the draft is gone"
    );
    assert_eq!(s.get("drafts", "").1, json!({ "drafts": [] }));
}

#[test]
fn the_branch_is_merged_into_a_draft_before_it_is_published() {
    if !has_git() {
        return;
    }
    let s = site("review");
    let (status, saved) = s.save("posts/hello", "content/posts/hello.md", EDITED, None);
    assert_eq!(status, 200, "{saved}");
    let id = saved["draft"].as_str().expect("id").to_owned();
    s.write("content/posts/hello.md", &HELLO.replace("Three.", "Three!"));
    s.git(&["commit", "-qam", "Three on main"]);

    let (_, draft) = s.get("draft", &format!("id={id}"));
    assert_eq!(draft["conflicts"], json!(["content/posts/hello.md"]));
    let (status, published) = s.post("publish", &json!({ "id": id }));
    assert_eq!(status, 200, "{published}");
    assert_eq!(
        s.read("content/posts/hello.md"),
        EDITED.replace("Three.", "Three!")
    );
    assert_eq!(
        s.git(&["log", "--format=%s", "main"]),
        "Publish posts/hello: Hello\nThree on main\ninit"
    );
    assert_eq!(s.git(&["status", "--porcelain"]), "");
}

#[test]
fn changes_to_the_same_lines_are_not_published() {
    if !has_git() {
        return;
    }
    let s = site("review");
    let (_, saved) = s.save("posts/hello", "content/posts/hello.md", EDITED, None);
    let id = saved["draft"].as_str().expect("id").to_owned();
    s.write("content/posts/hello.md", &HELLO.replace("One.", "One?"));
    s.git(&["commit", "-qam", "One on main"]);
    let main = s.git(&["rev-parse", "main"]);
    let (status, e) = s.post("publish", &json!({ "id": id }));
    assert_eq!(status, 409, "{e}");
    assert_eq!(e["conflicts"], json!(["content/posts/hello.md"]));
    assert_eq!(s.git(&["rev-parse", "main"]), main);
    assert_eq!(
        s.get("drafts", "").1["drafts"][0]["id"],
        json!(id),
        "the draft stays"
    );
}

#[test]
fn publishing_leaves_the_checkout_s_own_changes() {
    if !has_git() {
        return;
    }
    let s = site("review");
    let (_, saved) = s.save("posts/hello", "content/posts/hello.md", EDITED, None);
    let id = saved["draft"].as_str().expect("id").to_owned();
    let main = s.git(&["rev-parse", "main"]);

    // An uncommitted change of a file the draft changes: nothing is published.
    s.write("content/posts/hello.md", "mine\n");
    let (status, e) = s.post("publish", &json!({ "id": id }));
    assert_eq!(status, 409, "{e}");
    assert!(
        e["error"]
            .as_str()
            .is_some_and(|m| m.contains("content/posts/hello.md")),
        "{e}"
    );
    assert_eq!(s.git(&["rev-parse", "main"]), main);
    assert_eq!(s.read("content/posts/hello.md"), "mine\n");

    // An uncommitted change of another file stays as it is.
    s.git(&["checkout", "--", "content/posts/hello.md"]);
    s.write("content/posts/other.md", "mine\n");
    let (status, published) = s.post("publish", &json!({ "id": id }));
    assert_eq!(status, 200, "{published}");
    assert_eq!(s.read("content/posts/hello.md"), EDITED);
    assert_eq!(s.read("content/posts/other.md"), "mine\n");
    assert_eq!(
        s.git(&["status", "--porcelain"]),
        "M content/posts/other.md"
    );
}

#[test]
fn saves_go_to_the_draft_they_were_opened_from() {
    if !has_git() {
        return;
    }
    let s = site("review");
    let (_, first) = s.save("posts/hello", "content/posts/hello.md", EDITED, None);
    let id = first["draft"].as_str().expect("id").to_owned();
    // The other page, saved into the first one's draft.
    let (status, second) = s.save(
        "posts/other",
        "content/posts/other.md",
        "other\n",
        Some(&id),
    );
    assert_eq!((status, &second["draft"]), (200, &json!(id)), "{second}");
    let (_, draft) = s.get("draft", &format!("id={id}"));
    assert_eq!(draft["files"].as_array().map(Vec::len), Some(2));
    assert_eq!(draft["entry"], "posts/other", "the last save's page");

    let (status, gone) = s.post("discard", &json!({ "id": id }));
    assert_eq!((status, &gone["discarded"]), (200, &json!(id)));
    assert_eq!(s.git(&["branch", "--list", "cms/*"]), "");
    assert_eq!(s.post("discard", &json!({ "id": id })).0, 404);
    assert_eq!(s.post("publish", &json!({ "id": "../main" })).0, 400);
    assert_eq!(s.git(&["rev-list", "--count", "main"]), "1");
}

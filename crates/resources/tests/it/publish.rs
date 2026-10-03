//! Publishing: eager bundles, URL tokens in every escaped form (paths with `&` and `'` like the
//! real-site-like `herrs-salt-&-vinegar` and `Lay's` bundles), absolute and
//! protocol-relative tokens, the `publish` filter, `Never`, and repeated publishing.

use std::fs;

use ssg_base::{Idx, LangIdx};
use ssg_resources::{BundleResource, PublishPolicy};

use crate::support::{MemSink, store};

#[test]
fn tokens_in_every_form() {
    let tmp = tempfile::tempdir().unwrap();
    let site = tmp.path().join("site");
    fs::create_dir_all(site.join("content/b/herrs-salt-&-vinegar")).unwrap();
    fs::create_dir_all(site.join("assets/css")).unwrap();
    fs::write(
        site.join("config.toml"),
        "baseURL = \"https://shop.example/sub/\"\n",
    )
    .unwrap();
    fs::write(site.join("assets/css/a b.css"), "a{}").unwrap();
    fs::write(site.join("assets/css/unused.css"), "u{}").unwrap();
    fs::write(site.join("assets/css/marked.css"), "m{}").unwrap();
    let bundle = site.join("content/b/herrs-salt-&-vinegar");
    for f in [
        "Lay's.txt",
        "eager.txt",
        "never.txt",
        "plain.txt",
        "abs.txt",
        "proto.txt",
    ] {
        fs::write(bundle.join(f), f).unwrap();
    }
    let s = store(&site, &tmp.path().join("home"));
    let l = LangIdx::from_index(0);
    let reg = |name: &str, policy| {
        s.register_bundle(&BundleResource {
            lang: l,
            file: bundle.join(name),
            name: name.to_owned(),
            dir: "/b/herrs-salt-&-vinegar".to_owned(),
            policy,
        })
    };
    let lays = reg("Lay's.txt", PublishPolicy::OnReference);
    reg("eager.txt", PublishPolicy::Eager);
    let never = reg("never.txt", PublishPolicy::Never);
    reg("plain.txt", PublishPolicy::OnReference);
    reg("abs.txt", PublishPolicy::OnReference);
    reg("proto.txt", PublishPolicy::OnReference);
    let css = s.get_asset(l, "css/a b.css").unwrap().unwrap();
    s.get_asset(l, "css/unused.css").unwrap().unwrap();
    let marked = s.get_asset(l, "css/marked.css").unwrap().unwrap();
    s.mark_published(marked);
    s.mark_published(never);

    // Links escape `'` like Go's path escaping; `&` stays.
    assert_eq!(
        s.resource(lays).rel_permalink,
        "/sub/b/herrs-salt-&-vinegar/Lay%27s.txt"
    );
    assert_eq!(s.resource(css).rel_permalink, "/sub/css/a%20b.css");
    assert_eq!(
        s.resolve_token("/sub/b/herrs-salt-&amp;-vinegar/Lay&#39;s.txt"),
        [lays]
    );

    let tokens = [
        // HTML attribute, escaped by the template engine.
        "/sub/b/herrs-salt-&amp;-vinegar/Lay&#39;s.txt",
        // JSON (a search index), escaped by the JSON encoder.
        "\\/sub\\/b\\/herrs-salt-\\u0026-vinegar\\/plain.txt",
        // Absolute, percent-encoded differently, with a query.
        "https://shop.example/sub/b/herrs-salt-%26-vinegar/abs.txt?v=2",
        "//shop.example/sub/b/herrs-salt-&-vinegar/proto.txt#x",
        "/sub/css/a%20b.css",
        "/sub/b/herrs-salt-&-vinegar/never.txt",
        // Not resources.
        "/sub/css/missing.css",
        "css/unused.css",
        "https://other.example/sub/css/unused.css",
    ];
    let sink = MemSink::default();
    let stats = s.publish(tokens, &sink).unwrap();
    let files: Vec<String> = sink.0.lock().unwrap().keys().cloned().collect();
    assert_eq!(
        files,
        [
            "b/herrs-salt-&-vinegar/Lay's.txt",
            "b/herrs-salt-&-vinegar/abs.txt",
            "b/herrs-salt-&-vinegar/eager.txt",
            "b/herrs-salt-&-vinegar/plain.txt",
            "b/herrs-salt-&-vinegar/proto.txt",
            "css/a b.css",
            "css/marked.css",
        ]
    );
    assert_eq!(stats.files, 7);
    assert_eq!(stats.resolved, 6);

    // A second call writes only what is new.
    let again = MemSink::default();
    let stats = s
        .publish(["/sub/css/unused.css", "/sub/css/a%20b.css"], &again)
        .unwrap();
    assert_eq!(stats.files, 1);
    assert_eq!(
        again.0.lock().unwrap().keys().collect::<Vec<_>>(),
        ["css/unused.css"]
    );
}

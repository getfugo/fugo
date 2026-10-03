//! `paths` and the URL string helpers against the `common/paths/strings` oracle, plus the
//! path newtypes.

use serde_json::Value as J;
use ssg_base::paths;
use ssg_base::url::{self, BaseUrl};
use ssg_base::{ContentKey, OutputPath, Permalink, TermKey, UrlPath};

use crate::support::{Tally, fixture, text};

/// An expected result: bytes, or `Err` for a Go error or panic (texts are not compared).
fn expected(v: &J) -> Result<Vec<u8>, ()> {
    match v {
        J::String(s) => Ok(s.as_bytes().to_vec()),
        J::Object(o) if o.contains_key("$nh:bytes") => {
            let hex = o["$nh:bytes"].as_str().unwrap();
            Ok((0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect())
        }
        J::Object(o) if o.contains_key("panic") || o.contains_key("err") => Err(()),
        other => panic!("unexpected fixture value {other}"),
    }
}

fn check_str(t: &mut Tally, input: &str, name: &str, want: &J, got: Result<String, ()>) {
    check_bytes(t, input, name, want, &got.map(String::into_bytes));
}

fn check_bytes(t: &mut Tally, input: &str, name: &str, want: &J, got: &Result<Vec<u8>, ()>) {
    let w = expected(want);
    t.check(w == *got, || {
        format!(
            "{name}({input:?}): want {want}, got {:?}",
            got.as_ref()
                .map(|b| String::from_utf8_lossy(b).into_owned())
        )
    });
}

fn pair(a: &str, b: &str) -> J {
    J::Array(vec![J::from(a), J::from(b)])
}

#[test]
fn strings_oracle() {
    let f = fixture("oracle/common/paths/strings.json.gz");
    let roots: Vec<&str> = f["contextRoots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let hosts: Vec<&str> = f["permalinkHosts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let mut t = Tally::new("paths/strings");
    for c in f["cases"].as_array().unwrap() {
        let Some(s) = text(&c["in"]) else {
            t.skip(|| format!("{}: input is not UTF-8", c["in"]));
            continue;
        };
        check_str(
            &mut t,
            s,
            "Sanitize",
            &c["Sanitize"],
            Ok(paths::sanitize(s)),
        );
        check_str(
            &mut t,
            s,
            "MakeTitle",
            &c["MakeTitle"],
            Ok(paths::make_title(s)),
        );
        check_str(
            &mut t,
            s,
            "PathEscape",
            &c["PathEscape"],
            url::path_escape(s).map_err(drop),
        );
        check_str(
            &mut t,
            s,
            "URLEscape",
            &c["URLEscape"],
            url::url_escape(s).map_err(drop),
        );
        check_str(
            &mut t,
            s,
            "NormalizePathStringBasic",
            &c["NormalizePathStringBasic"],
            Ok(paths::normalize_key(s)),
        );
        if c.get("textOnly").is_some() {
            continue;
        }
        let eq = |t: &mut Tally, name: &str, want: &J, got: J| {
            t.check(*want == got, || {
                format!("{name}({s:?}): want {want}, got {got}")
            });
        };
        eq(&mut t, "Ext", &c["Ext"], J::from(paths::ext(s)));
        eq(
            &mut t,
            "ExtNoDelimiter",
            &c["ExtNoDelimiter"],
            J::from(paths::ext_no_delimiter(s)),
        );
        let (name, ext) = paths::file_and_ext(s);
        eq(&mut t, "FileAndExt", &c["FileAndExt"], pair(name, ext));
        eq(&mut t, "PathAndExt", &c["PathAndExt"], pair(name, ext));
        eq(
            &mut t,
            "FileAndExtNoDelimiter",
            &c["FileAndExtNoDelimiter"],
            pair(name, ext.strip_prefix('.').unwrap_or(ext)),
        );
        eq(
            &mut t,
            "Filename",
            &c["Filename"],
            J::from(paths::filename(s)),
        );
        eq(&mut t, "Dir", &c["Dir"], J::from(paths::dir(s)));
        eq(
            &mut t,
            "TrimExt",
            &c["TrimExt"],
            J::from(paths::trim_ext(s)),
        );
        eq(
            &mut t,
            "PrettifyURLPath",
            &c["PrettifyURLPath"],
            J::from(paths::prettify_url_path(s)),
        );
        eq(
            &mut t,
            "PrettifyURL",
            &c["PrettifyURL"],
            J::from(paths::prettify_url(s)),
        );
        eq(&mut t, "Uglify", &c["Uglify"], J::from(paths::uglify(s)));
        for (i, root) in roots.iter().enumerate() {
            check_bytes(
                &mut t,
                s,
                &format!("AddContextRoot[{root}]"),
                &c["AddContextRoot"][i],
                &url::add_context_root(root, s).map_err(drop),
            );
        }
        for (i, host) in hosts.iter().enumerate() {
            check_str(
                &mut t,
                s,
                &format!("MakePermalink[{host}]"),
                &c["MakePermalink"][i],
                url::make_permalink(host, s)
                    .map(|u| u.to_string())
                    .map_err(drop),
            );
        }
    }
    t.finish();
}

#[test]
fn clean_and_join() {
    for (input, want) in [
        ("", "."),
        ("/", "/"),
        ("a/../..", ".."),
        ("/../a", "/a"),
        ("a//b/./c/", "a/b/c"),
        ("../../a/b/..", "../../a"),
    ] {
        assert_eq!(paths::clean(input), want, "clean({input:?})");
    }
    assert_eq!(paths::join(&["", ""]), "");
    assert_eq!(paths::join(&["a", "", "/b/"]), "a/b");
    assert_eq!(paths::join(&["/", "x"]), "/x");
}

#[test]
fn newtypes() {
    let k = ContentKey::from_source("/Posts/My Post/");
    assert_eq!(k.as_str(), "posts/my-post");
    assert_eq!(k.to_path(), "/posts/my-post");
    assert_eq!(k.first_segment(), "posts");
    assert_eq!(k.segments().collect::<Vec<_>>(), vec!["posts", "my-post"]);
    let parent = k.parent().unwrap();
    assert_eq!(parent.as_str(), "posts");
    assert_eq!(parent.parent(), Some(ContentKey::home()));
    assert_eq!(ContentKey::home().parent(), None);
    assert_eq!(ContentKey::home().to_path(), "/");
    assert!(k.starts_with_segments(&parent));
    assert!(k.starts_with_segments(&ContentKey::home()));
    assert!(!ContentKey::from_source("postscript").starts_with_segments(&parent));
    // Nothing but case and spaces changes.
    assert_eq!(
        ContentKey::from_source("a/C++ & Ümlaut").as_str(),
        "a/c++-&-ümlaut"
    );

    assert_eq!(
        TermKey::new("Tags", "Rust Lang").as_str(),
        "/tags/rust-lang"
    );
    assert_eq!(TermKey::new("tags", "c++").as_str(), "/tags/c++");

    assert_eq!(
        OutputPath::new("posts/one/index.html").as_str(),
        "/posts/one/index.html"
    );
    assert_eq!(OutputPath::new("/a/../../b.html").as_str(), "/b.html");
    assert_eq!(OutputPath::new("/a/b.html").relative(), "a/b.html");

    let u = UrlPath::new("posts/my post/");
    assert_eq!(u.as_str(), "/posts/my post/");
    assert_eq!(u.escaped(), "/posts/my%20post/");
    assert_eq!(
        UrlPath::new("/ภาษา/").escaped(),
        "/%E0%B8%A0%E0%B8%B2%E0%B8%A9%E0%B8%B2/"
    );

    let base = BaseUrl::parse("https://example.org/docs/").unwrap();
    assert_eq!(
        Permalink::new(&base, &u).as_str(),
        "https://example.org/docs/posts/my%20post/"
    );
    assert_eq!(
        Permalink::from_escaped(&base, "/q?x=1/").as_str(),
        "https://example.org/docs/q?x=1/"
    );
    assert_eq!(
        Permalink::from_escaped(&base, "a%20b/").as_str(),
        "https://example.org/docs/a%20b/"
    );
}

/// `UrlPath::escaped` takes `%XX` as escapes, like Go's `EscapedPath` of the parsed path.
#[test]
fn url_path_escapes_like_go() {
    let esc = |p: &str| UrlPath::new(p).escaped();
    // A valid escaping is kept as written (hex case included).
    assert_eq!(esc("/a%20b/"), "/a%20b/");
    assert_eq!(esc("/a%2Fb/"), "/a%2Fb/");
    assert_eq!(esc("/a%2fb/"), "/a%2fb/");
    assert_eq!(esc("/a%E0%B8%A0/"), "/a%E0%B8%A0/");
    // Otherwise the escapes are decoded and the whole path escaped.
    assert_eq!(esc("/a b%2F/"), "/a%20b//");
    assert_eq!(esc("/ภ%e0%b8%a0/"), "/%E0%B8%A0%E0%B8%A0/");
    // A `%` that starts no escape is a literal percent sign.
    assert_eq!(esc("/100%/"), "/100%25/");
    assert_eq!(esc("/50%off/"), "/50%25off/");
    // Without any `%`, sub-delimiters Go's path encoder escapes are escaped.
    assert_eq!(
        esc("/herrs-salt-&-vinegar/Lay's.txt"),
        "/herrs-salt-&-vinegar/Lay%27s.txt"
    );
    // A query mark is part of the path.
    assert_eq!(esc("/q?x=1/"), "/q%3Fx=1/");
}

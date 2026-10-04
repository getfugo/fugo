//! The manifest extractor's parts: the HTML and XML scans, the URL normalisation, the L1 path
//! normalisation, txtar archives and the JSON text of the harness's files.

use pretty_assertions::assert_eq;
use ssg_dev::manifest::{Kind, image_info, norm_path, page_url};
use ssg_dev::scan::{self, RelLink};
use ssg_dev::urls::SiteUrls;
use ssg_dev::{json, txtar};

#[test]
fn html_scan() {
    let page = scan::html(concat!(
        "<!DOCTYPE html><html><head><title> A  &amp; B </title>",
        "<link rel=\"canonical\" href=\"https://example.org/a/\">",
        "<link rel=\"alternate\" hreflang=\"th\" type=\"text/html\" href=\"/th/a/\">",
        "<style>p { content: \"<a href=/no/>\" }</style>",
        "<script>let s = '<a href=/no/>';</script></head>",
        "<body><h1 id=\"top\">Title</h1><p>one<b>two</b></p>",
        "<noscript><a href=\"/noscript/\">x</a></noscript>",
        "<pre><code><span>fn</span><span> main</span></code></pre>",
        "<img src=\"/i.png\" srcset=\"/i-1x.png 1x, /i-2x.png 2x\">",
        "<textarea><a href=/no/></textarea>",
        "<p>“quoted” – dash… </p></body></html>",
    ));
    assert_eq!(page.title, ["A & B"]);
    assert_eq!(
        page.rel_links,
        [
            RelLink {
                rel: "canonical",
                href: "https://example.org/a/".into(),
                hreflang: String::new(),
                media_type: String::new()
            },
            RelLink {
                rel: "alternate",
                href: "/th/a/".into(),
                hreflang: "th".into(),
                media_type: "text/html".into()
            },
        ]
    );
    assert_eq!(
        page.urls,
        [
            "https://example.org/a/",
            "/th/a/",
            "/noscript/",
            "/i.png",
            "/i-1x.png",
            "/i-2x.png"
        ]
    );
    assert_eq!(page.ids, ["top"]);
    assert_eq!(page.refresh, None);
    assert_eq!(
        page.visible_text(),
        "A & B Title one two x fn main <a href=/no/> \"quoted\" - dash..."
    );
}

#[test]
fn alias_page() {
    let page = scan::html(
        "<!DOCTYPE html><html><head><title>/to/</title><meta http-equiv=\"refresh\" content=\"0; url=/to/\"></head></html>",
    );
    assert_eq!(page.refresh.as_deref(), Some("/to/"));
}

#[test]
fn xml_scan() {
    let items = scan::xml(concat!(
        "<?xml version=\"1.0\"?><rss xmlns:atom=\"http://www.w3.org/2005/Atom\"><channel>",
        "<link>https://example.org/</link>",
        "<atom:link href=\"https://example.org/index.xml\" rel=\"self\"/>",
        "<item><link> https://example.org/a?x=1&amp;y=2 </link><guid>https://example.org/a</guid>",
        "<description><![CDATA[<a href=\"/no/\">]]></description></item>",
        "<loc/></channel></rss>",
    ));
    let want = [
        ("link", "https://example.org/"),
        ("atom:link href", "https://example.org/index.xml"),
        ("link", "https://example.org/a?x=1&y=2"),
        ("guid", "https://example.org/a"),
        ("loc", ""),
    ];
    assert_eq!(items, want.map(|(k, v)| (k.to_owned(), v.to_owned())));
}

#[test]
fn site_urls() {
    let urls = SiteUrls::new(&[
        "https://example.org/docs/".into(),
        "https://example.org/th/".into(),
    ])
    .expect("base URLs");
    let page = "/a/b/";
    let cases = [
        ("https://example.org/docs/x/", Some("/x/")),
        ("https://EXAMPLE.org/docs/x/?q=1#f", Some("/x/?q=1#f")),
        ("//example.org/docs/y", Some("/y")),
        ("https://other.org/docs/x/", None),
        ("/docs/z/", Some("/z/")),
        ("/th/%E0%B8%82%E0%B8%99%E0%B8%A1/", Some("/ขนม/")),
        ("../c/", Some("/a/c/")),
        ("d.html", Some("/a/b/d.html")),
        ("#top", Some("/a/b/#top")),
        ("/css/app.0123456789abcdef0123.css", Some("/css/app.H.css")),
        ("mailto:a@example.org", None),
        ("", None),
    ];
    for (raw, want) in cases {
        assert_eq!(urls.internal(raw, page).as_deref(), want, "{raw}");
    }
    assert_eq!(
        urls.any("https://other.org/%E0%B8%82", page),
        "https://other.org/ข"
    );
}

#[test]
fn paths() {
    assert_eq!(norm_path("img/a_hu_3f2a9c.png"), "img/a_hu_H.png");
    assert_eq!(norm_path("js/main.0123456789abcdef.js"), "js/main.H.js");
    assert_eq!(
        norm_path("a.0123456789abcdef.0123456789abcdef.css"),
        "a.H.H.css"
    );
    assert_eq!(norm_path("a.abc.css"), "a.abc.css");
    assert_eq!(page_url("index.html"), "/");
    assert_eq!(page_url("a/index.html"), "/a/");
    assert_eq!(page_url("a/b.html"), "/a/b.html");
    assert_eq!(Kind::of("a/_redirects"), Kind::Lines);
    assert_eq!(Kind::of("site.webmanifest"), Kind::Json);
    assert_eq!(Kind::of("A.PNG"), Kind::Image);
}

#[test]
fn images() {
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbImage::new(3, 2)
        .write_to(&mut png, image::ImageFormat::Png)
        .expect("a PNG");
    assert_eq!(image_info(png.get_ref()), Some((3, 2, "png".into())));
    assert_eq!(
        image_info(b"\0\0\x01\0\x01\0\x10\0"),
        Some((16, 256, "ico".into()))
    );
    assert_eq!(image_info(b"not an image"), None);
}

#[test]
fn txtar_archives() {
    let files = txtar::parse(
        "comment\n-- a.txt --\none\n\n-- dir/b.txt --\ntwo\n-- empty --\n-- last --\nno newline",
    );
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(
        files,
        [
            ("a.txt", "one\n\n"),
            ("dir/b.txt", "two\n"),
            ("empty", ""),
            ("last", "no newline\n")
        ]
    );
}

#[test]
fn json_text() {
    let v = serde_json::json!({"b": [1, {"y": null, "x": "ข"}], "a": true});
    assert_eq!(
        json::line(&v),
        r#"{"a": true, "b": [1, {"x": "ข", "y": null}]}"#
    );
    assert_eq!(json::string("a\"b\n"), r#""a\"b\n""#);
}

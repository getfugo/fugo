//! The embedded templates (`ssg-layouts/embedded/**`, T32) rendered against testsite views
//! through the binary (REWRITE_PLAN.md §7.1, T60), as insta snapshots reviewed against Go's
//! templates (`tpl/tplimpl/embedded/templates/**`).
//!
//! The testsite gets a test-only overlay (`embedded-overlay.txtar`): an `embedded` section
//! whose layouts include every embedded partial, pages that call every embedded shortcode and
//! render hook, a bundle with images, a `series` taxonomy and site params. The shared testsite
//! and Go's reference build are left alone. `rss.xml`, `sitemap.xml`, `sitemapindex.xml`,
//! `robots.txt`, `alias.html` and the link and image hooks (the testsite is a multilingual
//! single-host site, so `useEmbedded = "auto"` uses them) are already byte-identical to Go's
//! output in the A-T gate (`parity.rs`) and its full-output snapshot.
//!
//! Go's output for the overlay does not exist (Go is not built here); each snapshot was
//! reviewed line by line against Go's template source. The differences found and their
//! verdicts are in `crates/cli/README.md` (T60 review table).

use std::fs;
use std::path::Path;

use ssg_resources::{RemoteOptions, cache_key};
use ssg_testkit::txtar::Archive;

use crate::build::testsite;
use crate::{binary, fixture, site_from, stderr};

/// A 1×1 RGB PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x68, 0x70, 0x50, 0x00,
    0x00, 0x02, 0x24, 0x00, 0xe1, 0x95, 0xa4, 0xb1, 0x8b, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

/// oEmbed answers as the x and vimeo shortcodes request them, keyed by the URL Go's templates
/// build (`querify` sorts and query-escapes): a cache miss is a warning and an empty
/// shortcode in the snapshot.
const REMOTE: &[(&str, &str)] = &[
    (
        "https://publish.x.com/oembed?dnt=false&url=https%3A%2F%2Fx.com%2FSanDiegoZoo%2Fstatus%2F1453110110599868418",
        r#"{"url":"https://twitter.com/SanDiegoZoo/status/1453110110599868418","author_name":"San Diego Zoo","html":"<blockquote class=\"twitter-tweet\"><p lang=\"en\" dir=\"ltr\">Owl bet you&#39;ll lose this staring contest 🦉</p>&mdash; San Diego Zoo (@sandiegozoo) <a href=\"https://twitter.com/sandiegozoo/status/1453110110599868418\">October 26, 2021</a></blockquote>\n<script async src=\"https://platform.twitter.com/widgets.js\" charset=\"utf-8\"></script>\n","type":"rich"}"#,
    ),
    (
        "https://publish.x.com/oembed?dnt=false&omit_script=true&url=https%3A%2F%2Fx.com%2FSanDiegoZoo%2Fstatus%2F1453110110599868418",
        r#"{"url":"https://twitter.com/SanDiegoZoo/status/1453110110599868418","author_name":"San Diego Zoo","html":"<blockquote class=\"twitter-tweet\"><p lang=\"en\" dir=\"ltr\">Owl bet you&#39;ll lose this staring contest 🦉</p>&mdash; San Diego Zoo (@sandiegozoo) <a href=\"https://twitter.com/sandiegozoo/status/1453110110599868418\">October 26, 2021</a></blockquote>\n","type":"rich"}"#,
    ),
    (
        "https://vimeo.com/api/oembed.json?dnt=0&url=https%3A%2F%2Fvimeo.com%2F55073825",
        r#"{"type":"video","version":"1.0","provider_name":"Vimeo","provider_url":"https://vimeo.com/","title":"Sample \"video\" & more","thumbnail_url":"https://i.vimeocdn.com/video/452001751-8216e0571c_295x166.jpg","video_id":55073825}"#,
    ),
];

/// Outbound HTTP disabled (§7.2): ureq's proxy from the environment, a refusing port.
const NO_NETWORK: &[(&str, &str)] = &[
    ("HTTPS_PROXY", "http://127.0.0.1:9"),
    ("HTTP_PROXY", "http://127.0.0.1:9"),
    ("ALL_PROXY", "http://127.0.0.1:9"),
];

/// The testsite plus the overlay, in `dir`.
fn overlay_site(dir: &Path) {
    testsite(dir);
    Archive::read(&fixture("embedded-overlay.txtar"))
        .expect("read overlay")
        .write_to(dir)
        .expect("write overlay");
    for png in [
        "content/embedded/bundle/feature.png",
        "content/embedded/bundle/other.png",
        "content/embedded/featured/cover.png",
        "assets/img/logo.png",
    ] {
        let p = dir.join(png);
        fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        fs::write(p, PNG).expect("write png");
    }
    // Markdown attributes on blocks, and standalone images as blocks (so the image hook gets
    // `attributes`); the testsite's own content is not affected.
    let cfg = dir.join("config.toml");
    let mut toml = fs::read_to_string(&cfg).expect("config.toml");
    toml.push_str(
        "timeout = \"5s\"\n[markup.goldmark.parser]\nwrapStandAloneImageWithinParagraph = false\n\
         [markup.goldmark.parser.attribute]\nblock = true\n",
    );
    fs::write(&cfg, toml).expect("write config.toml");
    let cache = dir.join("resources/_gen/getresource");
    fs::create_dir_all(&cache).expect("mkdir cache");
    for (url, body) in REMOTE {
        let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{body}");
        fs::write(
            cache.join(cache_key(url, &RemoteOptions::default())),
            response,
        )
        .expect("write cache entry");
    }
}

fn read(dir: &Path, rel: &str) -> String {
    fs::read_to_string(dir.join("public").join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// Builds the overlay site with `privacy` as `config/_default/privacy.toml`; the site dir and
/// the warnings (with the temporary directory redacted).
fn build(privacy: &str) -> (tempfile::TempDir, std::path::PathBuf, String) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let site = tmp.path().join("testsite");
    overlay_site(&site);
    fs::write(site.join("config/_default/privacy.toml"), privacy).expect("privacy.toml");
    let o = binary(&site, &["--clock", "2026-01-01T00:00:00Z"], NO_NETWORK);
    let err = crate::redact_site(&stderr(&o), &site);
    assert!(o.status.success(), "{err}");
    (tmp, site, err)
}

const PRIVACY: &str =
    "[googleAnalytics]\nrespectDoNotTrack = true\n[youtube]\nprivacyEnhanced = true\n";

/// Every page of the overlay that calls an embedded template.
#[test]
fn embedded_templates() {
    let (_tmp, site, warnings) = build(PRIVACY);
    assert_eq!(warnings, "", "no warnings");
    ssg_testkit::snapshot::settings().bind(|| {
        // Render hooks: link, image (block with attributes), table; partials of a page.
        insta::assert_snapshot!("hooks", read(&site, "embedded/hooks/index.html"));
        // Shortcodes: figure, details, highlight, youtube, vimeo, instagram, x, param, ref,
        // relref.
        insta::assert_snapshot!("shortcodes", read(&site, "embedded/shortcodes/index.html"));
        // `_funcs/get-page-images`: the images param (resource, local path, URL), audio,
        // videos, see-also of a series, keywords from the terms of every taxonomy.
        insta::assert_snapshot!("bundle", read(&site, "embedded/bundle/index.html"));
        // A `*cover*` resource as the featured image.
        insta::assert_snapshot!("featured", read(&site, "embedded/featured/index.html"));
        // Pagination (default and terse) on the first and the last pager; partials of a list.
        insta::assert_snapshot!("section_page1", read(&site, "embedded/index.html"));
        insta::assert_snapshot!("section_page2", read(&site, "embedded/page/2/index.html"));
    });
    // What the hooks referenced is published: the asset, the page resources.
    for f in [
        "img/logo.png",
        "embedded/bundle/other.png",
        "embedded/featured/cover.png",
    ] {
        assert!(site.join("public").join(f).is_file(), "{f}");
    }
}

/// The `simple` privacy variants of vimeo, x and instagram, and the disabled services.
#[test]
fn embedded_templates_simple_and_disabled() {
    let (_tmp, site, warnings) = build(
        "[vimeo]\nsimple = true\nenableDNT = false\n[x]\nsimple = true\n[instagram]\nsimple = true\n",
    );
    assert_eq!(warnings, "", "no warnings");
    let page = read(&site, "embedded/shortcodes/index.html");
    let from = page.find("<blockquote\n").expect("instagram");
    // Instagram's markup is the default variant's (snapshotted above); only the script goes.
    let (head, tail) = page.split_at(from);
    let tail = &tail[tail.find("</blockquote>").expect("end") + "</blockquote>".len()..];
    ssg_testkit::snapshot::settings().bind(|| {
        insta::assert_snapshot!(
            "shortcodes_simple",
            format!("{head}[instagram blockquote]{tail}")
        );
    });

    let (_tmp, site, warnings) = build(
        "[googleAnalytics]\ndisable = true\n[youtube]\ndisable = true\n[vimeo]\ndisable = true\n\
         [x]\ndisable = true\n[instagram]\ndisable = true\n",
    );
    assert_eq!(warnings, "", "no warnings");
    let page = read(&site, "embedded/shortcodes/index.html");
    for gone in [
        "<iframe",
        "<blockquote",
        "twitter-tweet",
        "googletagmanager",
    ] {
        assert!(!page.contains(gone), "{gone}: {page}");
    }
    assert!(!read(&site, "embedded/hooks/index.html").contains("gtag"));
}

/// The embedded templates' errors and warnings: argument checks and the Universal Analytics
/// warning.
#[test]
fn embedded_template_errors() {
    let site = site_from(
        r#"
-- config.toml --
baseURL = "https://example.org/"
disableKinds = ["taxonomy", "term", "rss", "sitemap", "robots", "404", "section"]
[services.googleAnalytics]
id = "UA-12345"
-- layouts/single.html --
{% include "_partials/google_analytics.html" %}{{ page.content }}
-- layouts/home.html --
{{ partial(name="pagination", format="wide") }}
-- content/_index.md --
---
title: Home
---
-- content/youtube.md --
---
title: youtube
---
{{< youtube >}}
-- content/vimeo.md --
---
title: vimeo
---
{{< vimeo class="c" >}}
-- content/instagram.md --
---
title: instagram
---
{{< instagram >}}
-- content/x.md --
---
title: x
---
{{< x user="u" >}}
-- content/param.md --
---
title: param
---
{{< param >}} {{< param nope >}}
-- content/qr.md --
---
title: qr
---
{{< qr level="huge" scale=1 />}}
"#,
    );
    let o = binary(
        site.path(),
        &["-M", "--clock", "2026-01-01T00:00:00Z"],
        NO_NETWORK,
    );
    assert_eq!(o.status.code(), Some(1));
    let err = crate::redact_site(&stderr(&o), site.path());
    // One line per diagnostic, in the order the pages were rendered (parallel): sorted.
    let mut lines: Vec<&str> = err.lines().collect();
    lines.sort_unstable();

    ssg_testkit::snapshot::settings().bind(|| {
        insta::assert_snapshot!("errors", lines.join("\n"));
    });
}

/// The goat code block hook (`diagrams_goat`, the GoAT port; feature `goat` of the default
/// build) writes Go's markup byte for byte: a `viewBox` of GoAT's size, or the
/// `width`/`height` attributes instead, the `class`, and GoAT's SVG (the expected bytes are Go's
/// `diagrams.Goat` output for these diagrams).
#[test]
fn goat_code_block() {
    let site = site_from(
        r#"
-- config.toml --
baseURL = "https://example.org/"
disableKinds = ["taxonomy", "term", "rss", "sitemap", "robotstxt", "404", "section", "home"]
-- layouts/single.html --
{{ page.content }}
-- content/p.md --
---
title: p
---
```goat
*--+
   |
   v
```

```goat {class="c" width="100"}
\
```
"#,
    );
    let o = binary(site.path(), &["--quiet"], NO_NETWORK);
    assert!(o.status.success(), "{}", stderr(&o));
    let page = read(site.path(), "p/index.html");
    // The hook's lines (as Go's template writes them, blank-but-indented lines included)
    // around GoAT's `<g>` element.
    let hook = |class: &str, size: &[&str], g: &[&str]| {
        let mut lines = vec![
            String::new(),
            format!(r#"<div class="goat svg-container {class}">"#),
            "  ".to_owned(),
            "    <svg".to_owned(),
            r#"      xmlns="http://www.w3.org/2000/svg""#.to_owned(),
            r#"      font-family="Menlo,Lucida Console,monospace""#.to_owned(),
            "      ".to_owned(),
        ];
        lines.extend(size.iter().map(|l| (*l).to_owned()));
        lines.push("      >".to_owned());
        lines.push("      <g transform='translate(8,16)'>".to_owned());
        lines.extend(g.iter().map(|l| (*l).to_owned()));
        lines.extend(["</g>", "", "    </svg>", "  ", "</div>", ""].map(str::to_owned));
        lines.join("\n")
    };
    let first = hook(
        "",
        &[r#"        viewBox="0 0 40 57""#],
        &[
            "<path d='M 0,0 L 24,0' fill='none' stroke='currentColor'></path>",
            "<path d='M 24,0 L 24,32' fill='none' stroke='currentColor'></path>",
            "<polygon points='32.000000,32.000000 20.000000,26.400000 20.000000,37.599998' \
             fill='currentColor' transform='rotate(90.000000, 24.000000, 32.000000)'></polygon>",
            "<circle cx='0' cy='0' r='6' stroke='currentColor' fill='currentColor'></circle>",
        ],
    );
    let second = hook(
        "c",
        &[r#"        width="100""#, "        "],
        &["<path d='M -4,-8 L 4,8' fill='none' stroke='currentColor'></path>"],
    );
    assert!(page.contains(&first), "{page}");
    assert!(page.contains(&second), "{page}");
}

/// The text Go's `TestQRShortcode` encodes; the expected image names hash it.
const QR_TEXT: &str = "https://gohugo.io";

/// The `qr` shortcode renders what Go's `TestQRShortcode` asserts (names, sizes and
/// attributes), and publishes the images.
#[test]
fn qr_shortcode_equals_go_s() {
    let site = site_from(
        &r#"
-- config.toml --
baseURL = "https://example.org/"
disableKinds = ['page','rss','section','sitemap','taxonomy','term']
-- layouts/home.html --
{{ page.content }}
-- content/_index.md --
---
title: home
---
{{< qr
	text="$TEXT"
	level="high"
	scale=4
	targetDir="codes"
	alt="QR code linking to $TEXT"
	class="my-class"
	id="my-id"
	title="My Title"
/>}}

{{< qr >}}
$TEXT"
{{< /qr >}}
"#
        .replace("$TEXT", QR_TEXT),
    );
    let o = binary(
        site.path(),
        &["--clock", "2026-01-01T00:00:00Z"],
        NO_NETWORK,
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let html = read(site.path(), "index.html");
    for want in [
        format!(
            r#"<img src="/codes/qr_be5d263c2671bcbd.png" width="148" height="148" alt="QR code linking to {QR_TEXT}" class="my-class" id="my-id" title="My Title">"#
        ),
        r#"<img src="/qr_472aab57ec7a6e3d.png" width="132" height="132">"#.to_owned(),
    ] {
        assert!(html.contains(&want), "{want}\nnot in\n{html}");
    }
    for (file, side) in [
        ("codes/qr_be5d263c2671bcbd.png", 148),
        ("qr_472aab57ec7a6e3d.png", 132),
    ] {
        let png = fs::read(site.path().join("public").join(file)).expect("published");
        let be = |i: usize| u32::from_be_bytes(png[i..i + 4].try_into().expect("4 bytes"));
        assert_eq!((be(16), be(20)), (side, side), "{file}");
    }
}

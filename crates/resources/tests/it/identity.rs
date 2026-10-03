//! Identities: named targets (earlier language wins, conflicts within a language name both
//! call sites), transforms and metadata memoized, `concat`, `inject_generated`, and the image
//! seam (`image_input` → `ImageQueue::enqueue` → `register_image` → publish).

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use ssg_base::diag::Position;
use ssg_base::url::BaseUrl;
use ssg_base::{Idx, LangIdx, Value};
use ssg_config::MediaTypes;
use ssg_images::{ImageQueue, ImageSpec, Imaging, QrLevel};
use ssg_resources::meta::ResourceMeta;
use ssg_resources::{
    CallSite, HashAlgo, LangTarget, PublishPolicy, QrOptions, RemoteConfig, ResourceError,
    ResourceKind, ResourceStore, StoreConfig, Transform, qr_target,
};
use ssg_vfs::Vfs;

use crate::support::{MemSink, config, store, synth_dir};

fn lang(i: usize) -> LangIdx {
    LangIdx::from_index(i)
}

fn at(lang_index: usize, file: &str, line: u32) -> CallSite {
    CallSite {
        lang: lang(lang_index),
        position: Some(Position {
            file: Arc::from(PathBuf::from(file)),
            line,
            col: 3,
        }),
    }
}

fn bare(multihost: bool) -> ResourceStore {
    let lt = |url: &str, prefix: &str| LangTarget {
        base_url: BaseUrl::parse(url).unwrap(),
        target_prefix: prefix.to_owned(),
    };
    let languages = if multihost {
        vec![
            lt("https://en.example.org/docs/", "/en"),
            lt("https://fr.example.org/", "/fr"),
        ]
    } else {
        vec![
            lt("https://example.org/sub/", ""),
            lt("https://example.org/sub/", ""),
        ]
    };
    ResourceStore::new(StoreConfig {
        languages,
        multihost,
        media_types: Arc::new(MediaTypes::default()),
        vfs: None,
        images: None,
        remote: RemoteConfig::default(),
        transforms: Arc::default(),
    })
}

/// `images.QR`: Go's names (`TestQR`), Go's bytes, one resource per name, published when
/// referenced, per-language targets on multihost sites.
#[test]
fn qr_codes() {
    let s = bare(false);
    // The URL of Go's `TestQR`, which the recorded names and bytes encode.
    let url = "https://gohugo.io";
    let opts = QrOptions::default();
    let a = s.qr_code(url, &opts, &at(0, "layouts/a.html", 1)).unwrap();
    let r = s.resource(a);
    assert_eq!(r.rel_permalink, "/sub/qr_924bf7d80a564b23.png");
    assert_eq!(r.target.as_str(), "/qr_924bf7d80a564b23.png");
    assert_eq!(r.media_type_string(), "image/png");
    assert_eq!(r.kind, ResourceKind::Image);
    assert_eq!(r.policy, PublishPolicy::OnReference);
    assert_eq!(s.image_size(&r), Some((132, 132)));
    let png = ssg_images::qr_png(url, QrLevel::Medium, 4).unwrap();
    assert_eq!(&*s.content(a).unwrap(), &png[..]);
    // Another call site or language: the same resource.
    assert_eq!(
        s.qr_code(url, &opts, &at(1, "layouts/b.html", 9)).unwrap(),
        a
    );
    let options = |level, scale, dir: &str| QrOptions {
        level,
        scale,
        target_dir: dir.to_owned(),
    };
    for (o, want) in [
        (options(QrLevel::Low, 2, ""), "/sub/qr_9bf1ce25c5f2c058.png"),
        (
            options(QrLevel::High, 6, ""),
            "/sub/qr_bdc74ee7f5c11cc6.png",
        ),
        (
            options(QrLevel::High, 6, "foo/bar"),
            "/sub/foo/bar/qr_14162f02f2b83fff.png",
        ),
    ] {
        let id = s.qr_code(url, &o, &at(0, "x", 1)).unwrap();
        assert_eq!(s.resource(id).rel_permalink, want, "{o:?}");
        assert_eq!(qr_target(url, &o), want.trim_start_matches("/sub"));
    }
    // Go hashes the directory as written, then cleans the path.
    let slashed = qr_target(url, &options(QrLevel::High, 6, "/foo/bar/"));
    assert!(slashed.starts_with("/foo/bar/qr_"), "{slashed}");
    assert_ne!(slashed, "/foo/bar/qr_14162f02f2b83fff.png");
    assert!(matches!(
        s.qr_code("", &opts, &at(0, "x", 1)),
        Err(ResourceError::Image(_))
    ));
    assert!(
        s.qr_code(url, &options(QrLevel::Low, 1, ""), &at(0, "x", 1))
            .is_err()
    );

    // Published once referenced.
    let sink = MemSink::default();
    s.publish(["/sub/qr_924bf7d80a564b23.png"], &sink).unwrap();
    assert_eq!(
        sink.0.lock().unwrap().get("qr_924bf7d80a564b23.png"),
        Some(&png)
    );

    // Multihost: one image per language, under each language's prefix.
    let m = bare(true);
    let en = m.resource(m.qr_code(url, &opts, &at(0, "x", 1)).unwrap());
    let fr = m.resource(m.qr_code(url, &opts, &at(1, "x", 1)).unwrap());
    assert_eq!(en.target.as_str(), "/en/qr_924bf7d80a564b23.png");
    assert_eq!(fr.target.as_str(), "/fr/qr_924bf7d80a564b23.png");
    assert_eq!(
        fr.permalink.as_str(),
        "https://fr.example.org/qr_924bf7d80a564b23.png"
    );
}

#[test]
fn named_targets() {
    let s = bare(false);
    let a = s
        .from_string("css/x.css", "a{}", &at(0, "layouts/a.html", 1))
        .unwrap();
    assert_eq!(
        s.from_string("/css/x.css", "a{}", &at(0, "layouts/b.html", 2))
            .unwrap(),
        a
    );
    let r = s.resource(a);
    assert_eq!(r.name, "/css/x.css");
    assert_eq!(r.media_type_string(), "text/css");
    assert_eq!(r.rel_permalink, "/sub/css/x.css");
    assert_eq!(r.permalink.as_str(), "https://example.org/sub/css/x.css");
    assert_eq!(&*s.content(a).unwrap(), b"a{}");

    // Same language, other input: an error naming both call sites.
    let err = s
        .from_string("css/x.css", "b{}", &at(0, "layouts/c.html", 7))
        .unwrap_err();
    assert!(matches!(err, ResourceError::TargetConflict { .. }), "{err}");
    let msg = err.to_string();
    assert!(
        msg.contains("layouts/a.html:1:3") && msg.contains("layouts/c.html:7:3"),
        "{msg}"
    );
    // Another language: the earlier language's resource.
    assert_eq!(
        s.from_string("css/x.css", "fr{}", &at(1, "layouts/a.html", 1))
            .unwrap(),
        a
    );
    // Template outputs claim targets the same way.
    assert!(matches!(
        s.from_template_output("css/x.css", "c{}".into(), &at(0, "x", 1)),
        Err(ResourceError::TargetConflict { .. })
    ));
    let t = s
        .from_template_output("js/t.js", "var t;".into(), &at(0, "x", 1))
        .unwrap();
    assert_eq!(
        s.from_template_output("js/t.js", "var t;".into(), &at(0, "y", 2))
            .unwrap(),
        t
    );
    assert!(matches!(
        s.from_string("/", "x", &at(0, "x", 1)),
        Err(ResourceError::EmptyTarget)
    ));

    // Multihost: every language has its own copy under its directory and host.
    let m = bare(true);
    let en = m
        .from_string("a.txt", "en", &CallSite::in_lang(lang(0)))
        .unwrap();
    let fr = m
        .from_string("a.txt", "fr", &CallSite::in_lang(lang(1)))
        .unwrap();
    assert_ne!(en, fr);
    assert_eq!(m.resource(en).target.as_str(), "/en/a.txt");
    assert_eq!(m.resource(fr).target.as_str(), "/fr/a.txt");
    assert_eq!(m.resource(en).rel_permalink, "/docs/a.txt");
    assert_eq!(
        m.resource(fr).permalink.as_str(),
        "https://fr.example.org/a.txt"
    );
}

#[test]
fn concat_and_copy() {
    let s = bare(false);
    let call = CallSite::in_lang(lang(0));
    let a = s.from_string("js/a.js", "var a = 1", &call).unwrap();
    let b = s.from_string("js/b.js", "var b = 2", &call).unwrap();
    let c = s.from_string("css/c.css", "c{}", &call).unwrap();
    let all = s.concat("js/all.js", &[a, b], &call).unwrap();
    assert_eq!(&*s.content(all).unwrap(), b"var a = 1\n;\nvar b = 2");
    assert_eq!(s.concat("js/all.js", &[a, b], &call).unwrap(), all);
    assert!(matches!(
        s.concat("js/all.js", &[b, a], &call),
        Err(ResourceError::TargetConflict { .. })
    ));
    assert!(matches!(
        s.concat("js/mixed.js", &[a, c], &call),
        Err(ResourceError::MixedMediaTypes { .. })
    ));
    let d = s.from_string("css/d.css", "d{}", &call).unwrap();
    let css = s.concat("all.css", &[c, d], &call).unwrap();
    assert_eq!(&*s.content(css).unwrap(), b"c{}d{}");

    let copy = s.copy("x/copy.js", a, &call).unwrap();
    let r = s.resource(copy);
    assert_eq!(
        (r.name.as_str(), r.target.as_str()),
        ("/js/a.js", "/x/copy.js")
    );
    assert_eq!(&*s.content(copy).unwrap(), b"var a = 1");
}

#[test]
fn transforms_and_metadata_are_memoized() {
    let home = tempfile::tempdir().unwrap();
    let s = store(&synth_dir(), home.path());
    let css = s.get_asset(lang(0), "css/a.css").unwrap().unwrap();
    assert_eq!(s.get_asset(lang(1), "/css/a.css").unwrap(), Some(css));
    assert_eq!(s.get_asset(lang(0), "css/missing.css").unwrap(), None);
    assert_eq!(s.get_asset(lang(0), "").unwrap(), None);
    let f = Transform::Fingerprint(HashAlgo::from_str("sha384").unwrap());
    let fp = s.transform(css, f.clone()).unwrap();
    assert_eq!(s.transform(css, f).unwrap(), fp);
    assert!(s.resource(fp).integrity().unwrap().starts_with("sha384-"));
    assert!(matches!(
        HashAlgo::from_str("sha1"),
        Err(ResourceError::UnsupportedHash(_))
    ));

    let meta = ResourceMeta::parse(&Value::from_json(serde_json::json!([
        {"src": "**.css", "title": "Style :counter", "params": {"Kind": "css"}},
        {"src": "**", "name": "never-used-for-css-title"},
    ])))
    .unwrap();
    let m = s.apply_meta(css, &meta);
    assert_ne!(m, css);
    assert_eq!(s.apply_meta(css, &meta), m);
    let r = s.resource(m);
    assert_eq!(r.title, "Style 1");
    assert_eq!(r.name, "never-used-for-css-title");
    assert_eq!(r.params.get("kind").and_then(Value::as_str), Some("css"));
    assert_eq!(r.target, s.resource(css).target);
    assert_eq!(s.apply_meta(css, &ResourceMeta::default()), css);
    let none =
        ResourceMeta::parse(&Value::from_json(serde_json::json!([{"src": "*.txt"}]))).unwrap();
    assert_eq!(s.apply_meta(css, &none), css);
    assert!(ResourceMeta::parse(&Value::from_json(serde_json::json!([{"name": "x"}]))).is_err());

    let found: Vec<String> = s
        .find_assets(lang(0), "images/*.{png,gif}")
        .unwrap()
        .into_iter()
        .map(|id| s.resource(id).name.clone())
        .collect();
    assert_eq!(
        found,
        [
            "/images/fuzzy-circle.png",
            "/images/pix.gif",
            "/images/watermark.png"
        ]
    );
    assert_eq!(
        s.find_asset(lang(0), "/CSS/*.css")
            .unwrap()
            .map(|id| s.resource(id).name.clone()),
        Some("/css/a.css".to_owned())
    );
}

#[test]
fn image_seam() {
    let home = tempfile::tempdir().unwrap();
    let cfg = config(&synth_dir(), home.path());
    let queue = Arc::new(ImageQueue::new(Imaging::default(), None));
    let s = ResourceStore::new(StoreConfig::from_config(
        &cfg,
        Some(Arc::new(Vfs::new(&cfg).unwrap())),
        Some(Arc::clone(&queue)),
    ));
    let l = lang(0);
    let src = s.get_asset(l, "images/watermark.png").unwrap().unwrap();
    assert_eq!(s.resource(src).kind, ResourceKind::Image);
    let svg = s.get_asset(l, "images/logo.svg").unwrap().unwrap();
    assert_eq!(s.image_input(svg), None);
    let input = s.image_input(src).unwrap();
    let e = queue
        .enqueue(
            &input,
            Some(&ImageSpec::from_str("resize 300x240").unwrap()),
            &[],
        )
        .unwrap();
    let img = s.register_image(src, &e);
    assert_eq!(s.register_image(src, &e), img);
    let r = s.resource(img);
    assert_eq!(r.name, "/images/watermark.png");
    assert_eq!(r.target.as_str(), format!("/images/{}", e.file_name));
    assert_eq!(r.media_type_string(), "image/png");
    assert_eq!(r.policy, PublishPolicy::OnReference);
    assert!(r.rel_permalink.starts_with("/sub/images/watermark_hu_"));
    // A fingerprinted processed image publishes the processed bytes at its own target.
    let fp = s
        .transform(img, Transform::Fingerprint(HashAlgo::Md5))
        .unwrap();
    let sink = MemSink::default();
    let stats = s
        .publish(
            [r.permalink.as_str(), s.resource(fp).rel_permalink.as_str()],
            &sink,
        )
        .unwrap();
    assert_eq!(stats.images, 2);
    let files = sink.0.lock().unwrap();
    let bytes = &files[r.target.relative()];
    assert_eq!(&files[s.resource(fp).target.relative()], bytes);
    assert_eq!(&*s.content(img).unwrap(), bytes.as_slice());
    assert!(bytes.starts_with(b"\x89PNG"));
    assert!(!files.contains_key("images/watermark.png"));
}

/// Multihost (`oracle/resources/site-multihost`, keys oracle): an asset exists per language,
/// under the language's directory, linked from the language's host.
#[test]
fn multihost_assets() {
    let fx: serde_json::Value = ssg_testkit::fixture::oracle("oracle/resources/keys/keys.json.gz");
    let tmp = tempfile::tempdir().unwrap();
    let site = tmp.path().join("site");
    std::fs::create_dir_all(site.join("assets/images")).unwrap();
    std::fs::copy(
        ssg_testkit::fixture::testdata("oracle/resources/site-multihost/config.toml"),
        site.join("config.toml"),
    )
    .unwrap();
    std::fs::copy(
        synth_dir().join("assets/images/watermark.png"),
        site.join("assets/images/watermark.png"),
    )
    .unwrap();
    let s = store(&site, &tmp.path().join("home"));
    let mut tokens = Vec::new();
    for (i, want) in fx["multihost"].as_array().unwrap().iter().enumerate() {
        let id = s
            .get_asset(lang(i), "images/watermark.png")
            .unwrap()
            .unwrap();
        let r = s.resource(id);
        assert_eq!(r.rel_permalink, want["relPermalink"].as_str().unwrap());
        assert_eq!(r.permalink.as_str(), want["permalink"].as_str().unwrap());
        tokens.push(r.permalink.as_str().to_owned());
    }
    let sink = MemSink::default();
    s.publish(tokens.iter().map(String::as_str), &sink).unwrap();
    let published: Vec<String> = sink.0.lock().unwrap().keys().cloned().collect();
    let want: Vec<&str> = fx["multihostPublished"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p.as_str())
        .filter(|p| !p.contains("_hu_"))
        .collect();
    assert_eq!(published, want);
}

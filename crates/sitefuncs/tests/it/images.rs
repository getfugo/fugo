//! Image operations are queued: names and sizes are known at once; QR codes.

use ssg_base::ResourceId;

use crate::support;

#[test]
fn resize_fill_fit_crop_process() {
    let site = support::load();
    let s = site.home_scope();
    let r = |src: &str| site.render(src, &s);
    let logo = "get_asset(path=\"img/logo.png\")";
    let out = r(&format!(
        "{{% set i = {logo} | resize(width=35) %}}{{{{ i.width }}}}x{{{{ i.height }}}}|{{{{ i.rel_permalink }}}}"
    ));
    assert!(
        out.starts_with("35x35|/sub/img/logo_hu_") && out.ends_with(".png"),
        "{out}"
    );
    assert_eq!(
        r(&format!(
            "{{% set i = {logo} | fill(width=20, height=10, anchor=\"top\") %}}{{{{ i.width }}}}x{{{{ i.height }}}}"
        )),
        "20x10"
    );
    assert_eq!(
        r(&format!(
            "{{% set i = {logo} | fit(width=100, height=10) %}}{{{{ i.width }}}}x{{{{ i.height }}}}"
        )),
        "10x10"
    );
    assert_eq!(
        r(&format!(
            "{{% set i = {logo} | crop(spec=\"30x20\") %}}{{{{ i.width }}}}x{{{{ i.height }}}}"
        )),
        "30x20"
    );
    let webp = r(&format!(
        "{{% set i = {logo} | process(spec=\"resize 20x webp q50\") %}}{{{{ i.width }}}}|{{{{ i.media_type.type }}}}"
    ));
    assert_eq!(webp, "20|image/webp");
    // The same operation is the same resource.
    assert_eq!(
        r(&format!(
            "{{% set a = {logo} | resize(width=35) %}}{{% set b = {logo} | resize(spec=\"35x\") %}}{{{{ a.rel_permalink == b.rel_permalink }}}}"
        )),
        "true"
    );
    let e = site
        .try_render(&format!("{{{{ {logo} | resize(width=0) }}}}"), &s)
        .expect_err("width 0");
    assert!(e.to_string().contains("positive integer"), "{e}");
    let e = site
        .try_render(
            &format!("{{{{ {logo} | resize(spec=\"fill 10x10\") }}}}"),
            &s,
        )
        .expect_err("action mismatch");
    assert!(e.to_string().contains("use `process`"), "{e}");
    let e = site
        .try_render("{{ get_asset(path=\"css/a.css\") | resize(width=10) }}", &s)
        .expect_err("not an image");
    assert!(e.to_string().contains("not an image"), "{e}");
    // The kwargs complete the spec string before it is checked.
    assert_eq!(
        r(&format!(
            "{{% set i = {logo} | fill(spec=\"webp q80\", width=20, height=10) %}}{{{{ i.width }}}}x{{{{ i.height }}}}|{{{{ i.media_type.type }}}}"
        )),
        "20x10|image/webp"
    );
    let e = site
        .try_render(&format!("{{{{ {logo} | fill(width=10) }}}}"), &s)
        .expect_err("no height");
    assert!(e.to_string().contains("need a width and a height"), "{e}");
    // A kwarg of the wrong type names the filter and the kwarg.
    let e = site
        .try_render(&format!("{{{{ {logo} | resize(width=\"10\") }}}}"), &s)
        .expect_err("string width");
    assert!(e.to_string().contains("resize(width=)"), "{e}");
    let e = site
        .try_render(
            &format!("{{{{ {logo} | resize(width=10, format=\"webp\", height=17000) }}}}"),
            &s,
        )
        .expect_err("too large for WebP");
    assert!(e.to_string().contains("at most 16383x16383"), "{e}");
}

#[test]
fn image_filter_with_overlays_and_exif() {
    let site = support::load();
    let s = site.home_scope();
    let out = site.render(
        "{% set logo = get_asset(path=\"img/logo.png\") %}{% set small = logo | resize(width=10) %}{% set i = logo | image_filter(filters=[{\"op\": \"grayscale\"}, {\"op\": \"overlay\", \"image\": small, \"x\": 1, \"y\": 2}, {\"op\": \"process\", \"spec\": \"resize 40x\"}]) %}{{ i.width }}|{{ i.rel_permalink }}",
        &s,
    );
    assert!(out.starts_with("40|/sub/img/logo_hu_"), "{out}");
    let e = site
        .try_render(
            "{{ get_asset(path=\"img/logo.png\") | image_filter(filters=[{\"op\": \"wobble\"}]) }}",
            &s,
        )
        .expect_err("unknown op");
    assert!(e.to_string().contains("image_filter(filters=)"), "{e}");
    // The PNG has no EXIF data.
    assert_eq!(
        site.render("{{ get_asset(path=\"img/logo.png\") | exif is none }}", &s),
        "true"
    );
    let e = site
        .try_render("{{ get_asset(path=\"img/logo.png\") | image_colors }}", &s)
        .expect_err("not implemented");
    assert!(e.to_string().contains("not implemented"), "{e}");
}

/// The store resource of a rendered `{{ x.__rid }}`.
fn rid(out: &str) -> ResourceId {
    ResourceId::from_raw(out.trim().parse().expect("resource id"))
}

const LOGO: &str = r#"get_asset(path="img/logo.png")"#;
const FONT: &str = r#"get_asset(path="fonts/mulish.ttf")"#;

#[test]
fn text_and_dither_filters() {
    let mulish = ssg_testkit::fixture::legacy_docs().join("assets/opengraph/mulish-black.ttf");
    let site = support::load_copying(&[], &[("assets/fonts/mulish.ttf", &mulish)]);
    let s = site.home_scope();
    let r = |src: &str| site.render(src, &s);
    // Text with a font resource (any key case, Tera's `line_spacing`), then dithering.
    let text =
        format!(r#"{{"op": "text", "text": "Hi", "size": 20, "line_spacing": 4, "Font": {FONT}}}"#);
    let out = r(&format!(
        r#"{{% set i = {LOGO} | image_filter(filters=[{text}, {{"op": "dither", "method": "FloydSteinberg"}}]) %}}{{{{ i.width }}}}x{{{{ i.height }}}}|{{{{ i.rel_permalink }}}}|{{{{ i.__rid }}}}"#
    ));
    let parts: Vec<&str> = out.split('|').collect();
    assert_eq!(parts[0], "128x128", "{out}");
    assert!(
        parts[1].starts_with("/sub/img/logo_hu_") && parts[1].ends_with(".png"),
        "{out}"
    );
    let png = site.store.content(rid(parts[2])).expect("processed");
    let (size, _) = ssg_images::probe(&png, "result").expect("png");
    assert_eq!(size, (128, 128));
    // The default font with the same options: another image.
    let default_font = r(&format!(
        r#"{{% set i = {LOGO} | image_filter(filters=[{{"op": "text", "text": "Hi", "size": 20, "line_spacing": 4}}, {{"op": "dither"}}]) %}}{{{{ i.rel_permalink }}}}"#
    ));
    assert_ne!(default_font, parts[1]);
    // The same filters again (keys in another order): the same resource.
    let again = r(&format!(
        r#"{{% set i = {LOGO} | image_filter(filters=[{{"text": "Hi", "op": "text", "size": 20, "font": {FONT}, "line_spacing": 4}}, {{"op": "dither"}}]) %}}{{{{ i.rel_permalink }}}}"#
    ));
    assert_eq!(again, parts[1]);

    for (bad, why) in [
        (
            r#"{"op": "text", "text": "x", "font": "fonts/mulish.ttf"}"#,
            "must be a resource",
        ),
        (
            r#"{"op": "text", "text": "x", "font": get_asset(path="css/a.css")}"#,
            "not a usable font",
        ),
        (
            r#"{"op": "text", "text": "x", "alignx": "middle"}"#,
            "alignment",
        ),
        (r##"{"op": "dither", "colors": ["#000"]}"##, "two colors"),
        (r#"{"op": "dither", "method": "bayer"}"#, "dithering method"),
    ] {
        let e = site
            .try_render(
                &format!("{{{{ {LOGO} | image_filter(filters=[{bad}]) }}}}"),
                &s,
            )
            .expect_err(bad);
        assert!(e.to_string().contains(why), "{bad}: {e}");
    }
}

/// `images.QR` names from the Go implementation's `TestQR` and `TestQRShortcode` (the site's
/// base path is `/sub`), and Go's bytes.
#[test]
fn qr_codes_have_go_s_names_and_bytes() {
    let site = support::load();
    let s = site.home_scope();
    let r = |src: &str| site.render(src, &s);
    // The text the Go tests encode: the names below are hashes of it.
    let url = "https://gohugo.io";
    for (args, path) in [
        ("", "/sub/qr_924bf7d80a564b23.png"),
        (r#", level="medium""#, "/sub/qr_924bf7d80a564b23.png"),
        (
            r#", level="medium", scale=4"#,
            "/sub/qr_924bf7d80a564b23.png",
        ),
        (r#", level="low", scale=2"#, "/sub/qr_9bf1ce25c5f2c058.png"),
        (
            r#", level="medium", scale=3"#,
            "/sub/qr_7af14b329dd10af7.png",
        ),
        (
            r#", level="quartile", scale=5"#,
            "/sub/qr_9600ecb2010c2185.png",
        ),
        (r#", level="high", scale=6"#, "/sub/qr_bdc74ee7f5c11cc6.png"),
        (
            r#", level="high", scale=6, target_dir="foo/bar""#,
            "/sub/foo/bar/qr_14162f02f2b83fff.png",
        ),
        // Go decodes the options weakly: a numeric string is a scale too.
        (
            r#", level="low", scale="2""#,
            "/sub/qr_9bf1ce25c5f2c058.png",
        ),
    ] {
        let out = r(&format!(
            r#"{{% set q = qr_code(text="{url}"{args}) %}}{{{{ q.rel_permalink }}}}"#
        ));
        assert_eq!(out, path, "{args}");
    }
    let out = r(&format!(
        r#"{{% set q = qr_code(text="{url}", level="high", scale=4, target_dir="codes") %}}{{{{ q.rel_permalink }}}} {{{{ q.width }}}}x{{{{ q.height }}}} {{{{ q.media_type.type }}}} {{{{ q.__rid }}}}"#
    ));
    let parts: Vec<&str> = out.split(' ').collect();
    assert_eq!(
        parts[..3],
        ["/sub/codes/qr_be5d263c2671bcbd.png", "148x148", "image/png"]
    );
    let png = site.store.content(rid(parts[3])).expect("qr");
    assert_eq!(
        &*png,
        &ssg_images::qr_png(url, ssg_images::QrLevel::High, 4).expect("qr")[..]
    );
    // One resource per name.
    assert_eq!(
        r(&format!(
            r#"{{% set a = qr_code(text="{url}") %}}{{% set b = qr_code(text="{url}", level="MEDIUM") %}}{{{{ a.__rid == b.__rid }}}}"#
        )),
        "true"
    );
    // A QR code (bytes, not a file) can be processed like any image: the result keeps its
    // name's stem, beside it; it can be an overlay too.
    let out = r(&format!(
        r#"{{% set q = qr_code(text="{url}") %}}{{% set i = q | resize(width=20, format="webp") %}}{{{{ i.rel_permalink }}}} {{{{ i.width }}}}x{{{{ i.height }}}}"#
    ));
    assert!(
        out.starts_with("/sub/qr_924bf7d80a564b23_hu_") && out.ends_with(".webp 20x20"),
        "{out}"
    );
    let out = r(&format!(
        r#"{{% set q = qr_code(text="{url}") %}}{{% set i = get_asset(path="img/logo.png") | image_filter(filters=[{{"op": "overlay", "image": q, "x": 0, "y": 0}}]) %}}{{{{ i.rel_permalink }}}}"#
    ));
    assert!(out.starts_with("/sub/img/logo_hu_"), "{out}");
    for (bad, why) in [
        (r#"qr_code(text="")"#, "empty"),
        (
            r#"qr_code(text="x", level="huge")"#,
            "low, medium, quartile or high",
        ),
        (r#"qr_code(text="x", scale=1)"#, "at least 2"),
        (r#"qr_code(text="x", scale="big")"#, "at least 2"),
    ] {
        let e = site
            .try_render(&format!("{{{{ {bad} }}}}"), &s)
            .expect_err(bad);
        assert!(e.to_string().contains(why), "{bad}: {e}");
    }
}

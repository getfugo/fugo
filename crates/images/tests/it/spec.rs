//! The spec grammar and `[imaging]` decoding against the `nh-images/config` oracle.

use serde_json::{Value as J, json};
use ssg_config::ImagingConfig;
use ssg_images::{
    Action, Anchor, Color, Hint, ImageError, ImageFormat, ImageSpec, Imaging, Resample,
};
use ssg_testkit::fixture::oracle;

fn cases() -> Vec<J> {
    let doc: J = oracle("oracle/images/config/config.json.gz");
    doc["cases"].as_array().expect("cases").clone()
}

fn go_format(n: u64) -> ImageFormat {
    match n {
        1 => ImageFormat::Jpeg,
        2 => ImageFormat::Png,
        3 => ImageFormat::Gif,
        4 => ImageFormat::Tiff,
        5 => ImageFormat::Bmp,
        6 => ImageFormat::Webp,
        _ => panic!("format {n}"),
    }
}

fn go_anchor(n: i64) -> Anchor {
    [
        Anchor::Center,
        Anchor::TopLeft,
        Anchor::Top,
        Anchor::TopRight,
        Anchor::Left,
        Anchor::Right,
        Anchor::BottomLeft,
        Anchor::Bottom,
        Anchor::BottomRight,
    ]
    .get(usize::try_from(n).unwrap_or(usize::MAX))
    .copied()
    .unwrap_or(Anchor::Smart)
}

fn go_resample(name: &str) -> Resample {
    let lower = name.to_ascii_lowercase();
    let base = lower.strip_suffix("resampling").unwrap_or(&lower);
    base.parse().unwrap_or_else(|e| panic!("{e}"))
}

fn go_hint(n: u64) -> Hint {
    [
        Hint::Photo,
        Hint::Picture,
        Hint::Photo,
        Hint::Drawing,
        Hint::Icon,
        Hint::Text,
    ][usize::try_from(n).expect("hint")]
}

/// The Go colour of a fixture as 8-bit RGBA (`rgba` is 16-bit premultiplied).
fn go_color(c: &J) -> Color {
    let rgba: Vec<u64> = c["rgba"]
        .as_array()
        .expect("rgba")
        .iter()
        .map(|v| v.as_u64().expect("u64"))
        .collect();
    let a = rgba[3];
    let un = |v: u64| {
        (v * 0xffff)
            .checked_div(a)
            .map_or(0, |x| u8::try_from((x >> 8).min(255)).expect("u8"))
    };
    Color([
        un(rgba[0]),
        un(rgba[1]),
        un(rgba[2]),
        u8::try_from(a >> 8).expect("u8"),
    ])
}

/// The four `[imaging]` configurations of the oracle.
fn imaging(name: &str) -> Imaging {
    let mut i = Imaging::default();
    match name {
        "defaults" | "site" | "quality0" => {}
        "topleft-lanczos-q90" => {
            i.anchor = Anchor::TopLeft;
            i.resample = Resample::Lanczos;
            i.quality = 90;
            i.background = "#abc123".parse().expect("color");
        }
        other => panic!("config {other}"),
    }
    i
}

#[test]
fn spec_grammar_matches_decode_image_config() {
    let (mut total, mut ok) = (0, 0);
    let mut failures = Vec::new();
    for c in cases().iter().filter(|c| c["kind"] == "imageconfig") {
        total += 1;
        let opts: Vec<&str> = c["opts"]
            .as_array()
            .expect("opts")
            .iter()
            .map(|o| o.as_str().expect("str"))
            .collect();
        let spec_text = opts.join(" ");
        let cfg = c["cfg"].as_str().expect("cfg");
        let source = go_format(c["fmt"].as_u64().expect("fmt"));
        let got = spec_text.parse::<ImageSpec>();
        let want = &c["res"];
        let matched = match (&got, want.get("ok")) {
            (Err(_), None) => true,
            (Ok(spec), Some(w)) => {
                let r = spec.resolve(&imaging(cfg), source);
                let action = w["action"].as_str().expect("action");
                let mut same = r.action.map_or("", Action::name) == action
                    && u64::from(r.width.unwrap_or(0)) == w["width"].as_u64().unwrap_or(u64::MAX)
                    && u64::from(r.height.unwrap_or(0)) == w["height"].as_u64().unwrap_or(u64::MAX)
                    && r.format == go_format(w["target"].as_u64().expect("target"))
                    && i64::from(spec.rotate.unwrap_or(0)) == w["rotate"].as_i64().expect("rotate")
                    && r.background == w["bgColor"].as_object().map(|_| go_color(&w["bgColor"]))
                    && spec.quality.is_some() == w["qualitySet"].as_bool().expect("qualitySet")
                    && r.hint == go_hint(w["hint"].as_u64().expect("hint"));
                // Go keeps a configured quality of 0 (and fixes it when encoding); the
                // config crate rejects it, so the default applies here.
                if cfg != "quality0" || spec.quality.is_some() {
                    same &= u64::from(r.quality) == w["quality"].as_u64().expect("quality");
                }
                if !action.is_empty() {
                    same &= r.anchor == go_anchor(w["anchor"].as_i64().expect("anchor"))
                        && r.filter == go_resample(w["filter"].as_str().expect("filter"));
                }
                same
            }
            _ => false,
        };
        // Accepted deviations (README): unknown tokens and negative sizes are errors.
        let deviation = got.is_err()
            && want.get("ok").is_some()
            && (opts.iter().any(|o| {
                let o = o.trim().to_ascii_lowercase();
                o == "bogus" || o == "1234" || o.starts_with('-')
            }));
        if matched || deviation {
            ok += 1;
        } else {
            failures.push(format!(
                "[{cfg}] {opts:?} fmt {source}: got {got:?}, want {want}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{}/{total} ok; failures:\n{}",
        ok,
        failures.join("\n")
    );
    assert_eq!(total, 2376);
}

#[test]
fn imaging_config_matches_decode_config() {
    let mut failures = Vec::new();
    let mut n = 0;
    for c in cases().iter().filter(|c| c["kind"] == "decodeconfig") {
        let name = c["name"].as_str().expect("name");
        // Normalise the keys as the config pipeline does (case-insensitive, camelCase fields).
        let mut input = serde_json::Map::new();
        for (k, v) in c["in"].as_object().expect("in") {
            let key = match k.to_ascii_lowercase().as_str() {
                "resamplefilter" => "resampleFilter".to_owned(),
                "bgcolor" => "bgColor".to_owned(),
                other => other.to_owned(),
            };
            input.insert(key, v.clone());
        }
        // Value types are the config crate's concern (T13): skip what it rejects.
        let Ok(cfg) = serde_json::from_value::<ImagingConfig>(J::Object(input)) else {
            continue;
        };
        if name.starts_with("quality-") || name.starts_with("hint-") {
            // Quality is range-checked by the config crate; unknown hints are an error here
            // (README, deviations).
            continue;
        }
        n += 1;
        let got = Imaging::from_config(&cfg);
        match (&got, c["res"].get("ok")) {
            (Ok(i), Some(w)) => {
                let cfg_w = &w["cfg"];
                let want_bg: Color = cfg_w["bgColor"]
                    .as_str()
                    .expect("bg")
                    .parse()
                    .expect("color");
                let same = i.resample.name() == cfg_w["resampleFilter"].as_str().expect("rf")
                    && i.anchor.name() == cfg_w["anchor"].as_str().expect("anchor")
                    && i.background == want_bg
                    && i64::from(i.quality) == cfg_w["quality"].as_i64().expect("q");
                if !same {
                    failures.push(format!("{name}: got {i:?}, want {cfg_w}"));
                }
            }
            (Err(_), None) => {}
            // Go compiles EXIF field patterns lazily; an invalid one is an error here.
            (Err(ImageError::Config { key, .. }), Some(_)) if key.starts_with("imaging.exif") => {}
            _ => failures.push(format!("{name}: got {got:?}, want {}", c["res"])),
        }
    }
    assert!(failures.is_empty(), "failures:\n{}", failures.join("\n"));
    assert!(n > 60, "only {n} cases compared");
}

#[test]
fn deviation_rules_are_documented() {
    let rules = crate::common::expected_diffs("spec");
    for rule in [
        "unknown_tokens",
        "negative_sizes",
        "alpha_hex_colors",
        "lenient_extensions",
        "config_hint",
        "exif_patterns",
    ] {
        assert!(
            rules.contains_key(rule),
            "expected_diffs.toml [spec] lacks {rule}"
        );
    }
}

#[test]
fn colors_parse_like_go() {
    for c in cases().iter().filter(|c| c["kind"] == "color") {
        let input = c["in"].as_str().expect("in");
        let got = input.parse::<Color>();
        match c["res"].get("ok") {
            Some(w) => {
                let got = got.unwrap_or_else(|e| panic!("{input}: {e}"));
                let want = go_color(&w["color"]);
                if want.is_opaque() {
                    assert_eq!(got, want, "{input}");
                } else {
                    // Go reads `#rgba`/`#rrggbbaa` as premultiplied; here they are CSS
                    // (non-premultiplied) colours (README, deviations).
                    assert_eq!(got.0[3], want.0[3], "{input}");
                }
            }
            None => assert!(got.is_err(), "{input}: {got:?}"),
        }
    }
}

#[test]
fn formats_match_extensions() {
    for c in cases().iter().filter(|c| c["kind"] == "fromext") {
        let ext = c["ext"].as_str().expect("ext");
        let got = ImageFormat::from_extension(ext);
        // Extensions are case-insensitive and the dot is optional here (README, deviations).
        let lenient = ext.chars().any(|c| c.is_ascii_uppercase()) || !ext.starts_with('.');
        if c["ok"] == true {
            assert_eq!(
                got,
                Some(go_format(c["fmt"].as_u64().expect("fmt"))),
                "{ext}"
            );
        } else if !lenient {
            assert_eq!(got, None, "{ext}");
        }
    }
    for c in cases().iter().filter(|c| c["kind"] == "format") {
        let f = go_format(c["fmt"].as_u64().expect("fmt"));
        assert_eq!(f.media_type(), c["mediaType"].as_str().expect("mt"));
        assert_eq!(f.supports_transparency(), c["transparency"] == true);
        assert_eq!(f.has_quality(), c["defaultQ"] == true);
    }
}

#[test]
fn typed_kwargs_equal_the_string_form() {
    let from_map: ImageSpec = serde_json::from_value(json!({
        "action": "fill", "width": 600, "height": 400, "format": "webp", "quality": 75,
        "hint": "Drawing", "filter": "Lanczos", "anchor": "TopRight", "rotate": 90,
        "background": "#fff"
    }))
    .expect("kwargs");
    let from_str: ImageSpec = "600x400 fill WEBP q75 drawing lanczos topright r90 #fff"
        .parse()
        .expect("string");
    assert_eq!(from_map, from_str);
    // The canonical string form parses back to the same spec.
    assert_eq!(
        from_str
            .to_string()
            .parse::<ImageSpec>()
            .expect("round trip"),
        from_str
    );
    let s: ImageSpec = serde_json::from_value(json!("resize x200")).expect("string form");
    assert_eq!(s, ImageSpec::resize(None, Some(200)));

    for bad in [
        json!({"action": "resize"}),
        json!({"action": "crop", "width": 10}),
        json!({"width": 10}),
        json!({"action": "resize", "width": 10, "quality": 0}),
        json!({"action": "resize", "width": 10, "anchor": "middle"}),
        json!({"action": "resize", "width": 10, "colour": "#fff"}),
        json!("resize 600x400x2"),
        json!("resize 600x480 q101"),
        json!("resize 600x480 #ggg"),
    ] {
        assert!(
            serde_json::from_value::<ImageSpec>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

//! The text and dither filters through the queue: pixels, options, fonts and names.

use std::collections::BTreeSet;

use image::{Rgba, RgbaImage};
use serde_json::json;
use ssg_images::{
    Color, DitherMethod, DitherSpec, FontInput, ImageError, ImageFilter, ImageInput, ImageQueue,
    Imaging, TextSpec,
};
use ssg_testkit::fixture::repo_file;

use crate::common::{decode, expected_diffs, png, write_file};

/// Every key but `pattern` of `expected_diffs.toml` `[dither]` is a method, and they are the
/// methods whose published definition could not be used: Steven Pigeon's kernel and the
/// constructed or re-oriented threshold matrices.
#[test]
fn dither_deviations_name_real_methods() {
    let listed: BTreeSet<DitherMethod> = expected_diffs("dither")
        .keys()
        .filter(|k| *k != "pattern")
        .map(|k| k.parse().unwrap_or_else(|e| panic!("{k}: {e}")))
        .collect();
    use DitherMethod as M;
    let expected: BTreeSet<DitherMethod> = [
        M::StevenPigeon,
        M::ClusteredDot6x6,
        M::ClusteredDot6x6_2,
        M::ClusteredDot6x6_3,
        M::ClusteredDot8x8,
        M::ClusteredDotDiagonal16x16,
        M::ClusteredDotDiagonal6x6,
        M::ClusteredDotDiagonal8x8_2,
        M::ClusteredDotDiagonal8x8_3,
        M::ClusteredDotHorizontalLine,
        M::ClusteredDotSpiral5x5,
        M::ClusteredDotVerticalLine,
        M::Horizontal3x5,
    ]
    .into_iter()
    .collect();
    assert_eq!(listed, expected);
}

fn filter(v: serde_json::Value) -> ImageFilter {
    serde_json::from_value(v).expect("filter map")
}

fn run(q: &ImageQueue, input: &ImageInput, filters: &[ImageFilter]) -> RgbaImage {
    let e = q.enqueue(input, None, filters).expect("enqueue");
    let img = decode(&q.encoded(e.id).expect("process"));
    assert_eq!(img.dimensions(), (e.width, e.height));
    img
}

/// The bounding box `(x0, y0, x1, y1)` (inclusive) of pixels that differ from `bg`.
fn ink(img: &RgbaImage, bg: [u8; 4]) -> Option<(u32, u32, u32, u32)> {
    img.enumerate_pixels()
        .filter(|(_, _, p)| p.0 != bg)
        .fold(None, |b, (x, y, _)| {
            Some(match b {
                None => (x, y, x, y),
                Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
            })
        })
}

fn mulish() -> std::path::PathBuf {
    repo_file("docs/assets/opengraph/mulish-black.ttf")
}

#[test]
fn text_is_drawn_where_the_options_say() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bg = [20, 40, 60, 255];
    let src = ImageInput::File(write_file(
        dir.path(),
        "bg.png",
        &png(&RgbaImage::from_pixel(300, 120, Rgba(bg))),
    ));
    let q = ImageQueue::new(Imaging::default(), None);
    let text = |extra: serde_json::Value| {
        let mut m = json!({"op": "text", "text": "Hello", "color": "#ff8800"});
        for (k, v) in extra.as_object().expect("map") {
            m[k] = v.clone();
        }
        filter(m)
    };

    // Defaults: white, 20 px Go Regular, at (10, 10) with the baseline one ascent below.
    let img = run(&q, &src, &[text(json!({"color": "#ffffff"}))]);
    let (x0, y0, x1, y1) = ink(&img, bg).expect("ink");
    assert!(
        (10..=12).contains(&x0) && (10..=16).contains(&y0),
        "{x0},{y0}"
    );
    assert!(y1 <= 10 + 19 + 1, "{y1}");
    assert!(
        img.pixels().any(|p| p.0 == [255, 255, 255, 255]),
        "solid glyph pixels"
    );
    let first_width = x1 - x0;

    // The colour.
    let img = run(&q, &src, &[text(json!({}))]);
    assert!(img.pixels().any(|p| p.0 == [255, 136, 0, 255]));

    // Right alignment ends at x, center is centred on x.
    let img = run(&q, &src, &[text(json!({"x": 200, "alignx": "right"}))]);
    let (_, _, rx1, _) = ink(&img, bg).expect("ink");
    assert!((196..=200).contains(&rx1), "{rx1}");
    let img = run(&q, &src, &[text(json!({"x": 150, "alignx": "center"}))]);
    let (cx0, _, cx1, _) = ink(&img, bg).expect("ink");
    assert!((cx0 + cx1).abs_diff(300) <= 4, "{cx0}..{cx1}");

    // Size scales the text.
    let img = run(&q, &src, &[text(json!({"size": 40}))]);
    let (bx0, _, bx1, _) = ink(&img, bg).expect("ink");
    assert!(
        bx1 - bx0 > first_width * 3 / 2,
        "{} vs {first_width}",
        bx1 - bx0
    );

    // Vertical alignment: bottom puts the block above y.
    let img = run(&q, &src, &[text(json!({"y": 100, "aligny": "bottom"}))]);
    let (_, _, _, by1) = ink(&img, bg).expect("ink");
    assert!(by1 <= 100 + 5, "{by1}");

    // Line spacing moves the second line.
    let two = |spacing: i64| {
        let img = run(
            &q,
            &src,
            &[text(
                json!({"text": "Hello\nHello", "linespacing": spacing}),
            )],
        );
        ink(&img, bg).expect("ink").3
    };
    assert_eq!(two(30) - two(2), 28);

    // Wrapping: a long text on a narrow canvas makes more lines.
    let img = run(
        &q,
        &src,
        &[text(
            json!({"text": "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen", "size": 16}),
        )],
    );
    let (_, _, wx1, wy1) = ink(&img, bg).expect("ink");
    assert!(wx1 < 300 - 20 + 2, "{wx1}");
    assert!(wy1 > 10 + 2 * 16, "wrapped onto several lines: {wy1}");

    // Nothing outside the text changes; the size never changes.
    let img = run(&q, &src, &[text(json!({"x": 400, "y": 400}))]);
    assert_eq!(ink(&img, bg), None);
}

#[test]
fn fonts_from_files_and_registered_bytes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bg = [0, 0, 0, 255];
    let src = ImageInput::File(write_file(
        dir.path(),
        "bg.png",
        &png(&RgbaImage::from_pixel(400, 100, Rgba(bg))),
    ));
    let q = ImageQueue::new(Imaging::default(), None);
    let with_font = |font: Option<FontInput>| {
        ImageFilter::Text(TextSpec {
            size: 40.0,
            font,
            ..TextSpec::new("Typography")
        })
    };
    let default = q.enqueue(&src, None, &[with_font(None)]).expect("default");
    let file = q
        .enqueue(&src, None, &[with_font(Some(FontInput::File(mulish())))])
        .expect("file");
    let id = q
        .add_font(std::fs::read(mulish()).expect("mulish"))
        .expect("register");
    assert_eq!(
        q.add_font(std::fs::read(mulish()).expect("mulish"))
            .expect("again"),
        id,
        "the id is the bytes' identity"
    );
    let registered = q
        .enqueue(&src, None, &[with_font(Some(FontInput::Registered(id)))])
        .expect("registered");
    // The font's content, not how it was given, names the result.
    assert_eq!(file.file_name, registered.file_name);
    assert_ne!(default.file_name, file.file_name);
    let a = decode(&q.encoded(default.id).expect("a"));
    let b = decode(&q.encoded(file.id).expect("b"));
    assert_ne!(ink(&a, bg), ink(&b, bg), "another font, other glyphs");

    // Font ids in template maps are integers, font files strings.
    let from_map = filter(json!({"op": "text", "text": "x", "font": id.raw()}));
    assert_eq!(
        from_map,
        ImageFilter::Text(TextSpec {
            font: Some(FontInput::Registered(id)),
            ..TextSpec::new("x")
        })
    );

    // Unknown ids and bytes that are not a font are errors.
    let unknown = q.enqueue(
        &src,
        None,
        &[filter(json!({"op": "text", "text": "x", "font": 12345}))],
    );
    assert!(
        matches!(unknown, Err(ImageError::UnknownFont(_))),
        "{unknown:?}"
    );
    assert!(matches!(
        q.add_font(b"not a font".to_vec()),
        Err(ImageError::Font { .. })
    ));
    let not_font = write_file(dir.path(), "x.ttf", b"nope");
    assert!(
        q.enqueue(&src, None, &[with_font(Some(FontInput::File(not_font)))])
            .is_err()
    );
}

fn gradient() -> RgbaImage {
    RgbaImage::from_fn(64, 48, |x, y| {
        let v = u8::try_from((x * 255 / 63 + y) % 256).expect("u8");
        Rgba([v, 255 - v, u8::try_from(y * 5).expect("u8"), 255])
    })
}

#[test]
fn dithered_pixels_are_palette_colours() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut img = gradient();
    // A transparent strip and a half-transparent one.
    for y in 0..48 {
        img.put_pixel(0, y, Rgba([9, 9, 9, 0]));
        img.put_pixel(1, y, Rgba([200, 100, 50, 128]));
    }
    let src = ImageInput::File(write_file(dir.path(), "g.png", &png(&img)));
    let q = ImageQueue::new(Imaging::default(), None);
    let palettes: [&[&str]; 3] = [
        &["#000000", "#ffffff"],
        &["#222222", "#808080", "#dddddd"],
        &["#ff0000", "#00ff00", "#0000ff", "#ffff00"],
    ];
    for method in DitherMethod::ALL {
        for colors in palettes {
            let out = run(
                &q,
                &src,
                &[filter(
                    json!({"op": "dither", "method": method.name(), "colors": colors}),
                )],
            );
            let allowed: BTreeSet<[u8; 3]> = colors
                .iter()
                .map(|c| {
                    let c: Color = c.parse().expect("colour");
                    [c.0[0], c.0[1], c.0[2]]
                })
                .collect();
            for (x, y, p) in out.enumerate_pixels() {
                let a = img.get_pixel(x, y).0[3];
                assert_eq!(p.0[3], a, "{method}: alpha kept");
                if a > 0 {
                    assert!(
                        allowed.contains(&[p.0[0], p.0[1], p.0[2]]),
                        "{method} {colors:?}: {:?} at ({x}, {y})",
                        p.0
                    );
                }
            }
            // Several palette colours are used on a gradient.
            let used: BTreeSet<[u8; 3]> = out
                .pixels()
                .filter(|p| p.0[3] == 255)
                .map(|p| [p.0[0], p.0[1], p.0[2]])
                .collect();
            assert!(used.len() >= 2, "{method} {colors:?}: {used:?}");
        }
    }
}

#[test]
fn dither_options_change_the_result_deterministically() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = ImageInput::File(write_file(dir.path(), "g.png", &png(&gradient())));
    let q = ImageQueue::new(Imaging::default(), None);
    let pixels = |spec: DitherSpec| run(&q, &src, &[ImageFilter::Dither(spec)]);
    let default = pixels(DitherSpec::default());
    assert_eq!(default, pixels(DitherSpec::default()), "deterministic");
    assert_eq!(
        filter(json!({"op": "dither"})),
        ImageFilter::Dither(DitherSpec::default()),
        "Go's defaults"
    );
    let linear = pixels(DitherSpec {
        serpentine: false,
        ..DitherSpec::default()
    });
    assert_ne!(default, linear, "serpentine changes the scan");
    let weak = pixels(DitherSpec {
        strength: 0.5,
        ..DitherSpec::default()
    });
    assert_ne!(default, weak, "strength scales the diffusion");
    let ordered = pixels(DitherSpec {
        method: DitherMethod::ClusteredDot4x4,
        ..DitherSpec::default()
    });
    // Ordered dithering repeats with the matrix: a flat grey tiles with period 4.
    let flat = ImageInput::File(write_file(
        dir.path(),
        "flat.png",
        &png(&RgbaImage::from_pixel(16, 16, Rgba([140, 140, 140, 255]))),
    ));
    let out = run(
        &q,
        &flat,
        &[filter(json!({"op": "dither", "method": "ClusteredDot4x4"}))],
    );
    for (x, y, p) in out.enumerate_pixels() {
        assert_eq!(p, out.get_pixel(x % 4, y % 4), "({x}, {y})");
    }
    assert_ne!(default, ordered);
    // Serpentine does not apply to ordered dithering.
    let ordered_linear = pixels(DitherSpec {
        method: DitherMethod::ClusteredDot4x4,
        serpentine: false,
        ..DitherSpec::default()
    });
    assert_eq!(ordered, ordered_linear);
    // The mean linear luminance is kept (the error is diffused in linear light).
    let luminance = |img: &RgbaImage| {
        let lin = |v: u8| {
            let v = f64::from(v) / 255.0;
            if v <= 0.040_45 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        img.pixels()
            .map(|p| 0.2126 * lin(p.0[0]) + 0.7152 * lin(p.0[1]) + 0.0722 * lin(p.0[2]))
            .sum::<f64>()
            / f64::from(img.width() * img.height())
    };
    let (source, dithered) = (luminance(&gradient()), luminance(&default));
    assert!((source - dithered).abs() < 0.03, "{source} vs {dithered}");
}

/// Names are `<stem>_hu_<digits>.<ext>` like every processed image; the digits hash the
/// source and the planned filter (every option of it, the font by its content), as Go's
/// hash covers the filter's options.
#[test]
fn names_follow_the_filter_options() {
    let q = ImageQueue::new(Imaging::default(), None);
    let sunset = ImageInput::File(repo_file("resources/testdata/sunset.jpg"));
    let name = |f: serde_json::Value| {
        q.enqueue(&sunset, None, &[filter(f)])
            .expect("enqueue")
            .file_name
    };
    let digits = |n: &str| {
        let rest = n.strip_prefix("sunset_hu_").expect("stem");
        let (hex, ext) = rest.split_once('.').expect("ext");
        assert_eq!(hex.len(), 16, "{n}");
        assert!(hex.bytes().all(|b| b.is_ascii_hexdigit()), "{n}");
        assert_eq!(ext, "jpg", "{n}");
        hex.to_owned()
    };
    let text = name(json!({"op": "text", "text": "Fugo"}));
    digits(&text);
    // Equivalent maps plan the same filter: the same name.
    assert_eq!(
        text,
        name(json!({"op": "text", "text": "Fugo", "size": 20, "x": "10", "COLOR": "#ffffff"}))
    );
    let mut names = BTreeSet::new();
    for f in [
        json!({"op": "text", "text": "Fugo"}),
        json!({"op": "text", "text": "Fugo!"}),
        json!({"op": "text", "text": "Fugo", "size": 21}),
        json!({"op": "text", "text": "Fugo", "x": 11}),
        json!({"op": "text", "text": "Fugo", "y": 11}),
        json!({"op": "text", "text": "Fugo", "color": "#000000"}),
        json!({"op": "text", "text": "Fugo", "alignx": "center"}),
        json!({"op": "text", "text": "Fugo", "aligny": "bottom"}),
        json!({"op": "text", "text": "Fugo", "linespacing": 3}),
        json!({"op": "text", "text": "Fugo", "font": mulish()}),
        json!({"op": "dither"}),
        json!({"op": "dither", "method": "stucki"}),
        json!({"op": "dither", "serpentine": false}),
        json!({"op": "dither", "strength": 0.8}),
        json!({"op": "dither", "colors": ["#000000", "#ffffff", "#808080"]}),
    ] {
        let n = name(f.clone());
        digits(&n);
        assert!(names.insert(n), "{f}: a name already used");
    }
    // A process step to PNG changes the extension only.
    let png_name = q
        .enqueue(
            &sunset,
            None,
            &[
                filter(json!({"op": "dither"})),
                filter(json!({"op": "process", "spec": "png"})),
            ],
        )
        .expect("png")
        .file_name;
    assert!(
        png_name.starts_with("sunset_hu_") && png_name.ends_with(".png"),
        "{png_name}"
    );
}

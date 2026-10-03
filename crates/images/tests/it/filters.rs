//! Every filter variant from template maps, the pixels of the geometry and compositing
//! filters, and transparent edges.

use image::{Rgba, RgbaImage};
use serde_json::json;
use ssg_images::{ImageFilter, ImageFormat, ImageInput, ImageQueue, ImageSpec, Imaging};
use ssg_testkit::fixture::repo_file;

use crate::common::{decode, png, write_file};

/// One template map per variant.
fn all_filter_maps(overlay: &str) -> Vec<serde_json::Value> {
    vec![
        json!({"op": "brightness", "percentage": 20}),
        json!({"op": "contrast", "percentage": -30}),
        json!({"op": "gamma", "gamma": 1.4}),
        json!({"op": "gaussian_blur", "sigma": 1.5}),
        json!({"op": "grayscale"}),
        json!({"op": "hue", "shift": 90}),
        json!({"op": "invert"}),
        json!({"op": "colorize", "hue": 30, "saturation": 60, "percentage": 50}),
        json!({"op": "color_balance", "r": 20, "g": 0, "b": -20}),
        json!({"op": "saturation", "percentage": 80}),
        json!({"op": "sepia", "percentage": 60}),
        json!({"op": "sigmoid", "midpoint": 0.5, "factor": 5}),
        json!({"op": "unsharp_mask", "sigma": 1, "amount": 1, "threshold": 0.05}),
        json!({"op": "pixelate", "size": 4}),
        json!({"op": "opacity", "opacity": 0.5}),
        json!({"op": "padding", "margin": [3, 5], "color": "#123456"}),
        json!({"op": "overlay", "image": overlay, "x": 4, "y": -2}),
        json!({"op": "mask", "image": overlay}),
        json!({"op": "auto_orient"}),
        json!({"op": "text", "text": "Hi there", "size": 9, "x": 2, "y": 3, "color": "#123456", "alignx": "center", "aligny": "bottom", "linespacing": 1}),
        json!({"op": "dither", "colors": ["#000000", "#ff0000", "#ffffff"], "method": "Atkinson", "serpentine": false, "strength": 0.8}),
        json!({"op": "process", "spec": "resize 20x webp q60"}),
    ]
}

fn checker(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        let v = u8::try_from((x * 7 + y * 13) % 256).expect("u8");
        Rgba([v, 255 - v, (v / 2).wrapping_add(60), 255])
    })
}

#[test]
fn every_filter_variant_runs_with_its_planned_size() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = write_file(dir.path(), "src.png", &png(&checker(40, 30)));
    let mut transparent = checker(40, 30);
    for (x, _, p) in transparent.enumerate_pixels_mut() {
        p.0[3] = if x < 20 { 255 } else { 0 };
    }
    let src_alpha = write_file(dir.path(), "alpha.png", &png(&transparent));
    let overlay = write_file(dir.path(), "over.png", &png(&checker(10, 8)));
    let jpg = repo_file("resources/testdata/sunset.jpg");
    let maps = all_filter_maps(overlay.to_str().expect("utf-8 path"));
    let names: std::collections::BTreeSet<&str> =
        maps.iter().map(|m| m["op"].as_str().expect("op")).collect();
    assert_eq!(names.len(), 22, "one map per variant");
    let q = ImageQueue::new(Imaging::default(), None);
    let small_jpg = q
        .enqueue(
            &ImageInput::File(jpg),
            Some(&"resize 48x".parse().expect("spec")),
            &[],
        )
        .expect("jpg");
    for m in &maps {
        let filter: ImageFilter = serde_json::from_value(m.clone()).expect("filter map");
        assert_eq!(filter.name(), m["op"].as_str().expect("op"));
        // Serialising gives the same map back (the spec in its canonical string form).
        let back: ImageFilter =
            serde_json::from_value(serde_json::to_value(&filter).expect("ser")).expect("de");
        assert_eq!(back, filter);
        for input in [
            ImageInput::File(src.clone()),
            ImageInput::File(src_alpha.clone()),
            ImageInput::Op(small_jpg.id),
        ] {
            let e = q
                .enqueue(&input, None, std::slice::from_ref(&filter))
                .unwrap_or_else(|err| panic!("{}: {err}", filter.name()));
            let out = decode(&q.encoded(e.id).expect("process"));
            assert_eq!(out.dimensions(), (e.width, e.height), "{}", filter.name());
        }
    }
    // A chain of all of them.
    let chain: Vec<ImageFilter> = maps
        .iter()
        .map(|m| serde_json::from_value(m.clone()).expect("filter"))
        .collect();
    let e = q
        .enqueue(&ImageInput::File(src), None, &chain)
        .expect("chain");
    assert_eq!(
        e.format,
        ImageFormat::Webp,
        "the last process spec picks the format"
    );
    let out = decode(&q.encoded(e.id).expect("process"));
    assert_eq!(out.dimensions(), (e.width, e.height));
}

#[test]
fn template_maps_are_checked() {
    for bad in [
        json!({"op": "text"}),
        json!({"op": "text", "text": "hi", "alignx": "middle"}),
        json!({"op": "text", "text": "hi", "size": -1}),
        json!({"op": "text", "text": "hi", "nope": 1}),
        json!({"op": "dither", "colors": ["#000000"]}),
        json!({"op": "dither", "method": "bayer"}),
        json!({"op": "dither", "strength": "strong"}),
        json!({"op": "dither", "colours": ["#000", "#fff"]}),
        json!({"op": "brightness"}),
        json!({"op": "padding"}),
        json!({"op": "padding", "margin": [1, 2, 3, 4, 5]}),
        json!({"op": "padding", "margin": 6000}),
        json!({"op": "padding", "margin": 1, "top": 2}),
        json!({"op": "padding", "margin": 1, "color": "#12"}),
        json!({"op": "process", "spec": "resize"}),
        json!({"op": "overlay"}),
    ] {
        let got = serde_json::from_value::<ImageFilter>(bad.clone());
        // An empty padding is valid (all sides 0); everything else here is not.
        if bad == json!({"op": "padding"}) {
            assert!(got.is_ok(), "{bad}");
        } else {
            assert!(got.is_err(), "{bad}: {got:?}");
        }
    }
    let ImageFilter::Padding(p) =
        serde_json::from_value(json!({"op": "padding", "margin": [1, 2, 3]})).expect("padding")
    else {
        panic!("padding");
    };
    assert_eq!((p.top, p.right, p.bottom, p.left), (1, 2, 3, 2));
}

fn run(q: &ImageQueue, input: &ImageInput, spec: Option<&str>, f: &[ImageFilter]) -> RgbaImage {
    let spec: Option<ImageSpec> = spec.map(|s| s.parse().expect("spec"));
    let e = q.enqueue(input, spec.as_ref(), f).expect("enqueue");
    decode(&q.encoded(e.id).expect("process"))
}

#[test]
fn geometry_and_compositing_pixels() {
    let dir = tempfile::tempdir().expect("tempdir");
    let base = RgbaImage::from_pixel(20, 10, Rgba([200, 0, 0, 255]));
    let src = ImageInput::File(write_file(dir.path(), "base.png", &png(&base)));
    let top = RgbaImage::from_pixel(4, 4, Rgba([0, 0, 255, 255]));
    let top = ImageInput::File(write_file(dir.path(), "top.png", &png(&top)));
    let q = ImageQueue::new(Imaging::default(), None);

    let img = run(
        &q,
        &src,
        None,
        &[ImageFilter::Overlay {
            image: top.clone(),
            x: 18,
            y: -2,
        }],
    );
    assert_eq!(img.get_pixel(18, 0).0, [0, 0, 255, 255]);
    assert_eq!(img.get_pixel(17, 0).0, [200, 0, 0, 255]);
    assert_eq!(img.get_pixel(19, 2).0, [200, 0, 0, 255]);

    let pad: ImageFilter =
        serde_json::from_value(json!({"op": "padding", "margin": [1, 2], "color": "#00ff00"}))
            .expect("padding");
    let img = run(&q, &src, None, &[pad]);
    assert_eq!(img.dimensions(), (24, 12));
    assert_eq!(img.get_pixel(0, 0).0, [0, 255, 0, 255]);
    assert_eq!(img.get_pixel(2, 1).0, [200, 0, 0, 255]);

    let img = run(&q, &src, None, &[ImageFilter::Opacity { opacity: 0.5 }]);
    assert_eq!(img.get_pixel(0, 0).0[3], 128);

    // A mask: white keeps, black removes.
    let mut m = RgbaImage::from_pixel(20, 10, Rgba([255, 255, 255, 255]));
    for y in 0..10 {
        for x in 0..10 {
            m.put_pixel(x, y, Rgba([0, 0, 0, 255]));
        }
    }
    let mask = ImageInput::File(write_file(dir.path(), "mask.png", &png(&m)));
    let img = run(&q, &src, None, &[ImageFilter::Mask { image: mask }]);
    assert_eq!(img.get_pixel(2, 5).0[3], 0);
    assert_eq!(img.get_pixel(17, 5).0, [200, 0, 0, 255]);

    let img = run(&q, &src, None, &[ImageFilter::Grayscale]);
    assert!(img.pixels().all(|p| p.0[0] == p.0[1] && p.0[1] == p.0[2]));

    // r90 turns counter-clockwise: the top-right corner goes to the top left.
    let mut corner = RgbaImage::from_pixel(20, 10, Rgba([0, 0, 0, 255]));
    corner.put_pixel(19, 0, Rgba([255, 255, 255, 255]));
    let corner = ImageInput::File(write_file(dir.path(), "corner.png", &png(&corner)));
    let img = run(&q, &corner, Some("r90"), &[]);
    assert_eq!(img.dimensions(), (10, 20));
    assert_eq!(img.get_pixel(0, 0).0, [255, 255, 255, 255]);
    let img = run(&q, &corner, Some("r45"), &[]);
    assert!(
        img.width() > 20 && img.height() > 20,
        "{:?}",
        img.dimensions()
    );
    assert_eq!(
        img.get_pixel(0, 0).0[3],
        0,
        "outside the rotated image is transparent"
    );

    // Crop and fill anchors.
    let img = run(&q, &corner, Some("crop 5x5 topright"), &[]);
    assert_eq!(img.get_pixel(4, 0).0, [255, 255, 255, 255]);
    let img = run(&q, &corner, Some("fill 5x10 bottomleft"), &[]);
    assert_eq!(img.dimensions(), (5, 10));
}

#[test]
fn auto_orient_follows_exif() {
    let q = ImageQueue::new(Imaging::default(), None);
    let src = ImageInput::File(repo_file("resources/testdata/exif/orientation6.jpg"));
    let plain = q.enqueue(&src, None, &[]).expect("plain");
    let oriented = q
        .enqueue(&src, None, &[ImageFilter::AutoOrient])
        .expect("oriented");
    assert_eq!(
        (oriented.width, oriented.height),
        (plain.height, plain.width)
    );
    // A processed image keeps its source's orientation (Go keeps its EXIF data): resize
    // (80x40 → 40x20), then orient (20x40).
    let resized = q
        .enqueue(&src, Some(&"resize 40x".parse().expect("spec")), &[])
        .expect("resized");
    assert_eq!((resized.width, resized.height), (40, 20));
    let then_oriented = q
        .enqueue(
            &ImageInput::Op(resized.id),
            None,
            &[ImageFilter::AutoOrient],
        )
        .expect("then oriented");
    assert_eq!((then_oriented.width, then_oriented.height), (20, 40));
    let img = decode(&q.encoded(then_oriented.id).expect("process"));
    assert_eq!(img.dimensions(), (20, 40));
    // Without EXIF orientation the filter does nothing.
    let sunset = ImageInput::File(repo_file("resources/testdata/sunset.jpg"));
    let a = q.enqueue(&sunset, None, &[]).expect("a");
    let b = q
        .enqueue(&sunset, None, &[ImageFilter::AutoOrient])
        .expect("b");
    assert_eq!((a.width, a.height), (b.width, b.height));
}

/// Resizing and blurring an image with a hard alpha edge must not darken the edge: fully
/// transparent pixels (black here) must not bleed into their opaque white neighbours.
#[test]
fn png_alpha_edges_do_not_darken() {
    let dir = tempfile::tempdir().expect("tempdir");
    let img = RgbaImage::from_fn(64, 64, |x, y| {
        if (x / 8 + y / 8) % 2 == 0 {
            Rgba([255, 255, 255, 255])
        } else {
            Rgba([0, 0, 0, 0])
        }
    });
    let src = ImageInput::File(write_file(dir.path(), "edge.png", &png(&img)));
    let q = ImageQueue::new(Imaging::default(), None);
    for (spec, filters) in [
        (Some("resize 21x lanczos"), vec![]),
        (Some("resize 21x box"), vec![]),
        (Some("resize 50x catmullrom"), vec![]),
        (Some("resize 100x mitchellnetravali"), vec![]),
        (Some("fill 30x20 center linear"), vec![]),
        (None, vec![ImageFilter::GaussianBlur { sigma: 2.0 }]),
        (None, vec![ImageFilter::Pixelate { size: 5 }]),
        (Some("r30"), vec![]),
    ] {
        let out = run(&q, &src, spec, &filters);
        let mut partial = 0;
        for p in out.pixels() {
            let [r, g, b, a] = p.0;
            if a > 8 {
                assert!(
                    r >= 250 && g >= 250 && b >= 250,
                    "{spec:?} {filters:?}: dark edge pixel {:?}",
                    p.0
                );
            }
            if a > 0 && a < 255 {
                partial += 1;
            }
        }
        if spec.is_some_and(|s| s.contains("21x") || s.contains("50x")) {
            assert!(
                partial > 0,
                "{spec:?}: a resize across edges gives partial alpha"
            );
        }
        // The PNG keeps its alpha channel.
        let e = q
            .enqueue(
                &src,
                spec.map(|s| s.parse().expect("spec")).as_ref(),
                &filters,
            )
            .expect("enqueue");
        let bytes = q.encoded(e.id).expect("encode");
        assert_eq!(
            image::load_from_memory(&bytes).expect("png").color(),
            image::ColorType::Rgba8
        );
    }
    // To JPEG: the edges blend into the (default white) background, never into black.
    let out = run(&q, &src, Some("resize 21x jpg"), &[]);
    assert!(
        out.pixels().all(|p| p.0[0] > 200),
        "JPEG edges flatten onto white"
    );
    let out = run(&q, &src, Some("resize 21x jpg #000000"), &[]);
    assert!(
        out.pixels().any(|p| p.0[0] < 40),
        "an explicit background is used"
    );
}

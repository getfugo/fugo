//! Pixel parity: PSNR against images processed by the Go implementation.
//!
//! * `testdata/golden/images/` (T01): the 15 Go-processed images of the acceptance gate
//!   (≥ 30 dB each), described by `manifest.json` (format below), frozen at 44529028 (nothing
//!   regenerates them). Skipped with a note if they are missing.
//! * Interim: the Go implementation's own golden images (`images_golden` in
//!   `testdata/upstream/resources/images/testdata`, written by
//!   `resources/images/images_golden_integration_test.go`), whose recipes are known.
//! * Interim: the small outputs of `nh-images/process` stored in full (`bytes.json.gz`).
//!
//! `manifest.json` is an array of
//! `{"golden": "<file in golden/images>", "source": "<path from the repository root>",
//!   "imaging": {<[imaging] keys>}?, "steps": [{"spec": "<spec>"} | {"filters": [<filter>]}]}`;
//! each step applies to the previous result, and file paths in filters (`image` of overlay and
//! mask, `font` of text) are relative to the repository root.
//!
//! A recipe with a `dither` filter is compared after a 7×7 box blur of both images: error
//! diffusion is chaotic in its input (the resized source already differs from Go's by a few
//! levels), so its noise pattern cannot match pixel for pixel, while the blur compares what
//! dithering must preserve, the local tone.

use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage};
use serde::Deserialize;
use serde_json::{Value as J, json};
use ssg_config::ImagingConfig;
use ssg_images::{ImageFilter, ImageInput, ImageQueue, ImageSpec, Imaging};
use ssg_testkit::fixture::{oracle, repo_file, testdata};

use crate::common::{decode, expected_diffs, psnr, synth, write_file};

/// The gate of the acceptance criteria.
const MIN_PSNR: f64 = 30.0;

#[derive(Deserialize)]
struct Recipe {
    golden: String,
    source: String,
    #[serde(default)]
    imaging: Option<ImagingConfig>,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Step {
    Spec { spec: ImageSpec },
    Filters { filters: Vec<J> },
}

/// Makes the `image` and `font` paths of filters absolute (relative to the repository root).
fn rooted(mut filter: J) -> ImageFilter {
    for key in ["image", "font"] {
        if let Some(J::String(p)) = filter.get(key) {
            let abs = repo_file(p);
            filter[key] = json!(abs);
        }
    }
    serde_json::from_value(filter).expect("filter")
}

/// Whether a recipe dithers (compared through a low-pass filter, see the module docs).
fn dithers(recipe: &Recipe) -> bool {
    recipe.steps.iter().any(|s| match s {
        Step::Filters { filters } => filters.iter().any(|f| f["op"] == "dither"),
        Step::Spec { .. } => false,
    })
}

/// A `(2r + 1)²` box blur (the low-pass of dithered comparisons), edges clamped.
fn box_blur(img: &RgbaImage, r: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    RgbaImage::from_fn(w, h, |x, y| {
        let (mut sum, mut n) = ([0u32; 4], 0u32);
        for yy in y.saturating_sub(r)..=(y + r).min(h - 1) {
            for xx in x.saturating_sub(r)..=(x + r).min(w - 1) {
                for (s, v) in sum.iter_mut().zip(img.get_pixel(xx, yy).0) {
                    *s += u32::from(v);
                }
                n += 1;
            }
        }
        Rgba(sum.map(|s| u8::try_from((s + n / 2) / n).unwrap_or(u8::MAX)))
    })
}

/// A comparison: the PSNR, and the encoded sizes of our result and of the golden image.
struct Compared {
    db: f64,
    ours_len: usize,
    golden_len: usize,
}

/// Runs a recipe and compares the result with the golden image.
fn run(recipe: &Recipe, golden_dir: &Path) -> Result<Compared, String> {
    let imaging = match &recipe.imaging {
        Some(c) => Imaging::from_config(c).map_err(|e| e.to_string())?,
        None => Imaging::default(),
    };
    let q = ImageQueue::new(imaging, None);
    let mut input = ImageInput::File(repo_file(&recipe.source));
    let mut last = None;
    for step in &recipe.steps {
        let e = match step {
            Step::Spec { spec } => q.enqueue(&input, Some(spec), &[]),
            Step::Filters { filters } => {
                let fs: Vec<ImageFilter> = filters.iter().map(|f| rooted(f.clone())).collect();
                q.enqueue(&input, None, &fs)
            }
        }
        .map_err(|e| e.to_string())?;
        input = ImageInput::Op(e.id);
        last = Some(e);
    }
    let e = last.ok_or("no steps")?;
    let ours_bytes = q.encoded(e.id).map_err(|e| e.to_string())?;
    let ours = decode(&ours_bytes);
    let golden_path = golden_dir.join(&recipe.golden);
    let golden_bytes =
        std::fs::read(&golden_path).map_err(|e| format!("{}: {e}", golden_path.display()))?;
    let golden = decode(&golden_bytes);
    if ours.dimensions() != golden.dimensions() {
        return Err(format!(
            "size {:?}, golden {:?}",
            ours.dimensions(),
            golden.dimensions()
        ));
    }
    if Some(e.format)
        != ssg_images::ImageFormat::from_extension(
            Path::new(&recipe.golden)
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or(""),
        )
    {
        return Err(format!("format {} for {}", e.format, recipe.golden));
    }
    let db = if dithers(recipe) {
        psnr(&box_blur(&ours, 3), &box_blur(&golden, 3))
    } else {
        psnr(&ours, &golden)
    };
    Ok(Compared {
        db,
        ours_len: ours_bytes.len(),
        golden_len: golden_bytes.len(),
    })
}

/// Accepted exceptions (expected_diffs.toml `[psnr]`) must still reach this.
const FLOOR_PSNR: f64 = 20.0;

fn run_all(recipes: &[Recipe], dir: &Path, label: &str) -> Vec<String> {
    let accepted = expected_diffs("psnr");
    let mut failures = Vec::new();
    let mut rows = Vec::new();
    for r in recipes {
        match run(r, dir) {
            Ok(Compared {
                db,
                ours_len,
                golden_len,
            }) => {
                let exception = accepted.contains_key(&r.golden);
                rows.push(format!(
                    "  {db:6.2} dB  {ours_len:7} B (Go {golden_len:7} B)  {}{}",
                    r.golden,
                    if exception {
                        "  (accepted, expected_diffs.toml)"
                    } else {
                        ""
                    }
                ));
                let min = if exception { FLOOR_PSNR } else { MIN_PSNR };
                if db < min {
                    failures.push(format!("{}: {db:.2} dB < {min}", r.golden));
                }
            }
            Err(e) => failures.push(format!("{}: {e}", r.golden)),
        }
    }
    eprintln!(
        "{label}: PSNR of {} images\n{}",
        recipes.len(),
        rows.join("\n")
    );
    failures
}

#[test]
fn golden_images_from_t01() {
    let dir = testdata("golden/images");
    let manifest = dir.join("manifest.json");
    if !manifest.is_file() {
        eprintln!(
            "SKIPPED: {} is missing: the 15 Go-processed golden images (T01) are frozen at \
             44529028, restore them from git (the interim PSNR checks below cover Go's own \
             golden images)",
            manifest.display()
        );
        return;
    }
    let recipes: Vec<Recipe> = serde_json::from_slice(&std::fs::read(&manifest).expect("manifest"))
        .expect("manifest.json");
    assert!(
        recipes.len() >= 15,
        "{} golden images, expected 15",
        recipes.len()
    );
    let failures = run_all(&recipes, &dir, "T01 golden images");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The Go implementation's own golden images and their recipes (images_golden_integration_test.go),
/// the smart-anchor crops and fills included.
fn go_golden_recipes() -> Vec<Recipe> {
    let sunset = "resources/testdata/sunset.jpg";
    let gopher = "resources/testdata/gopher-hero8.png";
    let mask = "resources/testdata/mask.png";
    let mask2 = "resources/testdata/mask2.png";
    let x300 = json!({"spec": "resize x300"});
    let mut recipes = Vec::new();
    let mut add = |golden: &str, source: &str, imaging: J, steps: J| {
        recipes.push(
            serde_json::from_value::<Recipe>(json!({
                "golden": golden, "source": source,
                "imaging": if imaging.is_null() { J::Null } else { imaging },
                "steps": steps,
            }))
            .expect("recipe"),
        );
    };
    let misc = |f: J| json!([x300, {"filters": f}]);
    for (name, filters) in [
        (
            "brightness-40.jpg",
            json!([{"op": "brightness", "percentage": 40}]),
        ),
        (
            "contrast-50.jpg",
            json!([{"op": "contrast", "percentage": 50}]),
        ),
        ("gamma-1.667.jpg", json!([{"op": "gamma", "gamma": 1.667}])),
        (
            "gaussianblur-5.jpg",
            json!([{"op": "gaussian_blur", "sigma": 5}]),
        ),
        ("grayscale.jpg", json!([{"op": "grayscale"}])),
        (
            "grayscale+colorize-180-50-20.jpg",
            json!([{"op": "grayscale"}, {"op": "colorize", "hue": 180, "saturation": 50, "percentage": 20}]),
        ),
        (
            "colorbalance-180-50-20.jpg",
            json!([{"op": "color_balance", "r": 180, "g": 50, "b": 20}]),
        ),
        ("hue--15.jpg", json!([{"op": "hue", "shift": -15}])),
        ("invert.jpg", json!([{"op": "invert"}])),
        (
            "opacity-0.65.jpg",
            json!([{"op": "opacity", "opacity": 0.65}]),
        ),
        (
            "padding-20-40-#976941.jpg",
            json!([{"op": "padding", "margin": [20, 40], "color": "#976941"}]),
        ),
        ("pixelate-10.jpg", json!([{"op": "pixelate", "size": 10}])),
        (
            "saturation-65.jpg",
            json!([{"op": "saturation", "percentage": 65}]),
        ),
        (
            "sigmoid-0.6--4.jpg",
            json!([{"op": "sigmoid", "midpoint": 0.6, "factor": -4}]),
        ),
        (
            "unsharpmask.jpg",
            json!([{"op": "unsharp_mask", "sigma": 10, "amount": 0.4, "threshold": 0.03}]),
        ),
    ] {
        add(
            &format!("filters/misc/{name}"),
            sunset,
            J::Null,
            misc(filters),
        );
    }
    add(
        "filters/misc/sepia-80.jpg",
        sunset,
        J::Null,
        json!([x300, {"filters": [{"op": "grayscale"}]}, {"filters": [{"op": "sepia", "percentage": 80}]}]),
    );
    add(
        "filters/misc/rotate270.jpg",
        "resources/testdata/exif/orientation6.jpg",
        J::Null,
        json!([{"filters": [{"op": "auto_orient"}]}]),
    );
    add(
        "filters/misc/text.jpg",
        sunset,
        J::Null,
        // The text of the Go test's golden image.
        misc(
            json!([{"op": "text", "text": "Hugo Rocks!", "color": "#fbfaf5",
            "linespacing": 8, "size": 40, "x": 25, "y": 190}]),
        ),
    );
    add(
        "filters/misc/dither-default.jpg",
        sunset,
        J::Null,
        misc(json!([{"op": "dither"}])),
    );
    // TestImagesGoldenFiltersText: the 900×562 sunset, text centred on (450, 281).
    let lorem = "Pariatur deserunt sunt nisi sunt tempor quis eu. Sint et nulla enim officia \
        sunt cupidatat. Eu amet ipsum qui velit cillum cillum ad Lorem in non ad aute.";
    let longer = "Est exercitation deserunt exercitation nostrud magna. Eiusmod anim deserunt \
        sit elit dolore ea incididunt nisi. Ea ullamco excepteur voluptate occaecat duis \
        pariatur proident cupidatat.  Eu id esse qui consectetur commodo ad ex esse cupidatat \
        velit duis cupidatat. Aliquip irure tempor consequat non amet in mollit ipsum officia \
        tempor laborum.";
    for (name, text, alignx, aligny) in [
        ("text_alignx-center.jpg", lorem, "center", "top"),
        ("text_alignx-right.jpg", lorem, "right", "top"),
        ("text_alignx-left.jpg", lorem, "left", "top"),
        (
            "text_alignx-center_aligny-center.jpg",
            longer,
            "center",
            "center",
        ),
        (
            "text_alignx-center_aligny-bottom.jpg",
            longer,
            "center",
            "bottom",
        ),
    ] {
        add(
            &format!("filters/text/{name}"),
            sunset,
            J::Null,
            json!([{"filters": [{"op": "text", "text": text, "color": "#fbfaf5",
                "linespacing": 8, "size": 28, "x": 450, "y": 281,
                "alignx": alignx, "aligny": aligny}]}]),
        );
    }
    // The overlay is the gopher resized to x80, itself an operation: run it separately below.
    let mask_cfg = |bg: &str| json!({"bgColor": bg, "hint": "photo", "quality": 75, "resampleFilter": "Lanczos"});
    for (name, spec, bg, m) in [
        (
            "filters/mask/transparant.png",
            "resize x300 png",
            "#ebcc34",
            mask,
        ),
        (
            "filters/mask/yellow.jpg",
            "resize x300 jpg",
            "#ebcc34",
            mask,
        ),
        ("filters/mask/wide.jpg", "resize 600x200", "#ebcc34", mask),
        (
            "filters/mask/blue.jpg",
            "resize x300 #323ea8",
            "#ebcc34",
            mask,
        ),
        (
            "filters/mask2/green.jpg",
            "resize x300 jpg",
            "#33ff44",
            mask,
        ),
        (
            "filters/mask2/pink.jpg",
            "resize x300 jpg",
            "#a83269",
            mask2,
        ),
    ] {
        add(
            name,
            sunset,
            mask_cfg(bg),
            json!([{"filters": [{"op": "process", "spec": spec}, {"op": "mask", "image": m}]}]),
        );
    }
    for (name, source, spec) in [
        (
            "process/misc/crop-500x200-smart.jpg",
            sunset,
            "crop 500x200 smart",
        ),
        (
            "process/misc/fill-500x200-smart.jpg",
            sunset,
            "fill 500x200 smart",
        ),
        (
            "process/misc/fit-500x200-smart.jpg",
            sunset,
            "fit 500x200 smart",
        ),
        (
            "process/misc/resize-100x100-r180.png",
            gopher,
            "resize 100x100 r180",
        ),
        (
            "process/misc/resize-300x300-jpg-b31280.jpg",
            gopher,
            "resize 300x300 jpg #b31280",
        ),
    ] {
        add(name, source, J::Null, json!([{"spec": spec}]));
    }
    let methods_cfg = json!({"bgColor": "#ebcc34", "hint": "photo", "quality": 75, "resampleFilter": "MitchellNetravali"});
    for (name, source, spec) in [
        ("methods/resize-sunsetjpg-300x.jpg", sunset, "resize 300x"),
        ("methods/resize-sunsetjpg-x200.jpg", sunset, "resize x200"),
        (
            "methods/fill-sunsetjpg-90x120-left.jpg",
            sunset,
            "fill 90x120 left",
        ),
        (
            "methods/fill-sunsetjpg-90x120-right.jpg",
            sunset,
            "fill 90x120 right",
        ),
        ("methods/fit-sunsetjpg-200x200.jpg", sunset, "fit 200x200"),
        // `.Crop "200x200"`: the default anchor, smart.
        ("methods/crop-sunsetjpg-200x200.jpg", sunset, "crop 200x200"),
        (
            "methods/crop-sunsetjpg-350x400-center.jpg",
            sunset,
            "crop 350x400 center",
        ),
        (
            "methods/crop-sunsetjpg-350x400-smart.jpg",
            sunset,
            "crop 350x400 smart",
        ),
        (
            "methods/crop-sunsetjpg-350x400-center-r90.jpg",
            sunset,
            "crop 350x400 center r90",
        ),
        (
            "methods/crop-sunsetjpg-350x400-center-q20.jpg",
            sunset,
            "crop 350x400 center q20",
        ),
        ("methods/resize-gopherpng-100x.png", gopher, "resize 100x"),
        (
            "methods/resize-gopherpng-100x-fc03ec.png",
            gopher,
            "resize 100x #fc03ec",
        ),
        (
            "methods/resize-gopherpng-100x-03fc56-jpg.jpg",
            gopher,
            "resize 100x #03fc56 jpg",
        ),
    ] {
        add(name, source, methods_cfg.clone(), json!([{"spec": spec}]));
    }
    recipes
}

fn go_golden_dir() -> PathBuf {
    repo_file("resources/images/testdata/images_golden")
}

#[test]
fn go_golden_images_interim() {
    let recipes = go_golden_recipes();
    let mut failures = run_all(&recipes, &go_golden_dir(), "Go golden images (interim)");
    // The overlay: the gopher resized to x80 drawn at (20, 20) over the x300 sunset.
    let q = ImageQueue::new(Imaging::default(), None);
    let sunset = q
        .enqueue(
            &ImageInput::File(repo_file("resources/testdata/sunset.jpg")),
            Some(&"resize x300".parse().expect("spec")),
            &[],
        )
        .expect("sunset");
    let gopher = q
        .enqueue(
            &ImageInput::File(repo_file("resources/testdata/gopher-hero8.png")),
            Some(&"resize x80".parse().expect("spec")),
            &[],
        )
        .expect("gopher");
    let over = q
        .enqueue(
            &ImageInput::Op(sunset.id),
            None,
            &[ImageFilter::Overlay {
                image: ImageInput::Op(gopher.id),
                x: 20,
                y: 20,
            }],
        )
        .expect("overlay");
    let ours_bytes = q.encoded(over.id).expect("encode");
    let golden_bytes =
        std::fs::read(go_golden_dir().join("filters/misc/overlay-20-20.jpg")).expect("golden");
    let db = psnr(&decode(&ours_bytes), &decode(&golden_bytes));
    eprintln!(
        "  {db:6.2} dB  {:7} B (Go {:7} B)  filters/misc/overlay-20-20.jpg",
        ours_bytes.len(),
        golden_bytes.len()
    );
    if db < MIN_PSNR {
        failures.push(format!("overlay-20-20.jpg: {db:.2} dB"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The oracle's small outputs stored in full, for sources whose pixels are reproduced here
/// exactly (the PNG synthetic sources) and results that are not lossy-encoded twice.
#[test]
fn oracle_small_outputs_interim() {
    let doc: J = oracle("oracle/images/process/bytes.json.gz");
    let dir = tempfile::tempdir().expect("tempdir");
    let q = ImageQueue::new(Imaging::default(), None);
    let mut values = Vec::new();
    let mut low = Vec::new();
    for (i, c) in doc["cases"].as_array().expect("cases").iter().enumerate() {
        let src = c["src"].as_str().expect("src");
        let op = c["op"].as_str().expect("op");
        let Some(spec) = op.strip_prefix("d:") else {
            continue;
        };
        if !src.ends_with(":png") || spec.contains("gif") {
            // JPEG sources: their bytes are Go's (tests/it/jpeg.rs), but they decode with
            // another IDCT and chroma upsampling than Go's; GIF palettes differ by design.
            continue;
        }
        let transparent = ["nrgbaa", "nrgba64", "paletted"]
            .iter()
            .any(|k| src.starts_with(&format!("gen:{k}:")));
        if transparent && spec.contains("jpg") {
            // The oracle encodes without Go's flattening onto the background (it calls
            // EncodeTo directly), so transparent pixels come out black.
            continue;
        }
        let bytes = synth(src).expect("synthetic source");
        let path = write_file(dir.path(), &format!("s{i}.png"), &bytes);
        let spec: ImageSpec = spec.parse().expect("spec");
        let e = q
            .enqueue(&ImageInput::File(path), Some(&spec), &[])
            .expect("enqueue");
        let ours = decode(&q.encoded(e.id).expect("encode"));
        let go = decode(&base64(c["b"].as_str().expect("b")));
        if ours.dimensions() != go.dimensions() {
            low.push(format!(
                "{src} {op}: size {:?} vs {:?}",
                ours.dimensions(),
                go.dimensions()
            ));
            continue;
        }
        let db = psnr(&ours, &go);
        if db < MIN_PSNR {
            low.push(format!("{src} {op}: {db:.1} dB"));
        }
        values.push(db);
    }
    values.sort_by(f64::total_cmp);
    let median = values.get(values.len() / 2).copied().unwrap_or(0.0);
    eprintln!(
        "oracle small outputs: {} compared, median {median:.1} dB, {} below {MIN_PSNR} dB:\n{}",
        values.len(),
        low.len(),
        low.join("\n")
    );
    assert!(values.len() > 100, "only {} outputs compared", values.len());
    assert!(median >= MIN_PSNR, "median PSNR {median:.1} dB");
}

fn base64(s: &str) -> Vec<u8> {
    let val = |c: u8| -> u32 {
        match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("base64 {c}"),
        }
    };
    let digits: Vec<u8> = s.bytes().filter(|&c| c != b'=').collect();
    let mut out = Vec::new();
    for chunk in digits.chunks(4) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &c)| n | val(c) << (18 - 6 * i));
        let b = n.to_be_bytes();
        out.extend_from_slice(&b[1..chunk.len()]);
    }
    out
}

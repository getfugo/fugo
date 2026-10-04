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

mod recipes;

use recipes::*;

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

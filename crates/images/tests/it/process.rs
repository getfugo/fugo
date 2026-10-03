//! Result sizes against the `nh-images/process` oracle: every spec, filter and real-site
//! chain the oracle ran, on every source that is available here.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value as J;
use ssg_base::ImageOpId;
use ssg_images::{
    Anchor, Color, Enqueued, ImageError, ImageFilter, ImageInput, ImageQueue, ImageSpec, Imaging,
    PaddingSpec, Resample,
};
use ssg_testkit::fixture::{oracle, repo_file};

use crate::common::{decode, expected_diffs, source_path, synth, write_file};

/// The oracle's `[imaging]` configurations by prefix.
fn imaging(prefix: &str) -> Imaging {
    let mut i = Imaging::default();
    if prefix == "lz" {
        i.resample = Resample::Lanczos;
        i.anchor = Anchor::TopLeft;
        i.background = "#abc123".parse().expect("color");
    }
    i
}

/// The filter chains of the oracle (`filterChains` in main.go).
fn filter_chain(name: &str, wm: &ImageInput, mask: &ImageInput) -> Vec<ImageFilter> {
    use ImageFilter as F;
    let overlay = |x, y| F::Overlay {
        image: wm.clone(),
        x,
        y,
    };
    let pad = |v: &[i32], c: Color| F::Padding(PaddingSpec::from_shorthand(v, c).expect("padding"));
    let process = |s: &str| F::Process {
        spec: s.parse().expect("spec"),
    };
    match name {
        "overlay" => vec![overlay(0, 0)],
        "overlay-off" => vec![overlay(10, -5)],
        "brightness" => vec![F::Brightness { percentage: 30.0 }],
        "contrast" => vec![F::Contrast { percentage: -20.0 }],
        "gamma" => vec![F::Gamma { gamma: 1.5 }],
        "blur" => vec![F::GaussianBlur { sigma: 1.2 }],
        "gray" => vec![F::Grayscale],
        "hue" => vec![F::Hue { shift: -45.0 }],
        "invert" => vec![F::Invert],
        "pixelate" => vec![F::Pixelate { size: 5 }],
        "saturation" => vec![F::Saturation { percentage: 50.0 }],
        "sepia" => vec![F::Sepia { percentage: 70.0 }],
        "sigmoid" => vec![F::Sigmoid {
            midpoint: 0.5,
            factor: 7.0,
        }],
        "unsharp" => vec![F::UnsharpMask {
            sigma: 1.0,
            amount: 0.8,
            threshold: 0.02,
        }],
        "colorize" => vec![F::Colorize {
            hue: 180.0,
            saturation: 50.0,
            percentage: 30.0,
        }],
        "colorbalance" => vec![F::ColorBalance {
            r: 10.0,
            g: -10.0,
            b: 20.0,
        }],
        "opacity" => vec![F::Opacity { opacity: 0.6 }],
        "opacity-over" => vec![F::Opacity { opacity: 1.7 }],
        "padding" => vec![pad(&[10, 20], "#abc123".parse().expect("color"))],
        "padding-neg" => vec![pad(&[-2], Color::WHITE)],
        "padding-bad" => vec![pad(&[-1000], Color::WHITE)],
        "mask" => vec![F::Mask {
            image: mask.clone(),
        }],
        "process" => vec![process("resize 50x webp")],
        "chain" => vec![
            F::Brightness { percentage: 10.0 },
            process("resize 40x"),
            overlay(0, 0),
        ],
        other => panic!("unknown chain {other}"),
    }
}

fn bounds(res: &J) -> Option<(u32, u32)> {
    // "(0,0)-(W,H)"
    let b = res["bounds"].as_str()?;
    let (_, max) = b.split_once(")-(")?;
    let (w, h) = max.trim_end_matches(')').split_once(',')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// A queue per `[imaging]` configuration, and the watermark and mask inputs.
struct Runner {
    queues: BTreeMap<&'static str, ImageQueue>,
    wm: ImageInput,
    mask: ImageInput,
}

impl Runner {
    fn queue(&self, prefix: &str) -> &ImageQueue {
        &self.queues[match prefix {
            "lz" => "lz",
            _ => "d",
        }]
    }

    fn spec_op(
        &self,
        prefix: &str,
        input: &ImageInput,
        spec: &str,
    ) -> Result<Enqueued, ImageError> {
        let spec: ImageSpec = spec.parse()?;
        self.queue(prefix).enqueue(input, Some(&spec), &[])
    }
}

/// The outcome of one oracle case, for the size comparison and optional processing.
struct Outcome {
    op: String,
    got: Result<Enqueued, String>,
    want: Option<(u32, u32)>,
}

fn run_source(r: &Runner, path: PathBuf, cases: &[&J], out: &mut Vec<Outcome>) {
    let input = ImageInput::File(path);
    let mut chain: BTreeMap<String, ImageOpId> = BTreeMap::new();
    for c in cases {
        let op = c["op"].as_str().expect("op");
        let res = &c["res"];
        let want = bounds(res);
        let got: Result<Enqueued, ImageError> = if op == "decode" {
            // Enqueueing reads the header only: decode the pixels too, as Go's decode did.
            r.queue("d").enqueue(&input, None, &[]).and_then(|e| {
                r.queue("d").encoded(e.id)?;
                Ok(e)
            })
        } else if let Some(name) = op.strip_prefix("filter:") {
            r.queue("d")
                .enqueue(&input, None, &filter_chain(name, &r.wm, &r.mask))
        } else if let Some(step) = op.strip_prefix("chain:") {
            let (size, part) = step.rsplit_once(':').expect("chain step");
            let q = r.queue("d");
            let spec_of = |s: &str| s.parse::<ImageSpec>().expect("spec");
            let dims = if size == "render" {
                let e = q.enqueue(&input, None, &[]).expect("source");
                format!("{}x{}", e.width, e.height)
            } else {
                size.to_owned()
            };
            let get = |k: &str| chain.get(&format!("{size}:{k}")).copied();
            let result = match part {
                "A" => q.enqueue(&input, Some(&spec_of(&format!("resize {dims}"))), &[]),
                "W" => q.enqueue(&r.wm, Some(&spec_of(&format!("resize {dims}"))), &[]),
                "B" => {
                    let base = if size == "render" {
                        input.clone()
                    } else {
                        ImageInput::Op(get("A").expect("A"))
                    };
                    let overlay = ImageFilter::Overlay {
                        image: ImageInput::Op(get("W").expect("W")),
                        x: 0,
                        y: 0,
                    };
                    q.enqueue(&base, None, &[overlay])
                }
                "C" => q.enqueue(
                    &ImageInput::Op(get("B").expect("B")),
                    Some(&spec_of(&format!("resize {dims} webp"))),
                    &[],
                ),
                "C600" => q.enqueue(
                    &ImageInput::Op(get("B").expect("B")),
                    Some(&spec_of("resize 600x480 webp")),
                    &[],
                ),
                "A-webp" => q.enqueue(
                    &ImageInput::Op(get("A").expect("A")),
                    Some(&spec_of("resize 600x480 webp")),
                    &[],
                ),
                other => panic!("chain part {other}"),
            };
            if let Ok(e) = &result {
                chain.insert(format!("{size}:{part}"), e.id);
            }
            result
        } else {
            let (prefix, spec) = op.split_once(':').expect("cfg:spec");
            r.spec_op(prefix, &input, spec)
        };
        out.push(Outcome {
            op: op.to_owned(),
            got: got.map_err(|e| e.to_string()),
            want,
        });
    }
}

struct Report {
    expected: BTreeMap<String, String>,
    compared: usize,
    matched: usize,
    accepted: usize,
    missing_sources: usize,
    failures: Vec<String>,
    processed: usize,
}

fn run_oracle(process_small: bool) -> Report {
    let doc: J = oracle("oracle/images/process/process.json.gz");
    let cases = doc["cases"].as_array().expect("cases");
    let mut by_src: BTreeMap<&str, Vec<&J>> = BTreeMap::new();
    for c in cases {
        by_src
            .entry(c["src"].as_str().expect("src"))
            .or_default()
            .push(c);
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let wm_path = write_file(
        dir.path(),
        "watermark.png",
        &synth("gen:nrgbaa:600x480:png").expect("wm"),
    );
    let mask_path = repo_file("resources/testdata/mask.png");
    let queues: BTreeMap<&'static str, ImageQueue> = ["d", "lz"]
        .into_iter()
        .map(|p| (p, ImageQueue::new(imaging(p), None)))
        .collect();
    let runner = Runner {
        queues,
        wm: ImageInput::File(wm_path),
        mask: ImageInput::File(mask_path),
    };
    let mut report = Report {
        expected: expected_diffs("process"),
        compared: 0,
        matched: 0,
        accepted: 0,
        missing_sources: 0,
        failures: Vec::new(),
        processed: 0,
    };
    for (i, (src, cases)) in by_src.iter().enumerate() {
        let path = if let Some(bytes) = synth(src) {
            let ext = if src.ends_with(":png") { "png" } else { "jpg" };
            write_file(dir.path(), &format!("gen{i}.{ext}"), &bytes)
        } else if let Some(p) = source_path(src) {
            p
        } else {
            report.missing_sources += 1;
            continue;
        };
        let mut outcomes = Vec::new();
        run_source(&runner, path, cases, &mut outcomes);
        let small = src.starts_with("gen:") && !src.contains("640x480") && !src.contains("2048");
        for o in outcomes {
            report.compared += 1;
            let got_size = o.got.as_ref().ok().map(|e| (e.width, e.height));
            if got_size == o.want {
                report.matched += 1;
            } else if report.expected.contains_key(&format!("{src} {}", o.op)) {
                report.accepted += 1;
            } else {
                report.failures.push(format!(
                    "{src} {}: got {:?}, want {:?}",
                    o.op, o.got, o.want
                ));
            }
            // Process the small synthetic sources: the pixels must have the planned size.
            if process_small
                && small
                && o.op != "decode"
                && let Ok(e) = &o.got
            {
                let q = runner.queue(o.op.split_once(':').map_or("d", |(p, _)| p));
                let bytes = q
                    .encoded(e.id)
                    .unwrap_or_else(|err| panic!("{src} {}: {err}", o.op));
                let img = decode(&bytes);
                assert_eq!(
                    img.dimensions(),
                    (e.width, e.height),
                    "{src} {}: processed size differs from the planned one",
                    o.op
                );
                report.processed += 1;
            }
        }
    }
    report
}

#[test]
fn sizes_match_the_process_oracle() {
    let r = run_oracle(true);
    eprintln!(
        "process oracle: {}/{} sizes match ({} accepted differences); {} sources unavailable \
         (Go's image test data); {} small results processed",
        r.matched, r.compared, r.accepted, r.missing_sources, r.processed
    );
    assert!(
        r.failures.is_empty(),
        "{}/{} sizes match; mismatches:\n{}",
        r.matched,
        r.compared,
        r.failures.join("\n")
    );
    assert_eq!(r.missing_sources, 0, "oracle sources not found");
    assert!(r.compared >= 12_264, "only {} cases compared", r.compared);
}

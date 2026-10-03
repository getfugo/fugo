//! The deferred queue: names, identity, chains, publishing, `[caches.images]`, codecs.

use std::collections::BTreeMap;
use std::time::Duration;

use image::{Rgba, RgbaImage};
use ssg_base::paths::OutputPath;
use ssg_config::global::MaxAge;
use ssg_images::{
    Hint, ImageCache, ImageError, ImageFilter, ImageFormat, ImageInput, ImageQueue, ImageSpec,
    Imaging,
};
use ssg_testkit::fixture::repo_file;

use crate::common::{MemorySink, decode, png, psnr, write_file};

fn spec(s: &str) -> ImageSpec {
    s.parse().expect("spec")
}

fn photo() -> ImageInput {
    ImageInput::File(repo_file("resources/testdata/sunset.jpg"))
}

#[test]
fn names_are_stable_and_content_addressed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let q = ImageQueue::new(Imaging::default(), None);
    let a = q
        .enqueue(&photo(), Some(&spec("resize 300x")), &[])
        .expect("a");
    let again = q
        .enqueue(&photo(), Some(&spec("300x resize")), &[])
        .expect("again");
    assert_eq!(a, again, "the same operation is queued once");
    assert_eq!(q.len(), 1);
    let (stem, rest) = a.file_name.split_once("_hu_").expect("_hu_");
    assert_eq!(stem, "sunset");
    let (hash, ext) = rest.split_once('.').expect("ext");
    assert_eq!(hash.len(), 16);
    assert_eq!(ext, "jpg");
    // The digits hash content and operation only; the id also covers the name (see
    // `identical_bytes_keep_their_own_names`).
    assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()), "{hash}");

    let webp = q
        .enqueue(&photo(), Some(&spec("resize 300x webp")), &[])
        .expect("webp");
    assert_ne!(webp.id, a.id);
    assert!(webp.file_name.ends_with(".webp"));
    assert_eq!(webp.format, ImageFormat::Webp);

    // A chain keeps the source's stem, not `sunset_hu_…_hu_…`.
    let chained = q
        .enqueue(&ImageInput::Op(a.id), None, &[ImageFilter::Grayscale])
        .expect("chained");
    assert!(
        chained.file_name.starts_with("sunset_hu_"),
        "{}",
        chained.file_name
    );
    assert_eq!(chained.file_name.matches("_hu_").count(), 1);

    // The extension keeps its spelling when the format is kept.
    let bytes = std::fs::read(repo_file("resources/testdata/sunset.jpg")).expect("read");
    let upper = ImageInput::File(write_file(dir.path(), "Photo.JPEG", &bytes));
    let e = q
        .enqueue(&upper, Some(&spec("resize 10x")), &[])
        .expect("upper");
    assert!(e.file_name.starts_with("Photo_hu_") && e.file_name.ends_with(".JPEG"));
    // Same content under another name: same hash digits, other stem.
    assert_eq!(
        e.id,
        q.enqueue(&upper, Some(&spec("resize 10x")), &[])
            .expect("e")
            .id
    );

    // Different content, same name: another name.
    let other = write_file(dir.path(), "sunset.jpg", &png(&RgbaImage::new(8, 8)));
    let f = q
        .enqueue(&ImageInput::File(other), Some(&spec("resize 4x")), &[])
        .expect("other");
    let g = q
        .enqueue(&photo(), Some(&spec("resize 4x")), &[])
        .expect("g");
    assert_ne!(f.file_name, g.file_name);

    // The site defaults are part of the identity.
    let lanczos = Imaging {
        resample: ssg_images::Resample::Lanczos,
        ..Imaging::default()
    };
    let q2 = ImageQueue::new(lanczos, None);
    let b = q2
        .enqueue(&photo(), Some(&spec("resize 300x")), &[])
        .expect("b");
    assert_ne!(b.id, a.id);
}

#[test]
fn unknown_and_bad_inputs_are_errors() {
    let q = ImageQueue::new(Imaging::default(), None);
    let id = ssg_base::ImageOpId::from_raw(42);
    assert!(matches!(
        q.enqueue(&ImageInput::Op(id), None, &[]),
        Err(ImageError::UnknownOp(_))
    ));
    assert!(matches!(q.encoded(id), Err(ImageError::UnknownOp(_))));
    let missing = ImageInput::File(repo_file("does/not/exist.png"));
    assert!(matches!(
        q.enqueue(&missing, None, &[]),
        Err(ImageError::Io { .. })
    ));
    let not_image = ImageInput::File(repo_file("Cargo.toml"));
    assert!(q.enqueue(&not_image, None, &[]).is_err());
    let pad: ImageFilter =
        serde_json::from_value(serde_json::json!({"op": "padding", "margin": -5000}))
            .expect("padding");
    assert!(matches!(
        q.enqueue(&photo(), None, &[pad]),
        Err(ImageError::Filter { .. })
    ));
}

#[test]
fn process_writes_only_wanted_results() {
    let q = ImageQueue::new(Imaging::default(), None);
    let a = q
        .enqueue(&photo(), Some(&spec("resize 120x")), &[])
        .expect("a");
    let wm = q
        .enqueue(
            &ImageInput::File(repo_file("resources/testdata/gopher-hero8.png")),
            Some(&spec("resize 40x")),
            &[],
        )
        .expect("wm");
    let b = q
        .enqueue(
            &ImageInput::Op(a.id),
            None,
            &[ImageFilter::Overlay {
                image: ImageInput::Op(wm.id),
                x: 0,
                y: 0,
            }],
        )
        .expect("b");
    let c = q
        .enqueue(&ImageInput::Op(b.id), Some(&spec("resize 120x webp")), &[])
        .expect("c");
    let sink = MemorySink::default();
    let wanted: BTreeMap<OutputPath, _> = [
        (OutputPath::new(&format!("img/{}", b.file_name)), b.id),
        (OutputPath::new(&format!("img/{}", c.file_name)), c.id),
        (OutputPath::new(&format!("en/img/{}", c.file_name)), c.id),
    ]
    .into_iter()
    .collect();
    q.process(&wanted, &sink).expect("process");
    let files = sink.files.lock().expect("lock");
    assert_eq!(files.len(), 3, "intermediates (a, wm) are not published");
    for (path, id) in &wanted {
        let e = q.get(*id).expect("queued");
        let img = decode(&files[path]);
        assert_eq!(img.dimensions(), (e.width, e.height), "{path}");
    }
    assert!(files[&OutputPath::new(&format!("img/{}", c.file_name))].starts_with(b"RIFF"));
}

#[test]
fn cache_is_read_while_fresh() {
    let cache_dir = tempfile::tempdir().expect("tempdir");
    let cache = |max_age| {
        Some(ImageCache {
            dir: cache_dir.path().join("images"),
            max_age,
        })
    };
    let q = ImageQueue::new(Imaging::default(), cache(MaxAge::Forever));
    let e = q
        .enqueue(&photo(), Some(&spec("resize 64x png")), &[])
        .expect("e");
    let bytes = q.encoded(e.id).expect("encode");
    let cached = cache_dir.path().join("images").join(&e.file_name);
    assert_eq!(std::fs::read(&cached).expect("cache file"), &*bytes);

    // A second build reads the cache (a marker proves it) …
    std::fs::write(&cached, b"marker").expect("write marker");
    let q = ImageQueue::new(Imaging::default(), cache(MaxAge::Forever));
    let e2 = q
        .enqueue(&photo(), Some(&spec("resize 64x png")), &[])
        .expect("e2");
    assert_eq!(e2, e);
    assert_eq!(&*q.encoded(e.id).expect("cached"), b"marker");
    let q = ImageQueue::new(
        Imaging::default(),
        cache(MaxAge::For(Duration::from_secs(3600))),
    );
    q.enqueue(&photo(), Some(&spec("resize 64x png")), &[])
        .expect("e3");
    assert_eq!(&*q.encoded(e.id).expect("fresh"), b"marker");
    // … unless it is disabled (maxAge 0, --ignoreCache), which also rewrites the entry.
    let q = ImageQueue::new(Imaging::default(), cache(MaxAge::For(Duration::ZERO)));
    q.enqueue(&photo(), Some(&spec("resize 64x png")), &[])
        .expect("e4");
    assert_eq!(&*q.encoded(e.id).expect("recomputed"), &*bytes);
    assert_eq!(std::fs::read(&cached).expect("cache file"), &*bytes);
}

#[test]
fn cache_dir_comes_from_caches_images() {
    let file_cache = ssg_config::global::FileCache {
        dir: ":resourceDir/_gen".to_owned(),
        max_age: MaxAge::Forever,
        path: "/site/resources/_gen/images".into(),
        in_resource_dir: true,
    };
    let c = ImageCache::from_config(&file_cache);
    assert_eq!(c.dir, std::path::Path::new("/site/resources/_gen/images"));
    assert_eq!(c.max_age, MaxAge::Forever);
}

/// WebP: lossy at the spec's quality with the hint's preset and sharp YUV; transparency is
/// kept (`VP8X` + `ALPH`).
#[test]
fn webp_is_lossy_with_quality_and_alpha() {
    let q = ImageQueue::new(Imaging::default(), None);
    let size = |s: &str| {
        let e = q.enqueue(&photo(), Some(&spec(s)), &[]).expect("webp");
        q.encoded(e.id).expect("encode").len()
    };
    let q75 = size("resize 300x webp");
    assert!(size("resize 300x webp q10") < q75 && q75 < size("resize 300x webp q100"));
    let e = q
        .enqueue(&photo(), Some(&spec("resize 300x webp")), &[])
        .expect("webp");
    let bytes = q.encoded(e.id).expect("encode");
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(
        &bytes[8..16],
        b"WEBPVP8 ",
        "lossy VP8, no alpha for an opaque photo"
    );
    let jpg = q
        .enqueue(&photo(), Some(&spec("resize 300x")), &[])
        .expect("jpg");
    let db = psnr(&decode(&bytes), &decode(&q.encoded(jpg.id).expect("jpg")));
    assert!(db > 30.0, "webp q75 vs jpeg q75: {db:.1} dB");

    let dir = tempfile::tempdir().expect("tempdir");
    let half = RgbaImage::from_fn(32, 32, |x, _| {
        Rgba([200, 30, 30, if x < 16 { 255 } else { 0 }])
    });
    let src = ImageInput::File(write_file(dir.path(), "half.png", &png(&half)));
    let e = q
        .enqueue(&src, Some(&spec("webp")), &[])
        .expect("alpha webp");
    let bytes = q.encoded(e.id).expect("encode");
    assert_eq!(&bytes[12..16], b"VP8X");
    assert!(bytes.windows(4).any(|w| w == b"ALPH"));
    let out = decode(&bytes);
    assert_eq!(out.get_pixel(30, 5).0[3], 0);
    assert_eq!(out.get_pixel(2, 5).0[3], 255);

    // Every hint encodes.
    for hint in Hint::ALL {
        let s = format!("resize 100x webp {hint}");
        assert!(size(&s) > 0, "{s}");
    }
}

#[test]
fn every_format_encodes() {
    let q = ImageQueue::new(Imaging::default(), None);
    for f in ImageFormat::ALL {
        let e = q
            .enqueue(&photo(), Some(&spec(&format!("resize 40x {f}"))), &[])
            .expect("enqueue");
        assert_eq!(e.format, f);
        let bytes = q.encoded(e.id).expect("encode");
        let img = image::load_from_memory(&bytes).expect("decode");
        assert_eq!((img.width(), img.height()), (e.width, e.height), "{f}");
        assert_eq!(
            image::guess_format(&bytes).expect("guess"),
            match f {
                ImageFormat::Jpeg => image::ImageFormat::Jpeg,
                ImageFormat::Png => image::ImageFormat::Png,
                ImageFormat::Gif => image::ImageFormat::Gif,
                ImageFormat::Tiff => image::ImageFormat::Tiff,
                ImageFormat::Bmp => image::ImageFormat::Bmp,
                ImageFormat::Webp => image::ImageFormat::WebP,
            }
        );
    }
}

/// Identical bytes in two bundles under different names (`s00.jpg` and
/// `s15.jpg`): each result is named after its own source, whichever is queued first, and
/// only the hash digits (content + operation, as in Go) are shared.
#[test]
fn identical_bytes_keep_their_own_names() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bytes = std::fs::read(repo_file("resources/testdata/sunset.jpg")).expect("read");
    std::fs::create_dir_all(dir.path().join("a")).expect("mkdir");
    std::fs::create_dir_all(dir.path().join("b")).expect("mkdir");
    let s00 = ImageInput::File(write_file(&dir.path().join("a"), "s00.jpg", &bytes));
    let s15 = ImageInput::File(write_file(&dir.path().join("b"), "s15.jpg", &bytes));
    let names = |first: &ImageInput, second: &ImageInput| {
        let q = ImageQueue::new(Imaging::default(), None);
        let f = q.enqueue(first, Some(&spec("resize 30x")), &[]).expect("1");
        let s = q
            .enqueue(second, Some(&spec("resize 30x")), &[])
            .expect("2");
        let fc = q
            .enqueue(&ImageInput::Op(f.id), None, &[ImageFilter::Grayscale])
            .expect("1c");
        let sc = q
            .enqueue(&ImageInput::Op(s.id), None, &[ImageFilter::Grayscale])
            .expect("2c");
        for e in [&f, &s, &fc, &sc] {
            assert_eq!(q.get(e.id).as_ref(), Some(e), "get returns the caller's op");
        }
        let sink = MemorySink::default();
        let wanted: BTreeMap<OutputPath, _> = [&f, &s, &fc, &sc]
            .into_iter()
            .map(|e| (OutputPath::new(&e.file_name), e.id))
            .collect();
        q.process(&wanted, &sink).expect("process");
        assert_eq!(sink.files.lock().expect("lock").len(), 4);
        (f.file_name, s.file_name, fc.file_name, sc.file_name)
    };
    let (a00, a15, a00c, a15c) = names(&s00, &s15);
    let (b15, b00, b15c, b00c) = names(&s15, &s00);
    assert!(
        a00.starts_with("s00_hu_") && a15.starts_with("s15_hu_"),
        "{a00} {a15}"
    );
    assert!(
        a00c.starts_with("s00_hu_") && a15c.starts_with("s15_hu_"),
        "{a00c} {a15c}"
    );
    assert_eq!(
        (&a00, &a15, &a00c, &a15c),
        (&b00, &b15, &b00c, &b15c),
        "order-independent"
    );
    let digits = |n: &str| n.split_once("_hu_").expect("_hu_").1.to_owned();
    assert_eq!(
        digits(&a00),
        digits(&a15),
        "the digits hash content and operation"
    );
}

/// An image held in memory (a remote resource, a QR code) is processed like a file: its name
/// gives the result's stem, and the same bytes under the same name are the same operation as
/// the file's.
#[test]
fn images_in_memory_are_processed() {
    let q = ImageQueue::new(Imaging::default(), None);
    let ImageInput::File(path) = photo() else {
        unreachable!("a file")
    };
    let bytes: std::sync::Arc<[u8]> = std::fs::read(&path).expect("read").into();
    let memory = q.add_memory("remote/sunset.jpg", bytes);
    assert!(matches!(memory, ImageInput::Memory(_)), "{memory:?}");
    let from_memory = q
        .enqueue(&memory, Some(&spec("fill 60x40 webp")), &[])
        .expect("memory");
    let from_file = q
        .enqueue(&photo(), Some(&spec("fill 60x40 webp")), &[])
        .expect("file");
    assert!(
        from_memory.file_name.starts_with("sunset_hu_"),
        "{}",
        from_memory.file_name
    );
    assert_eq!(from_memory, from_file);
    // An overlay in memory, through the template layer's JSON form.
    let json = serde_json::to_value(&memory).expect("json");
    let overlay: ImageFilter =
        serde_json::from_value(serde_json::json!({"op": "overlay", "image": json, "x": 1, "y": 1}))
            .expect("filter");
    let marked = q
        .enqueue(&photo(), Some(&spec("resize 90x")), &[overlay])
        .expect("marked");
    let sink = MemorySink::default();
    let wanted: BTreeMap<OutputPath, _> = [
        (OutputPath::new(&from_memory.file_name), from_memory.id),
        (OutputPath::new(&marked.file_name), marked.id),
    ]
    .into_iter()
    .collect();
    q.process(&wanted, &sink).expect("process");
    let files = sink.files.lock().expect("lock");
    assert_eq!(
        decode(&files[&OutputPath::new(&from_memory.file_name)]).dimensions(),
        (60, 40)
    );
    assert_eq!(
        decode(&files[&OutputPath::new(&marked.file_name)]).width(),
        90
    );
    // Bytes the queue was never given.
    let unknown = ImageInput::Memory(ssg_images::MemoryImage {
        memory: 1,
        name: "gone.png".into(),
    });
    let e = q.enqueue(&unknown, None, &[]).expect_err("unknown");
    assert!(matches!(e, ImageError::UnknownMemory(_)), "{e}");
}

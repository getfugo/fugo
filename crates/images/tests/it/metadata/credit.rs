//! An image's own copyright notice and author ([`Credit`]): read from a resource's params with
//! `[imaging]`'s rules, and written in place of `[imaging]`'s, into originals and processed
//! images, per target and after the image cache.

use std::collections::BTreeMap;

use ssg_base::paths::OutputPath;
use ssg_base::{Map, Params};
use ssg_config::NoticeError;
use ssg_images::{Credit, CreditError, ImageInput, ImageMetadata, ImageQueue, Imaging};

use super::*;
use crate::common::MemorySink;

/// `[imaging]` of a site with its own notice, in 2031.
fn site() -> ImageMetadata {
    let cfg: ImagingConfig = serde_json::from_value(json!({
        "copyright": "© 2019–{year} Site",
        "author": "Site",
        "titleFromFileName": true,
    }))
    .expect("imaging config");
    ImageMetadata::from_config(&cfg, 2031)
}

fn params(v: serde_json::Value) -> Params {
    let ssg_base::Value::Map(m) = ssg_base::Value::from_json(v) else {
        panic!("a map")
    };
    Params::fold(&m)
}

/// EXIF `Artist` and `Copyright` of `bytes` (`None`: not written).
fn exif_notice(bytes: &[u8]) -> (Option<String>, Option<String>) {
    let e = exif::read(bytes, &all_tags()).expect("EXIF");
    let tag = |t: &str| {
        e.tags
            .get(t)
            .and_then(ssg_base::Value::as_str)
            .map(str::to_owned)
    };
    (tag("Artist"), tag("Copyright"))
}

#[test]
fn params_give_the_credit() {
    let m = site();
    let credit = m
        .credit(&params(json!({
            "Author": "Takeaway",
            "copyright": "© Takeaway {year}, CC BY-SA 3.0 (https://creativecommons.org/licenses/by-sa/3.0/)",
            "license": "CC BY-SA 3.0",
        })))
        .expect("valid");
    assert_eq!(
        credit,
        Some(Credit {
            copyright:
                "© Takeaway 2031, CC BY-SA 3.0 (https://creativecommons.org/licenses/by-sa/3.0/)"
                    .to_owned(),
            author: "Takeaway".to_owned(),
        })
    );
    // One of them replaces both of the site's: the other is empty.
    assert_eq!(
        m.credit(&params(json!({"author": "Ada"}))),
        Ok(Some(Credit {
            copyright: String::new(),
            author: "Ada".to_owned(),
        }))
    );
    assert_eq!(
        m.credit(&params(json!({"copyright": ""}))),
        Ok(Some(Credit::default()))
    );
    // Neither: the site's notice.
    assert_eq!(m.credit(&params(json!({"credit": "Ada"}))), Ok(None));
    assert_eq!(m.credit(&Params::fold(&Map::new())), Ok(None));
}

#[test]
fn params_are_checked_like_imaging() {
    let m = site();
    let long = "x".repeat(1001);
    for (p, want) in [
        (
            json!({"copyright": "© {yaer} A"}),
            CreditError::Text {
                key: "copyright",
                source: NoticeError::Placeholder("{yaer}".to_owned()),
            },
        ),
        (
            json!({"author": "A\nB"}),
            CreditError::Text {
                key: "author",
                source: NoticeError::Text,
            },
        ),
        (
            json!({"copyright": long}),
            CreditError::Text {
                key: "copyright",
                source: NoticeError::Text,
            },
        ),
        (
            json!({"copyright": "© A", "author": 2019}),
            CreditError::NotText {
                key: "author",
                found: "an integer",
            },
        ),
        (
            json!({"copyright": ["© A"]}),
            CreditError::NotText {
                key: "copyright",
                found: "a list",
            },
        ),
        (
            json!({"author": null}),
            CreditError::NotText {
                key: "author",
                found: "nothing",
            },
        ),
    ] {
        assert_eq!(m.credit(&params(p.clone())), Err(want), "{p}");
    }
    assert_eq!(
        m.credit(&params(json!({"copyright": "© {yaer} A"})))
            .expect_err("typo")
            .to_string(),
        "params.copyright: \"{yaer}\" is not a placeholder: the only one is {year}"
    );
    assert_eq!(
        m.credit(&params(json!({"author": true})))
            .expect_err("not text")
            .to_string(),
        "params.author: expected a string, found a boolean"
    );
    // An author is written as it is: braces are the copyright's placeholder only.
    assert!(m.credit(&params(json!({"author": "{A}"}))).is_ok());
}

/// Images published as they are: the params are the site's own, not read.
#[test]
fn without_strip_metadata_params_are_not_read() {
    for m in [
        ImageMetadata::KEEP,
        ImageMetadata {
            strip: false,
            ..site()
        },
    ] {
        assert_eq!(
            m.credit(&params(json!({"copyright": "© {yaer}"}))),
            Ok(None)
        );
        assert_eq!(m.credit(&params(json!({"author": 1}))), Ok(None));
    }
}

#[test]
fn originals_carry_the_credit() {
    let m = site();
    let photo = read("resources/testdata/sunset.jpg");
    let credit = Credit {
        copyright: "© Ada, CC BY 4.0".to_owned(),
        author: "Ada".to_owned(),
    };
    let out = m
        .original(&photo, "b/sunset.jpg", Some(&credit))
        .expect("rewritten");
    assert_eq!(
        exif_notice(&out),
        (Some("Ada".to_owned()), Some("© Ada, CC BY 4.0".to_owned()))
    );
    let rights = "<dc:rights><rdf:Alt><rdf:li xml:lang=\"x-default\">© Ada, CC BY 4.0<";
    assert!(contains(&out, rights.as_bytes()));
    assert!(contains(
        &out,
        b"<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">sunset<"
    ));
    assert!(!contains(&out, b"Site"), "nothing of the site's notice");
    // An author alone: no copyright notice at all, not the site's.
    let author = Credit {
        copyright: String::new(),
        author: "Ada".to_owned(),
    };
    let out = m
        .original(&photo, "sunset.jpg", Some(&author))
        .expect("rewritten");
    assert_eq!(exif_notice(&out), (Some("Ada".to_owned()), None));
    assert!(!contains(&out, b"dc:rights"));
    // Without a credit, the site's.
    let out = m.original(&photo, "sunset.jpg", None).expect("rewritten");
    assert_eq!(
        exif_notice(&out),
        (Some("Site".to_owned()), Some("© 2019–2031 Site".to_owned()))
    );
    // Published as they are, whatever the credit.
    assert_eq!(
        ImageMetadata::KEEP.original(&photo, "sunset.jpg", Some(&credit)),
        None
    );
}

/// One processed image published at two targets: one with its source's credit, one with the
/// site's notice; the cache keeps the encoder's bytes, without either.
#[test]
fn processed_images_carry_their_targets_credit() {
    let cache = tempfile::tempdir().expect("tempdir");
    let q = ImageQueue::new(
        Imaging {
            metadata: site(),
            ..Imaging::default()
        },
        Some(ssg_images::ImageCache {
            dir: cache.path().to_owned(),
            max_age: ssg_config::global::MaxAge::Forever,
        }),
    );
    let e = q
        .enqueue(
            &ImageInput::File(repo_file("resources/testdata/sunset.jpg")),
            Some(&"resize 60x".parse().expect("spec")),
            &[],
        )
        .expect("queued");
    let credited = OutputPath::new(&format!("a/{}", e.file_name));
    let site_notice = OutputPath::new(&format!("b/{}", e.file_name));
    let wanted: BTreeMap<OutputPath, _> = [(credited.clone(), e.id), (site_notice.clone(), e.id)]
        .into_iter()
        .collect();
    let credits: BTreeMap<OutputPath, Credit> = [(
        credited.clone(),
        Credit {
            copyright: "© Ada".to_owned(),
            author: "Ada".to_owned(),
        },
    )]
    .into_iter()
    .collect();
    let sink = MemorySink::default();
    q.process(&wanted, &credits, &sink).expect("process");
    let files = sink.files.lock().expect("lock");
    assert_eq!(
        exif_notice(&files[&credited]),
        (Some("Ada".to_owned()), Some("© Ada".to_owned()))
    );
    assert_eq!(
        exif_notice(&files[&site_notice]),
        (Some("Site".to_owned()), Some("© 2019–2031 Site".to_owned()))
    );
    // The title is the source's file name in both.
    for bytes in [&files[&credited], &files[&site_notice]] {
        assert!(contains(
            bytes,
            b"<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">sunset<"
        ));
    }
    let cached = std::fs::read(cache.path().join(&e.file_name)).expect("cached");
    assert!(
        exif::read(&cached, &all_tags()).is_none(),
        "the cache has no notice"
    );
    assert_eq!(&*q.encoded(e.id).expect("encoded"), cached.as_slice());
}

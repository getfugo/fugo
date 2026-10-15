//! Credits: an image's own copyright and author (`params_credit`), given to its file
//! (`set_credit`) and published with the file and with everything made from it (metadata
//! views, processed images, copies, fingerprints), in place of `[imaging]`'s; other images
//! keep the site's notice, and the image cache none.

use std::fs;
use std::str::FromStr;
use std::sync::Arc;

use serde_json::json;
use ssg_base::{Clock, Idx, LangIdx, ResourceId, Value};
use ssg_images::{Credit, CreditError, ImageQueue, ImageSpec, Imaging};
use ssg_resources::meta::ResourceMeta;
use ssg_resources::{
    BundleResource, CallSite, HashAlgo, PublishPolicy, ResourceError, ResourceStore, StoreConfig,
    Transform,
};
use ssg_testkit::fixture::repo_file;
use ssg_vfs::Vfs;

use crate::support::{MemSink, config};

/// A site with `[imaging]`'s notice, `content/p/{photo,other}.jpg` and `notes.txt`, its store
/// (with an image queue, in 2031) and the files registered, eager.
fn site(tmp: &std::path::Path) -> (ResourceStore, Arc<ImageQueue>, [ResourceId; 3]) {
    let site = tmp.join("site");
    fs::create_dir_all(site.join("content/p")).unwrap();
    fs::write(
        site.join("config.toml"),
        "baseURL = \"https://example.org/\"\n[imaging]\ncopyright = \"© {year} Site\"\n\
         author = \"Site\"\n",
    )
    .unwrap();
    let photo = fs::read(repo_file("resources/testdata/sunset.jpg")).unwrap();
    fs::write(site.join("content/p/photo.jpg"), &photo).unwrap();
    fs::write(site.join("content/p/other.jpg"), &photo).unwrap();
    fs::write(site.join("content/p/notes.txt"), "notes").unwrap();
    let cfg = config(&site, &tmp.join("home"));
    let clock = Clock("2031-06-01T00:00:00Z".parse().unwrap());
    let queue = Arc::new(ImageQueue::new(
        Imaging::from_config(&cfg.imaging, cfg.year(clock)).unwrap(),
        None,
    ));
    let s = ResourceStore::new(StoreConfig::from_config(
        &cfg,
        Some(Arc::new(Vfs::new(&cfg).unwrap())),
        Some(Arc::clone(&queue)),
        clock,
    ));
    let files = ["photo.jpg", "other.jpg", "notes.txt"].map(|name| {
        s.register_bundle(&BundleResource {
            lang: LangIdx::from_index(0),
            file: site.join("content/p").join(name),
            name: name.to_owned(),
            dir: "/p".to_owned(),
            policy: PublishPolicy::Eager,
        })
    });
    (s, queue, files)
}

/// `resources` front matter metadata.
fn meta(v: serde_json::Value) -> ResourceMeta {
    ResourceMeta::parse(&Value::from_json(v)).unwrap()
}

/// EXIF `Artist` and `Copyright` of a published file.
fn notice(files: &std::collections::BTreeMap<String, Vec<u8>>, path: &str) -> (String, String) {
    let all = ssg_images::ExifSettings {
        include: None,
        exclude: None,
        ..ssg_images::ExifSettings::default()
    };
    let bytes = files
        .get(path)
        .unwrap_or_else(|| panic!("{path} not published"));
    let e = ssg_images::exif::read(bytes, &all).unwrap_or_else(|| panic!("{path}: no EXIF"));
    let tag = |t: &str| {
        e.tags
            .get(t)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    (tag("Artist"), tag("Copyright"))
}

#[test]
fn a_files_credit_goes_with_what_is_made_from_it() {
    let tmp = tempfile::tempdir().unwrap();
    let (s, queue, [photo, other, notes]) = site(tmp.path());
    let m = meta(json!([
        {"src": "*.jpg", "params": {"license": "CC BY-SA 3.0"}},
        {"src": "photo.jpg", "params": {
            "author": "Takeaway",
            "copyright": "© Takeaway {year}, CC BY-SA 3.0",
        }},
        {"src": "notes.txt", "params": {"author": "Not an image"}},
    ]));
    let view = s.apply_meta(photo, &m);
    assert_ne!(view, photo, "a metadata view");
    let credit = Credit {
        copyright: "© Takeaway 2031, CC BY-SA 3.0".to_owned(),
        author: "Takeaway".to_owned(),
    };
    assert_eq!(s.params_credit(view).unwrap(), Some(credit.clone()));
    assert_eq!(
        s.params_credit(photo).unwrap(),
        None,
        "the file has no params"
    );
    assert_eq!(s.params_credit(s.apply_meta(other, &m)).unwrap(), None);
    // Params of other resources are the site's own.
    assert_eq!(s.params_credit(s.apply_meta(notes, &m)).unwrap(), None);

    assert_eq!(s.credit(view), None, "no credit until the file gets one");
    s.set_credit(photo, credit.clone());
    // Processed from the view, and from the file itself (a page without the metadata).
    let process = |from: ResourceId, spec: &str| {
        let e = queue
            .enqueue(
                &s.image_input(from).unwrap(),
                Some(&ImageSpec::from_str(spec).unwrap()),
                &[],
            )
            .unwrap();
        s.register_image(from, &e)
    };
    let small = process(view, "resize 60x");
    let gray = queue
        .enqueue(
            &s.image_input(small).unwrap(),
            None,
            &[ssg_images::ImageFilter::Grayscale],
        )
        .unwrap();
    let gray = s.register_image(small, &gray);
    let from_file = process(photo, "resize 40x");
    let from_other = process(other, "resize 40x");
    // The operation of `small` on the file: one target, two resources.
    let twice = process(photo, "resize 60x");
    assert_ne!(twice, small);
    assert_eq!(s.resource(twice).target, s.resource(small).target);
    let copy = s
        .copy(
            "og/photo.jpg",
            view,
            &CallSite::in_lang(LangIdx::from_index(0)),
        )
        .unwrap();
    let fingerprinted = s
        .transform(view, Transform::Fingerprint(HashAlgo::Sha256))
        .unwrap();
    for id in [view, small, gray, from_file, twice, copy, fingerprinted] {
        assert_eq!(
            s.credit(id).as_ref(),
            Some(&credit),
            "{}",
            s.resource(id).target
        );
    }
    assert_eq!(s.credit(from_other), None);
    assert_eq!(s.credit(other), None);

    let links: Vec<String> = [small, gray, from_file, from_other, copy, fingerprinted]
        .iter()
        .map(|&id| s.resource(id).rel_permalink.clone())
        .collect();
    let sink = MemSink::default();
    s.publish(links.iter().map(String::as_str), &sink).unwrap();
    let files = sink.0.lock().unwrap();
    let takeaway = (
        "Takeaway".to_owned(),
        "© Takeaway 2031, CC BY-SA 3.0".to_owned(),
    );
    let site = ("Site".to_owned(), "© 2031 Site".to_owned());
    for id in [photo, small, gray, from_file, copy, fingerprinted] {
        let path = s.resource(id).target.relative().to_owned();
        assert_eq!(notice(&files, &path), takeaway, "{path}");
    }
    for id in [other, from_other] {
        let path = s.resource(id).target.relative().to_owned();
        assert_eq!(notice(&files, &path), site, "{path}");
    }
    assert_eq!(files["p/notes.txt"], b"notes");
    // `.content` is the encoder's, as the image cache keeps it: no notice.
    let all = ssg_images::ExifSettings {
        include: None,
        exclude: None,
        ..ssg_images::ExifSettings::default()
    };
    assert!(ssg_images::exif::read(&s.content(small).unwrap(), &all).is_none());
}

#[test]
fn params_that_cannot_be_written_are_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let (s, _, [photo, ..]) = site(tmp.path());
    for (params, want) in [
        (
            json!({"copyright": "© {yaer} Ada"}),
            "params.copyright: \"{yaer}\" is not a placeholder: the only one is {year}",
        ),
        (
            json!({"author": "Ada\nLovelace"}),
            "params.author: must be at most 1000 bytes of text without control characters",
        ),
        (
            json!({"author": "x".repeat(1001)}),
            "params.author: must be at most 1000 bytes of text without control characters",
        ),
        (
            json!({"copyright": 2019}),
            "params.copyright: expected a string, found an integer",
        ),
    ] {
        let view = s.apply_meta(photo, &meta(json!([{"src": "*", "params": params}])));
        let e = s.params_credit(view).unwrap_err();
        assert!(matches!(e, ResourceError::Credit(_)), "{e:?}");
        assert_eq!(e.to_string(), want);
    }
    let view = s.apply_meta(
        photo,
        &meta(json!([{"src": "*", "params": {"author": []}}])),
    );
    assert!(matches!(
        s.params_credit(view),
        Err(ResourceError::Credit(CreditError::NotText {
            key: "author",
            found: "a list"
        }))
    ));
}

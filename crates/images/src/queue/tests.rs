use ssg_testkit::fixture::repo_file;

use super::*;

fn spec(s: &str) -> ImageSpec {
    s.parse().expect("spec")
}

fn photo() -> ImageInput {
    ImageInput::File(repo_file("resources/testdata/sunset.jpg"))
}

fn sorted(mut ids: Vec<ImageOpId>) -> Vec<ImageOpId> {
    ids.sort_unstable();
    ids
}

/// Two operations reading one unprocessed operation: it is processed in an earlier stage,
/// once, as is an overlay's image.
#[test]
fn stages_process_what_is_read_first_and_once() {
    let q = ImageQueue::new(Imaging::default(), None);
    let crop = q
        .enqueue(&photo(), Some(&spec("crop 200x200")), &[])
        .expect("crop");
    let small = q
        .enqueue(&ImageInput::Op(crop.id), Some(&spec("resize 50x")), &[])
        .expect("small");
    let large = q
        .enqueue(&ImageInput::Op(crop.id), Some(&spec("resize 100x")), &[])
        .expect("large");
    let mark = q
        .enqueue(&photo(), Some(&spec("resize 20x")), &[])
        .expect("mark");
    let marked = q
        .enqueue(
            &ImageInput::Op(large.id),
            None,
            &[ImageFilter::Overlay {
                image: ImageInput::Op(mark.id),
                x: 0,
                y: 0,
            }],
        )
        .expect("marked");
    let stages = q.stages(&[small.id, marked.id]);
    assert_eq!(stages.len(), 3, "{stages:?}");
    assert_eq!(sorted(stages[0].clone()), sorted(vec![crop.id, mark.id]));
    assert_eq!(sorted(stages[1].clone()), sorted(vec![small.id, large.id]));
    assert_eq!(stages[2], [marked.id]);
}

/// A result in the file cache is read from there: what it reads is not processed.
#[test]
fn stages_skip_what_a_cached_result_reads() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = ImageCache {
        dir: dir.path().to_owned(),
        max_age: MaxAge::Forever,
    };
    let q = ImageQueue::new(Imaging::default(), Some(cache));
    let crop = q
        .enqueue(&photo(), Some(&spec("crop 200x200")), &[])
        .expect("crop");
    let small = q
        .enqueue(&ImageInput::Op(crop.id), Some(&spec("resize 50x")), &[])
        .expect("small");
    assert_eq!(q.stages(&[small.id]), [vec![crop.id], vec![small.id]]);
    fs::write(dir.path().join(&small.file_name), b"cached").expect("write");
    assert_eq!(q.stages(&[small.id]), [vec![small.id]]);
}

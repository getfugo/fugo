use super::*;

/// Go's `TestIterators`.
#[test]
fn iterators() {
    let pairs = |it: &mut dyn Iterator<Item = Index>| -> Vec<(i32, i32)> {
        it.map(|i| (i.x, i.y)).collect()
    };
    // 1 3
    // 2 4
    assert_eq!(pairs(&mut up_down(2, 2)), [(0, 0), (0, 1), (1, 0), (1, 1)]);
    // 1 2
    // 3 4
    assert_eq!(
        pairs(&mut left_right(2, 2)),
        [(0, 0), (1, 0), (0, 1), (1, 1)]
    );
    // 1 3
    // 2 5
    // 4 6
    assert_eq!(
        pairs(&mut diag_up(2, 3)),
        [(0, 0), (0, 1), (1, 0), (0, 2), (1, 1), (1, 2)]
    );
    // 2 4 6
    // 1 3 5
    assert_eq!(
        pairs(&mut diag_down(3, 2)),
        [(0, 1), (0, 0), (1, 1), (1, 0), (2, 1), (2, 0)]
    );
}

/// Go's `TestReadASCII`.
#[test]
fn read_ascii() {
    let canvas = Canvas::new(" +-->\n | å\n +----->").expect("small");
    assert_eq!((canvas.width, canvas.height), (8, 3));
    assert_eq!(canvas.to_text(), " +-->   \n | å    \n +----->\n");
}

/// Lines as `bufio.ScanLines` splits them.
#[test]
fn scan_lines() {
    let size = |s: &str| {
        let c = Canvas::new(s).expect("small");
        (c.width, c.height)
    };
    assert_eq!(size(""), (0, 0));
    assert_eq!(size("\n"), (0, 1));
    assert_eq!(size("ab\r\nc\r"), (2, 2));
    assert_eq!(size("a\n\n"), (1, 2));
    assert_eq!(size("\ta\r\r\n"), (3, 1));
}

#[test]
fn text_is_set_apart() {
    let canvas = Canvas::new("+--+ foo\n|  |-->").expect("small");
    assert_eq!(canvas.to_text(), "+--+ foo\n|  |--> \n");
    let text: String = canvas
        .text()
        .iter()
        .filter_map(|s| match s {
            Shape::Text(t) => Some(t.ch),
            _ => None,
        })
        .collect();
    assert_eq!(text, "foo");
}

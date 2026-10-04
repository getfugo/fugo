use super::*;
use crate::font::FontData;

fn go_regular() -> FontData {
    FontData::go_regular()
}

fn lines(spec: &TextSpec, width: u32) -> Vec<Line> {
    let font = go_regular();
    let face = Face::new(font.bytes(), spec.size, "test").expect("face");
    layout(&face, spec, width)
}

fn width(spec: &TextSpec, s: &str) -> i64 {
    let font = go_regular();
    let face = Face::new(font.bytes(), spec.size, "test").expect("face");
    ceil_26_6(face.measure(s))
}

#[test]
fn defaults_are_go_s() {
    let t = TextSpec::new("x");
    assert_eq!((t.size, t.x, t.y, t.line_spacing), (20.0, 10, 10, 2));
    assert_eq!(
        (t.align_x, t.align_y, t.color),
        (AlignX::Left, AlignY::Top, Color::WHITE)
    );
    assert!(t.font.is_none());
}

#[test]
fn words_wrap_at_the_available_width() {
    let spec = TextSpec::new("aaa bbb ccc ddd eee fff ggg hhh iii jjj kkk lll mmm");
    let out = lines(&spec, 200);
    // Left: 200 − 20 − x.
    let available = 200 - 20 - spec.x;
    assert!(out.len() > 1, "{out:?}");
    for pair in out.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        // Each line would have overflowed with the next line's first word (Go measures the
        // line with its trailing space).
        let first = b.text.split(' ').next().expect("word");
        assert!(
            width(&spec, &format!("{} ", a.text)) + width(&spec, first) >= available,
            "{a:?} {b:?}"
        );
        assert!(width(&spec, &a.text) < available, "{a:?}");
    }
    let words: Vec<&str> = out.iter().flat_map(|l| l.text.split(' ')).collect();
    assert_eq!(words.join(" "), spec.text);
}

#[test]
fn a_word_wider_than_the_space_leaves_an_empty_line_before_it() {
    let spec = TextSpec::new("Supercalifragilisticexpialidocious fits");
    let out = lines(&spec, 60);
    assert_eq!(out[0].text, "");
    assert_eq!(out[1].text, "Supercalifragilisticexpialidocious");
}

#[test]
fn line_breaks_and_carriage_returns() {
    let spec = TextSpec::new("one\r\ntwo\n\nfour");
    let texts: Vec<String> = lines(&spec, 1000).into_iter().map(|l| l.text).collect();
    assert_eq!(texts, ["one", "two", "", "four"]);
}

#[test]
fn placement_and_alignment() {
    // Go Regular at 20 px: ascent 1209/64 → font height 19.
    let base = TextSpec {
        x: 100,
        y: 50,
        line_spacing: 5,
        ..TextSpec::new("Hello\nWorld wide")
    };
    let left = lines(&base, 1000);
    assert_eq!(
        left.iter().map(|l| (l.x, l.y)).collect::<Vec<_>>(),
        [(100, 69), (100, 93)]
    );
    let right = lines(
        &TextSpec {
            align_x: AlignX::Right,
            ..base.clone()
        },
        1000,
    );
    for l in &right {
        assert_eq!(l.x, 100 - width(&base, &l.text));
    }
    let center = lines(
        &TextSpec {
            align_x: AlignX::Center,
            ..base.clone()
        },
        1000,
    );
    for l in &center {
        assert_eq!(l.x, 100 - width(&base, &l.text) / 2);
    }
    // Two lines: total height 19 + 5 + 19 = 43.
    let middle = lines(
        &TextSpec {
            align_y: AlignY::Center,
            ..base.clone()
        },
        1000,
    );
    assert_eq!(middle[0].y, 69 - 43 / 2);
    let bottom = lines(
        &TextSpec {
            align_y: AlignY::Bottom,
            ..base.clone()
        },
        1000,
    );
    assert_eq!(bottom[0].y, 69 - 43);
    assert_eq!(bottom[1].y, 69 - 43 + 19 + 5);
}

#[test]
fn available_width_per_alignment() {
    // One long run of short words: the widest line shows the available width.
    let text = "ab ".repeat(60);
    let widest = |align_x, x| {
        let spec = TextSpec {
            x,
            align_x,
            ..TextSpec::new(text.clone())
        };
        lines(&spec, 420)
            .iter()
            .map(|l| width(&spec, &l.text))
            .max()
            .unwrap_or(0)
    };
    // Left at x = 10: up to 390; right at x = 300: up to 300; center at x = 100:
    // 2·min(400 − 100, 100) = 200.
    let (l, r, c) = (
        widest(AlignX::Left, 10),
        widest(AlignX::Right, 300),
        widest(AlignX::Center, 100),
    );
    assert!(l < 390 && l > 330, "{l}");
    assert!(r < 300 && r > 240, "{r}");
    assert!(c < 200 && c > 140, "{c}");
}

#[test]
fn options_from_template_maps() {
    let t: TextSpec = serde_json::from_value(serde_json::json!({
        "text": "Hi", "color": "#FFF", "size": "70", "x": 65.9, "y": "80",
        "alignX": "center", "align_y": "bottom", "line_spacing": 12.8,
    }))
    .expect("text");
    assert_eq!((t.size, t.x, t.y, t.line_spacing), (70.0, 65, 80, 12));
    assert_eq!((t.align_x, t.align_y), (AlignX::Center, AlignY::Bottom));
    assert_eq!(t.color, Color::WHITE);
    for bad in [
        serde_json::json!({}),
        serde_json::json!({"text": "x", "size": 0}),
        serde_json::json!({"text": "x", "size": 20_000}),
        serde_json::json!({"text": "x", "size": "big"}),
        serde_json::json!({"text": "x", "x": 1e18}),
        serde_json::json!({"text": "x", "linespacing": i64::MIN}),
        serde_json::json!({"text": "x", "alignx": "middle"}),
        serde_json::json!({"text": "x", "wobble": 1}),
        serde_json::json!({"text": "x", "color": "#12"}),
    ] {
        assert!(
            serde_json::from_value::<TextSpec>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

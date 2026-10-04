//! The port against the values its first implementation (Python 3.14) gave for the same inputs
//! (`tests/data/reference.json`, written once by that implementation): the HTML tokenizer, the
//! URL functions, JSON, `repr` and float formatting, the word hunks, the change patterns and the
//! diff fingerprints the committed baselines record. The golden manifests and baselines were
//! written by that implementation, so any difference here is a regression.

use std::path::Path;

use pretty_assertions::assert_eq;
use ssg_dev::html;
use ssg_dev::manifest::{self, HtmlScan, Urls, XmlScan};
use ssg_dev::py::{self, Py};
use ssg_dev::{difflib, fnmatch, structdiff, url};

fn reference() -> Py {
    manifest::read_json(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/reference.json"))
        .expect("the reference")
}

fn cases<'a>(r: &'a Py, section: &str) -> &'a [Py] {
    r.get(section)
        .and_then(Py::as_list)
        .unwrap_or_else(|| panic!("no section {section}"))
}

fn s(v: &Py) -> &str {
    v.as_str()
        .unwrap_or_else(|| panic!("not a string: {}", py::repr(v)))
}

fn strings(v: &Py) -> Vec<String> {
    v.as_list()
        .unwrap_or(&[])
        .iter()
        .map(|x| s(x).to_owned())
        .collect()
}

fn opt(v: &Py) -> Option<String> {
    v.as_str().map(str::to_owned)
}

#[test]
fn html_scan() {
    let r = reference();
    for c in cases(&r, "html") {
        let input = s(c.get_or_none("in"));
        let scan = HtmlScan::scan(input);
        let rel: Vec<Vec<String>> = scan.rel_links.iter().map(|l| l.to_vec()).collect();
        let want_rel: Vec<Vec<String>> = c
            .get_or_none("rel")
            .as_list()
            .unwrap_or(&[])
            .iter()
            .map(strings)
            .collect();
        assert_eq!(
            scan.title,
            strings(c.get_or_none("title")),
            "title of {input:?}"
        );
        assert_eq!(rel, want_rel, "rel of {input:?}");
        assert_eq!(
            scan.urls,
            strings(c.get_or_none("urls")),
            "urls of {input:?}"
        );
        assert_eq!(
            scan.refresh,
            opt(c.get_or_none("refresh")),
            "refresh of {input:?}"
        );
        assert_eq!(scan.text, s(c.get_or_none("text")), "text of {input:?}");
        assert_eq!(
            scan.visible_text(),
            s(c.get_or_none("visible")),
            "visible text of {input:?}"
        );
        assert_eq!(scan.ids, strings(c.get_or_none("ids")), "ids of {input:?}");
    }
}

#[test]
fn xml_scan() {
    let r = reference();
    for c in cases(&r, "xml") {
        let input = s(c.get_or_none("in"));
        let mut scan = XmlScan::default();
        html::parse(input, &mut scan);
        let items: Vec<Vec<String>> = scan
            .items
            .iter()
            .map(|(k, v)| vec![k.clone(), v.clone()])
            .collect();
        let want: Vec<Vec<String>> = c
            .get_or_none("items")
            .as_list()
            .unwrap_or(&[])
            .iter()
            .map(strings)
            .collect();
        assert_eq!(items, want, "{input:?}");
    }
}

#[test]
fn unescape() {
    let r = reference();
    for c in cases(&r, "unescape") {
        let [input, want] = c.as_list().expect("a pair") else {
            panic!("a pair")
        };
        assert_eq!(html::unescape(s(input)), s(want), "{:?}", s(input));
    }
}

#[test]
fn urls() {
    let r = reference();
    let u = r.get_or_none("urls");
    let urls = Urls::new(&strings(u.get_or_none("bases"))).expect("base URLs");
    for c in u.get_or_none("cases").as_list().expect("cases") {
        let [x, page, internal, any] = c.as_list().expect("a case") else {
            panic!("a case")
        };
        assert_eq!(
            urls.internal(s(x), s(page)).expect("a URL"),
            opt(internal),
            "internal({x:?}, {page:?})"
        );
        assert_eq!(
            urls.any(s(x), s(page)).expect("a URL"),
            s(any),
            "any({x:?}, {page:?})"
        );
    }
    for c in cases(&r, "urljoin") {
        let [base, x, want] = c.as_list().expect("a case") else {
            panic!("a case")
        };
        assert_eq!(
            url::urljoin(s(base), s(x)).expect("URLs"),
            s(want),
            "urljoin({base:?}, {x:?})"
        );
    }
    for c in cases(&r, "quote") {
        let [x, want] = c.as_list().expect("a pair") else {
            panic!("a pair")
        };
        assert_eq!(url::quote(s(x), ":/?#[]@!$&'()*+,;=%~"), s(want));
    }
    for c in cases(&r, "unquote") {
        let [x, want] = c.as_list().expect("a pair") else {
            panic!("a pair")
        };
        assert_eq!(url::unquote(s(x)), s(want), "{x:?}");
    }
}

#[test]
fn json_and_numbers() {
    let r = reference();
    for c in cases(&r, "json") {
        let [text, sorted, ascii, indent0] = c.as_list().expect("a case") else {
            panic!("a case")
        };
        let v = py::loads(s(text)).expect("JSON");
        assert_eq!(py::dumps(&v, false), s(sorted));
        assert_eq!(py::dumps(&v, true), s(ascii));
        assert_eq!(py::dumps_indent0(&v), s(indent0));
    }
    for c in cases(&r, "floats") {
        let [text, rounded, fixed] = c.as_list().expect("a case") else {
            panic!("a case")
        };
        let f: f64 = s(text).parse().expect("a float");
        assert_eq!(py::float_repr(f), s(text));
        assert_eq!(
            py::round(f, 4).to_bits(),
            rounded.as_f64().expect("a float").to_bits(),
            "round({f})"
        );
        assert_eq!(format!("{f:.4}"), s(fixed));
    }
    for c in cases(&r, "sum") {
        let [xs, total] = c.as_list().expect("a case") else {
            panic!("a case")
        };
        let xs: Vec<f64> = xs
            .as_list()
            .expect("floats")
            .iter()
            .map(|x| x.as_f64().expect("a float"))
            .collect();
        assert_eq!(
            py::sum_floats(xs.iter().copied()).to_bits(),
            total.as_f64().expect("a float").to_bits(),
            "{xs:?}"
        );
    }
    assert!(py::loads("[NaN, -Infinity]").is_ok());
    assert!(py::loads("\u{feff}{}").is_err());
    assert!(py::loads("[1,]").is_err());
    assert!(py::loads("\"\u{1}\"").is_err());
}

#[test]
fn repr_and_text() {
    let r = reference();
    for c in cases(&r, "repr") {
        let [x, want] = c.as_list().expect("a pair") else {
            panic!("a pair")
        };
        assert_eq!(py::repr(x), s(want));
    }
    for c in cases(&r, "text") {
        let [x, split, strip, lines] = c.as_list().expect("a case") else {
            panic!("a case")
        };
        assert_eq!(py::split_ws(s(x)), strings(split), "split {x:?}");
        assert_eq!(py::strip(s(x)), s(strip), "strip {x:?}");
        assert_eq!(py::splitlines(s(x)), strings(lines), "splitlines {x:?}");
    }
    for c in cases(&r, "norm_path") {
        let [x, want] = c.as_list().expect("a pair") else {
            panic!("a pair")
        };
        assert_eq!(manifest::norm_path(s(x)), s(want));
    }
    for c in cases(&r, "kind_of") {
        let [x, want] = c.as_list().expect("a pair") else {
            panic!("a pair")
        };
        assert_eq!(manifest::kind_of(s(x)), s(want), "{x:?}");
    }
    for c in cases(&r, "page_url") {
        let [x, want] = c.as_list().expect("a pair") else {
            panic!("a pair")
        };
        assert_eq!(manifest::page_url(s(x)), s(want));
    }
}

#[test]
fn hunks_patterns_and_fingerprints() {
    let r = reference();
    for c in cases(&r, "difflib") {
        let [a, b, ops] = c.as_list().expect("a case") else {
            panic!("a case")
        };
        let got: Vec<Py> = difflib::opcodes(&py::split_ws(s(a)), &py::split_ws(s(b)))
            .into_iter()
            .map(|(t, i1, i2, j1, j2)| {
                Py::List(vec![t.into(), i1.into(), i2.into(), j1.into(), j2.into()])
            })
            .collect();
        assert_eq!(
            py::dumps(&Py::List(got), false),
            py::dumps(ops, false),
            "{a:?} / {b:?}"
        );
    }
    for c in cases(&r, "hunks") {
        let [a, b, hunks] = c.as_list().expect("a case") else {
            panic!("a case")
        };
        assert_eq!(structdiff::text_hunks(s(a), s(b), 8, 20), strings(hunks));
    }
    for c in cases(&r, "fnmatch") {
        let [name, pattern, want] = c.as_list().expect("a case") else {
            panic!("a case")
        };
        assert_eq!(
            fnmatch::fnmatchcase(s(name), s(pattern)),
            want.truthy(),
            "fnmatchcase({name:?}, {pattern:?})"
        );
    }
    for c in cases(&r, "fingerprint") {
        let [parts, fp] = c.as_list().expect("a pair") else {
            panic!("a pair")
        };
        assert_eq!(
            structdiff::fingerprint(parts.as_list().expect("parts")),
            s(fp)
        );
    }
}

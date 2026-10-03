//! The LiveReload script injection against the Go oracle `transform/absurl/inject`.

use serde::Deserialize;
use ssg_base::url::UrlRef;
use ssg_publish::livereload;
use ssg_testkit::fixture::{GoString, oracle_lines};

use crate::support::{Tally, show};

#[derive(Deserialize)]
struct Case {
    t: String,
    #[serde(rename = "in")]
    input: GoString,
    out: GoString,
    url: Option<String>,
}

#[test]
fn inject_oracle() {
    let cases: Vec<Case> = oracle_lines("oracle/transform/absurl/inject.jsonl.gz");
    let mut t = Tally::default();
    for c in &cases {
        match (c.t.as_str(), &c.url) {
            ("livereload", Some(url)) => {
                let base = UrlRef::parse(url).expect("the oracle's URLs parse");
                let got = livereload::inject(&c.input.0, &livereload::script(&base));
                t.check(got == c.out.0, || {
                    format!(
                        "{url} in={} got={} want={}",
                        show(&c.input),
                        show(&GoString(got.clone())),
                        show(&c.out)
                    )
                });
            }
            // This port never adds the generator <meta> tag (output-publishing §0.4).
            ("generator", _) if c.out != c.input => t.accept("generator-tag-never-injected"),
            ("generator", _) => t.pass(),
            _ => panic!("unknown record {}", c.t),
        }
    }
    assert_eq!(t.total, 200);
    t.finish("inject");
}

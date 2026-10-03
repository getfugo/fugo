//! `remarshal` against Go's `transform.Remarshal` (yaml.v2, go-toml v2, encoding/json):
//! `tests/fixtures/remarshal/go.txt` is the Go build of the template below over `a.toml` and
//! `b.toml` (quoting, key order, dates, floats, tables, literal blocks). Byte-equal.

use tera::Context;

use crate::support::Harness;

const TEMPLATE: &str = "
===A-TOML
{{ a | remarshal(format='toml') | safe }}
===A-YAML
{{ a | remarshal(format='yaml') | safe }}
===A-JSON
{{ a | remarshal(format='json') | safe }}
===B-YAML
{{ b | remarshal(format='yaml') | safe }}
===B-TOML
{{ b | remarshal(format='toml') | safe }}
===B-JSON
{{ b | remarshal(format='json') | safe }}
";

#[test]
fn equals_go() {
    let dir = ssg_testkit::fixture::repo_dir().join("crates/funcs/tests/fixtures/remarshal");
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).expect("fixture");
    let mut ctx = Context::new();
    ctx.insert("a", &read("a.toml"));
    ctx.insert("b", &read("b.toml"));
    let got = Harness::new().render(TEMPLATE, &ctx).expect("renders");
    pretty_assertions::assert_eq!(got, read("go.txt"));
}

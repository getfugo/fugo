use super::*;

#[test]
fn yaml_scalars_as_yaml_v2() {
    let s = |x: &str| yaml_string(x, Some(2));
    assert_eq!(s(":resourceDir/_gen"), ":resourceDir/_gen");
    assert_eq!(s("{{ .Date }}"), "'{{ .Date }}'");
    assert_eq!(s("a: b"), "'a: b'");
    assert_eq!(s("true"), "\"true\"");
    assert_eq!(s("1.5"), "\"1.5\"");
    assert_eq!(s("0x1F"), "\"0x1F\"");
    assert_eq!(s("2023-01-01"), "\"2023-01-01\"");
    assert_eq!(s("1:20"), "\"1:20\"");
    assert_eq!(s(""), "\"\"");
    assert_eq!(s("- x"), "'- x'");
    assert_eq!(s("-x"), "-x");
    assert_eq!(s("it's"), "it's");
    assert_eq!(s("a #b"), "'a #b'");
    assert_eq!(s("a\tb"), "\"a\\tb\"");
    assert_eq!(s("l1\nl2"), "|-\n  l1\n  l2");
    assert_eq!(s("l1\nl2\n"), "|\n  l1\n  l2");
    assert_eq!(yaml_string("l1\nl2", None), "\"l1\\nl2\"");
}

#[test]
fn yaml_key_order_is_natural() {
    let mut keys = vec!["b", "a10", "a9", "_x", "Z", "a"];
    keys.sort_by(|a, b| yaml_key_order(a, b));
    assert_eq!(keys, ["_x", "Z", "a", "a9", "a10", "b"]);
}

#[test]
fn yaml_floats_as_go() {
    assert_eq!(yaml_float(1.5), "1.5");
    assert_eq!(yaml_float(1e21), "1e+21");
    assert_eq!(yaml_float(1_234_567.5), "1.2345675e+06");
    assert_eq!(yaml_float(123_456.5), "123456.5");
    assert_eq!(yaml_float(1e-5), "1e-05");
    assert_eq!(yaml_float(0.0001), "0.0001");
}

#[test]
fn toml_strings_as_go_toml() {
    assert_eq!(toml_string("red"), "'red'");
    assert_eq!(toml_string(r"a\.b"), r"'a\.b'");
    assert_eq!(toml_string("it's"), "\"it's\"");
    assert_eq!(toml_string("a\nb"), "\"a\\nb\"");
    assert_eq!(toml_key("text/html"), "'text/html'");
    assert_eq!(toml_key("a-b_c"), "a-b_c");
    assert_eq!(toml_key("it's"), "\"it's\"");
}

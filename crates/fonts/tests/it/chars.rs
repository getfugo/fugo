//! The characters of CSS strings and of pages' text.

use std::collections::BTreeSet;

use ssg_fonts::{CssStrings, add_html_text};

fn css(text: &str) -> String {
    let mut strings = CssStrings::default();
    strings.add(text);
    strings.chars().into_iter().collect()
}

fn html(text: &str) -> String {
    let mut out = BTreeSet::new();
    add_html_text(text, &mut out);
    out.into_iter().collect()
}

#[test]
fn content_strings_with_escapes() {
    assert_eq!(css(r#".fa-star::before{content:"\f005"}"#), "\u{f005}");
    // Single quotes, a space ending the escape, then a letter; an escaped quote.
    assert_eq!(css(r".a:before { content: '\f00d B\'' }"), "'B\u{f00d}");
    // Six digits at most; a character that is not a hex digit stands for itself.
    assert_eq!(css(r#"p::after{content:"\01F600x\;"}"#), ";x\u{1f600}");
    // A raw character (what compressed Sass writes).
    assert_eq!(css(".a::before{content:\"\u{f015}\"}"), "\u{f015}");
    // Zero is not a character.
    assert_eq!(css(r#"p::after{content:"\0"}"#), "\u{fffd}");
}

#[test]
fn quotes_and_custom_properties() {
    assert_eq!(css(r#"q{quotes:"«" "»"}"#), "«»");
    // Font Awesome 6.6 and later: `content: var(--fa)`, here through another property.
    assert_eq!(
        css(
            r#".fa-x{--fa-icon:"\e61a"}.fa-x{--fa: var( --fa-icon )}.fa::before{content:var(--fa)}"#
        ),
        "\u{e61a}"
    );
    // A custom property no `content` uses does not count.
    assert_eq!(css(r#":root{--bs-font:"Segoe UI";--fa:"x"}"#), "");
}

#[test]
fn images_are_not_text() {
    assert_eq!(
        css(r#"a::before{content:url("data:image/svg+xml,%3csvg xmlns='x'") "\f00c"}"#),
        "\u{f00c}"
    );
    assert_eq!(css("a::before{content:URL( a.png ) 'b'}"), "b");
}

#[test]
fn custom_properties_of_several_files() {
    let mut site = CssStrings::default();
    for file in [
        r#".fa::before{content:var(--fa)}"#,
        r#".fa-star{--fa:"\f005"}"#,
    ] {
        let mut one = CssStrings::default();
        one.add(file);
        site.merge(one);
    }
    assert_eq!(site.chars().into_iter().collect::<String>(), "\u{f005}");
}

#[test]
fn other_declarations_and_bad_strings_do_not_count() {
    assert_eq!(
        css(r#"a{font-family:"Ab";align-content:center;background:url("c")}"#),
        ""
    );
    // A string not closed before the end of the line, or of the value.
    assert_eq!(css("a::before{content:\"x\n}"), "");
    assert_eq!(css(r#"a::before{content:"x}"#), "");
}

#[test]
fn css_in_a_page() {
    let page = r#"<style>.i::before{content:"\f007"}</style><i style="content:'\f00c'">z</i>"#;
    assert_eq!(css(page), "\u{f007}\u{f00c}");
}

#[test]
fn page_text() {
    let page = r#"<!DOCTYPE html><title>Ti</title><p>Caf&eacute;&nbsp;<b>A</b></p>
<script>var z = "Z";</script><style>.q::before{content:"Q"}</style>
<img alt="Ω" src="x.png"><input placeholder="Ü" value="ß" data-x="Y"><!-- C -->"#;
    let expected: BTreeSet<char> = "TiCaf\u{e9}\u{a0}AΩÜß".chars().collect();
    assert_eq!(html(page), expected.into_iter().collect::<String>());
}

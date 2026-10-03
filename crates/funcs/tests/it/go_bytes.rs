//! Byte-for-byte agreement with Go where the embedded templates depend on it: `xml_escape`
//! (`transform.XMLEscape`, the RSS description) and the English names of `date(format=)`
//! (Go's `Time.Format`).

use tera::Context;

use crate::support::Harness;

#[test]
fn xml_escape_is_go_xml_escape_text() {
    let h = Harness::new();
    let mut ctx = Context::new();
    ctx.insert(
        "s",
        "<p class=\"x\">'a' & b</p>\n<pre>\tc\r</pre>\u{1}\u{b}\u{fffe}\u{ffff}é\u{1f600}",
    );
    // Go: transform.XMLEscape (forbidden characters dropped, then xml.EscapeText).
    assert_eq!(
        h.render("{{ s | xml_escape }}", &ctx).unwrap(),
        "&lt;p class=&#34;x&#34;&gt;&#39;a&#39; &amp; b&lt;/p&gt;&#xA;&lt;pre&gt;&#x9;c&#xD;&lt;/pre&gt;é\u{1f600}"
    );
    // Safe input is escaped too, and the result is not escaped again.
    assert_eq!(
        h.render("{{ '<b>\u{a}</b>' | safe | xml_escape }}", &ctx)
            .unwrap(),
        "&lt;b&gt;&#xA;&lt;/b&gt;"
    );
}

#[test]
fn date_format_names_are_english_unless_a_locale_is_given() {
    let h = Harness::new();
    let ctx = Context::new();
    let d = "'2024-02-29T08:00:00+00:00'";
    // Go's `.Lastmod.Format "Mon Jan 2, 2006"` is English in every language.
    assert_eq!(
        h.render(
            &format!("{{% set lang = 'th' %}}{{{{ {d} | date(format='%a %b %-d, %Y') }}}}"),
            &ctx
        )
        .unwrap(),
        "Thu Feb 29, 2024"
    );
    // `time.Format` localizes: `locale=` (here the render's `lang`).
    let th = h
        .render(
            &format!("{{% set lang = 'th' %}}{{{{ {d} | date(format='%b', locale=lang) }}}}"),
            &ctx,
        )
        .unwrap();
    assert_ne!(th, "Feb");
    assert_eq!(
        h.render(
            &format!("{{{{ {d} | date(format='%B', locale='en') }}}}"),
            &ctx
        )
        .unwrap(),
        "February"
    );
    // A style is always localized in the render's `lang`.
    let long_th = h
        .render(
            &format!("{{% set lang = 'th' %}}{{{{ {d} | date(style='long') }}}}"),
            &ctx,
        )
        .unwrap();
    assert!(!long_th.contains("February"), "{long_th}");
}

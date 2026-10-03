//! `url::BaseUrl` and `url::UrlRef` against the `common/urls` oracle.

use serde_json::{Value as J, json};
use ssg_base::url::{BaseUrl, UrlError};

use crate::support::{Tally, fixture, text};

fn dump(b: &BaseUrl) -> J {
    json!({
        "String": b.as_str(),
        "WithPath": b.as_str(),
        "WithPathNoTrailingSlash": b.with_path_no_trailing_slash(),
        "WithoutPath": b.without_path(),
        "BasePath": b.base_path(),
        "BasePathNoTrailingSlash": b.base_path_no_trailing_slash(),
        "Path": b.base_path(),
        "HostURL": b.host_url(),
        "Port": b.port().unwrap_or(0),
        "URL": b.as_str(),
    })
}

/// Go error texts are not reproduced: an error matches an error.
fn same(want: &J, got: &Result<BaseUrl, UrlError>) -> bool {
    match (want.get("ok"), got) {
        (Some(w), Ok(b)) => *w == dump(b),
        (None, Err(_)) => want.get("err").is_some(),
        _ => false,
    }
}

#[test]
fn base_url_oracle() {
    let f = fixture("oracle/common/urls/baseurl.json.gz");
    let protocols: Vec<&str> = f["protocols"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    let ports: Vec<i64> = f["ports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_i64().unwrap())
        .collect();
    let mut t = Tally::new("baseurl");
    for c in f["cases"].as_array().unwrap() {
        let Some(input) = text(&c["in"]) else {
            t.skip(|| format!("{}: input is not UTF-8", c["in"]));
            continue;
        };
        let got = BaseUrl::parse(input);
        let want = &c["new"];
        let non_utf8 = want["ok"]["BasePath"].get("$nh:bytes").is_some();
        if want["ok"]["Port"]
            .as_u64()
            .is_some_and(|p| p > u64::from(u16::MAX))
        {
            t.deviation(|| format!("{input:?}: port beyond 65535 is no port (Go: saturated)"));
            continue;
        }
        if non_utf8 && matches!(got, Err(UrlError::NonUtf8Path(_))) {
            t.deviation(|| format!("{input:?}: decoded base path is not UTF-8 (rejected)"));
            continue;
        }
        t.check(same(want, &got), || {
            format!("{input:?}: want {want}, got {:?}", got.as_ref().map(dump))
        });
        let Ok(b) = got else { continue };
        if let Some(wp) = c.get("withProtocol").and_then(J::as_array) {
            for (p, want) in protocols.iter().zip(wp) {
                let got = b.with_protocol(p);
                t.check(same(want, &got), || {
                    format!(
                        "{input:?}.with_protocol({p:?}): want {want}, got {:?}",
                        got.as_ref().map(dump)
                    )
                });
            }
        }
        if let Some(wp) = c.get("withPort").and_then(J::as_array) {
            for (&p, want) in ports.iter().zip(wp) {
                let Ok(port) = u16::try_from(p) else {
                    t.skip(|| format!("{input:?}.with_port({p}): not a port number"));
                    continue;
                };
                let got = Ok(b.with_port(port));
                t.check(same(want, &got), || {
                    format!(
                        "{input:?}.with_port({p}): want {want}, got {:?}",
                        got.as_ref().map(dump)
                    )
                });
            }
        }
    }
    t.finish();
}

#[test]
fn base_url_examples() {
    let b = BaseUrl::parse("http://example.com").unwrap();
    assert_eq!(b.as_str(), "http://example.com/");
    assert_eq!(b.with_path_no_trailing_slash(), "http://example.com");
    assert_eq!(b.base_path(), "/");
    assert_eq!(
        b.with_protocol("webcal://").unwrap().as_str(),
        "webcal://example.com/"
    );
    assert_eq!(
        b.with_protocol("webcal").unwrap().as_str(),
        "webcal://example.com/"
    );
    assert!(b.with_protocol("mailto:").is_err());
    let m = BaseUrl::parse("mailto:site@rules.com").unwrap();
    assert_eq!(m.as_str(), "mailto:site@rules.com");
    assert_eq!(
        m.with_protocol("webcal").unwrap().as_str(),
        "webcal:site@rules.com"
    );
    assert_eq!(BaseUrl::parse("").unwrap().as_str(), "/");
    let s = BaseUrl::parse("http://example.com/sub").unwrap();
    assert_eq!(s.as_str(), "http://example.com/sub/");
    assert_eq!(s.base_path_no_trailing_slash(), "/sub");
    assert_eq!(s.host_url(), "http://example.com");
    assert_eq!(
        BaseUrl::parse("http://localhost:1313/")
            .unwrap()
            .with_port(8080)
            .as_str(),
        "http://localhost:8080/"
    );
}

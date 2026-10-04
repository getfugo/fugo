use super::*;

#[test]
fn responses_round_trip() {
    let r = Response::parse(
        b"HTTP/2.0 200 OK\r\ncontent-type: text/plain\r\nX-A: 1\r\n\r\nbody\r\n\r\nmore",
    )
    .unwrap();
    assert_eq!(r.code, 200);
    assert_eq!(r.status, "200 OK");
    assert_eq!(r.header("Content-Type"), Some("text/plain"));
    assert_eq!(r.headers[0].0, "Content-Type");
    assert_eq!(r.body, b"body\r\n\r\nmore");
    assert_eq!(Response::parse(&r.to_bytes()).unwrap(), r);
}

#[test]
fn dispositions() {
    assert_eq!(
        disposition_filename("attachment; filename=\"report.json\"").as_deref(),
        Some("report.json")
    );
    assert_eq!(
        disposition_filename("attachment; filename=x; filename*=UTF-8''na%C3%AFve%20file.txt")
            .as_deref(),
        Some("naïve file.txt")
    );
    assert_eq!(disposition_filename("inline"), None);
}

#[test]
fn sniffing() {
    assert_eq!(sniff(b"{\"a\":1}"), "text/plain");
    assert_eq!(sniff(b"  <!doctype html><p>"), "text/html");
    assert_eq!(sniff(b"<?xml version=\"1.0\"?>"), "text/xml");
    assert_eq!(sniff(b"\x89PNG\r\n\x1a\n...."), "image/png");
    assert_eq!(sniff(b"\x00\x01\x02"), "application/octet-stream");
    assert_eq!(sniff(b"\x00\x01\x00\x00\x00\x12\x01\x00"), "font/ttf");
    assert_eq!(sniff(b"wOF2\x00\x01\x00\x00"), "font/woff2");
    assert_eq!(sniff(b"OTTO\x00\x0b"), "font/otf");
    assert_eq!(sniff(b"RIFF\x10\x00\x00\x00WEBPVP8 "), "image/webp");
    assert_eq!(sniff(b"RIFF\x10\x00\x00\x00WAVEfmt "), "audio/wave");
    assert_eq!(
        sniff(b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00mp42isom"),
        "video/mp4"
    );
    assert_eq!(sniff(b"\x00asm\x01\x00\x00\x00"), "application/wasm");
    assert_eq!(sniff(b"\xef\xbb\xbfhello"), "text/plain");
}

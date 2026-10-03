//! Reading template sources without parsing them: the `{% extends "baseof.html" %}` request for
//! the Go implementation's base resolution, and Go-template markers.

use std::ops::Range;

/// The literal a layout extends to request Go's base template resolution.
pub(crate) const BASEOF: &str = "baseof.html";

/// The byte range of the `"baseof.html"` literal (quotes included) when the template begins
/// (after whitespace and comments) with `{% extends "baseof.html" %}`.
pub(crate) fn baseof_extends(src: &str) -> Option<Range<usize>> {
    let mut i = 0;
    loop {
        i += leading_ws(&src[i..]);
        let rest = &src[i..];
        if let Some(body) = rest.strip_prefix("{#") {
            i += 2 + body.find("#}")? + 2;
            continue;
        }
        break;
    }
    let rest = &src[i..];
    let after_open = rest.strip_prefix("{%")?;
    let after_open = after_open.strip_prefix('-').unwrap_or(after_open);
    let ws = leading_ws(after_open);
    let after_kw = after_open[ws..].strip_prefix("extends")?;
    let ws2 = leading_ws(after_kw);
    if ws2 == 0 {
        return None;
    }
    let lit = &after_kw[ws2..];
    let quote = lit
        .chars()
        .next()
        .filter(|c| matches!(c, '"' | '\'' | '`'))?;
    let end = lit[1..].find(quote)? + 2;
    if &lit[1..end - 1] != BASEOF {
        return None;
    }
    let tail = &lit[end..];
    let tail = &tail[leading_ws(tail)..];
    let tail = tail.strip_prefix('-').unwrap_or(tail);
    if !tail.starts_with("%}") {
        return None;
    }
    let start = src.len() - lit.len();
    Some(start..start + end)
}

fn leading_ws(s: &str) -> usize {
    s.len() - s.trim_start().len()
}

/// `src` with its `{% extends "baseof.html" %}` literal replaced by `base`.
pub(crate) fn rewrite_extends(src: &str, base: &str) -> Option<String> {
    let r = baseof_extends(src)?;
    Some(format!("{}\"{base}\"{}", &src[..r.start], &src[r.end..]))
}

/// The first Go-template marker outside `{% raw %}` blocks and comments: `{{ .`, `{{ $`,
/// `{{ end }}`, `{{/*`, or an action keyword (`{{ define`, `{{ range`, `{{ with`, `{{ if`,
/// `{{ else`, `{{ block`, `{{ template`, `{{ partial`), with its 1-based line. Layouts with one
/// are refused ([`crate::IssueKind::GoTemplate`]); so are content adapters (ssg-build).
#[must_use]
pub fn go_marker(src: &str) -> Option<(usize, String)> {
    const KEYWORDS: [&str; 8] = [
        "define", "range", "with", "if", "else", "block", "template", "partial",
    ];
    let mut i = 0;
    while let Some(off) = src[i..].find('{') {
        let at = i + off;
        let rest = &src[at..];
        if let Some(skip) = raw_or_comment(rest) {
            i = at + skip;
            continue;
        }
        i = at + 1;
        let Some(inner) = rest.strip_prefix("{{") else {
            continue;
        };
        if inner.starts_with("/*") {
            return Some((line_of(src, at), "{{/*".to_owned()));
        }
        let inner = inner.strip_prefix('-').unwrap_or(inner);
        let body = inner.trim_start();
        let word: String = body
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        let after = &body[word.len()..];
        let marker = if body.starts_with('.') && !body.starts_with("..") {
            Some(".".to_owned())
        } else if body.starts_with('$') {
            Some("$".to_owned())
        } else if word == "end" && closes(after) {
            Some("end".to_owned())
        } else if KEYWORDS.contains(&word.as_str())
            && after.starts_with(char::is_whitespace)
            && !after.trim_start().starts_with([
                '=', '|', '~', '(', ')', '}', '+', '-', '*', '/', '%', '<', '>', '!', ',',
            ])
        {
            Some(word)
        } else {
            None
        };
        if let Some(m) = marker {
            return Some((line_of(src, at), format!("{{{{ {m}")));
        }
    }
    None
}

/// `-}}` or `}}` after optional whitespace.
fn closes(s: &str) -> bool {
    let s = s.trim_start();
    s.starts_with("}}") || s.starts_with("-}}")
}

/// The length to skip when `s` starts a Tera comment or a `{% raw %}` block.
fn raw_or_comment(s: &str) -> Option<usize> {
    if s.starts_with("{#") {
        return Some(s.find("#}").map_or(s.len(), |e| e + 2));
    }
    let body = s.strip_prefix("{%")?;
    let body = body.strip_prefix('-').unwrap_or(body).trim_start();
    if !body.starts_with("raw")
        || !body[3..]
            .trim_start()
            .trim_start_matches('-')
            .starts_with("%}")
    {
        return None;
    }
    let mut j = 0;
    while let Some(off) = s[j..].find("{%") {
        let at = j + off;
        let tag = s[at + 2..].trim_start_matches('-').trim_start();
        if let Some(t) = tag.strip_prefix("endraw")
            && let Some(close) = t.find("%}")
        {
            return Some(s.len() - t.len() + close + 2);
        }
        j = at + 2;
    }
    Some(s.len())
}

fn line_of(src: &str, at: usize) -> usize {
    src[..at].bytes().filter(|&b| b == b'\n').count() + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extends_baseof_is_found_after_whitespace_and_comments() {
        for src in [
            "{% extends \"baseof.html\" %}x",
            "\n  {# note #}\n{%- extends 'baseof.html' -%}",
            "{%extends `baseof.html`%}",
        ] {
            let r = baseof_extends(src).unwrap_or_else(|| panic!("{src:?}"));
            assert_eq!(&src[r.start + 1..r.end - 1], BASEOF, "{src:?}");
        }
        for src in [
            "x{% extends \"baseof.html\" %}",
            "{% extends \"other.html\" %}",
            "{% extendsbaseof.html %}",
            "{% extends \"baseof.html\" }}",
        ] {
            assert_eq!(baseof_extends(src), None, "{src:?}");
        }
        assert_eq!(
            rewrite_extends(
                "{%- extends 'baseof.html' %}{% block a %}{% endblock %}",
                "docs/baseof.html"
            )
            .unwrap(),
            "{%- extends \"docs/baseof.html\" %}{% block a %}{% endblock %}"
        );
    }

    #[test]
    fn go_markers() {
        assert_eq!(go_marker("a\nb {{ .Title }}"), Some((2, "{{ .".to_owned())));
        assert_eq!(go_marker("{{- end -}}"), Some((1, "{{ end".to_owned())));
        assert_eq!(
            go_marker("{{ with .Params }}"),
            Some((1, "{{ with".to_owned()))
        );
        for tera in [
            "{{ page.title }}",
            "{{ end_date }}",
            "{{ range(end=3) }}",
            "{{ if_x }}",
            "{{ with }}",
            "{# {{ .Title }} #}",
            "{% raw %}{{ .Title }}{% endraw %}{{ x }}",
            "{%- raw -%}{{ define \"x\" }}{%- endraw -%}",
            "{{ 1..3 }}",
        ] {
            assert_eq!(go_marker(tera), None, "{tera:?}");
        }
        assert_eq!(
            go_marker("{% raw %}{{ .x }}{% endraw %}{{ $y }}"),
            Some((1, "{{ $".to_owned()))
        );
    }
}

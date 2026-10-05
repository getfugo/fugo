//! The layouts of the generated sites, in Tera for fugo and in Go templates for the Go
//! implementation: the same pages from the same content (only escaping details differ, such as Go
//! writing `+` as `&#43;` in attributes).

/// The layouts a generated site is written with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Templates {
    /// Tera, for fugo.
    Tera,
    /// Go templates, for the Go implementation.
    Go,
}

impl Templates {
    /// Each layout file (below `layouts/`) and its text.
    #[must_use]
    pub fn layouts(self) -> [(&'static str, &'static str); 3] {
        match self {
            Self::Tera => [
                ("baseof.html", TERA_BASEOF),
                ("single.html", TERA_SINGLE),
                ("list.html", TERA_LIST),
            ],
            Self::Go => [
                ("baseof.html", GO_BASEOF),
                ("single.html", GO_SINGLE),
                ("list.html", GO_LIST),
            ],
        }
    }
}

const TERA_BASEOF: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{{ page.title }} | {{ site.title }}</title>
<link rel="canonical" href="{{ page.permalink }}">
{%- for f in page.alternative_output_formats %}
<link rel="{{ f.rel }}" type="{{ f.media_type.type }}" href="{{ f.permalink }}">
{%- endfor %}
</head>
<body>
<header>
<a href="{{ site.home.rel_permalink }}">{{ site.title }}</a>
<nav>{% for e in site.menus.main or [] %}<a href="{{ e.url }}">{{ e.name }}</a> {% endfor %}</nav>
</header>
<main>{% block main %}{% endblock main %}</main>
</body>
</html>
"#;

const TERA_SINGLE: &str = r#"{% extends "baseof.html" %}
{% block main %}
<article>
<h1>{{ page.title }}</h1>
<p><time datetime="{{ page.date | date(format="%Y-%m-%d") }}">{{ page.date | date(format="%e %B %Y") }}</time>, {{ page.reading_time }} min, {{ page.word_count }} words</p>
<nav>{{ page.table_of_contents }}</nav>
{{ page.content }}
{%- if page.prev_in_section %}
<p><a href="{{ page.prev_in_section.rel_permalink }}">{{ page.prev_in_section.title }}</a></p>
{%- endif %}
</article>
{% endblock main %}
"#;

const TERA_LIST: &str = r#"{% extends "baseof.html" %}
{% block main %}
<h1>{{ page.title }}</h1>
{{ page.content }}
{%- set pager = paginator() %}
<ul>
{%- for p in pager.pages %}
<li><a href="{{ p.rel_permalink }}">{{ p.title }}</a><p>{{ p.summary }}</p></li>
{%- endfor %}
</ul>
<nav>{% if pager.has_prev %}<a href="{{ pager.prev.url }}">Newer</a>{% endif %} {{ pager.page_number }} of {{ pager.total_pages }} {% if pager.has_next %}<a href="{{ pager.next.url }}">Older</a>{% endif %}</nav>
{% endblock main %}
"#;

const GO_BASEOF: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{{ .Title }} | {{ site.Title }}</title>
<link rel="canonical" href="{{ .Permalink }}">
{{- range .AlternativeOutputFormats }}
<link rel="{{ .Rel }}" type="{{ .MediaType.Type }}" href="{{ .Permalink }}">
{{- end }}
</head>
<body>
<header>
<a href="{{ site.Home.RelPermalink }}">{{ site.Title }}</a>
<nav>{{ range site.Menus.main }}<a href="{{ .URL }}">{{ .Name }}</a> {{ end }}</nav>
</header>
<main>{{ block "main" . }}{{ end }}</main>
</body>
</html>
"#;

const GO_SINGLE: &str = r#"{{ define "main" }}
<article>
<h1>{{ .Title }}</h1>
<p><time datetime="{{ .Date.Format "2006-01-02" }}">{{ .Date.Format "_2 January 2006" }}</time>, {{ .ReadingTime }} min, {{ .WordCount }} words</p>
<nav>{{ .TableOfContents }}</nav>
{{ .Content }}
{{- with .PrevInSection }}
<p><a href="{{ .RelPermalink }}">{{ .Title }}</a></p>
{{- end }}
</article>
{{ end }}
"#;

const GO_LIST: &str = r#"{{ define "main" }}
<h1>{{ .Title }}</h1>
{{ .Content }}
{{- $pager := .Paginator }}
<ul>
{{- range $pager.Pages }}
<li><a href="{{ .RelPermalink }}">{{ .Title }}</a><p>{{ .Summary }}</p></li>
{{- end }}
</ul>
<nav>{{ if $pager.HasPrev }}<a href="{{ $pager.Prev.URL }}">Newer</a>{{ end }} {{ $pager.PageNumber }} of {{ $pager.TotalPages }} {{ if $pager.HasNext }}<a href="{{ $pager.Next.URL }}">Older</a>{{ end }}</nav>
{{ end }}
"#;

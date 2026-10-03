---
title: urls.Encode
description: Returns the given string encoded so it can be safely placed inside a URL query.
categories: []
keywords: []
action:
  aliases: [urlencode]
  related:
    - functions/urls/Decode
  returnType: string
  signatures: [urls.Encode INPUT]
aliases: [/functions/urlencode]
---

The `urls.Encode` function encodes a string so it can be safely placed inside a URL query.

## Example

Consider this front matter:

{{< code-toggle file=content/blog/greatest-city.md fm=true >}}
title = "The World's Greatest City"
location = "Chicago IL"
tags = ["pizza","beer","hot dogs"]
{{< /code-toggle >}}

Use the `urlencode` function to encode the location and tags for use in URLs:

```go-html-template
<header>
    <h1>{{ .Title }}</h1>
    {{ with .Params.location }}
        <div><a href="/locations/{{ . | urlencode }}">{{ . }}</a></div>
    {{ end }}
    {{ with .Params.tags }}
    <ul>
        {{ range . }}
            <li>
                <a href="/tags/{{ . | urlencode }}">{{ . }}</a>
            </li>
        {{ end }}
    </ul>
    {{ end }}
</header>
```

Hugo renders this to:

```html
<header>
    <h1>The World&#39;s Greatest City</h1>
    <div><a href="/locations/Chicago%20IL">Chicago IL</a></div>
    <ul>
        <li>
            <a href="/tags/pizza">pizza</a>
        </li>
        <li>
            <a href="/tags/beer">beer</a>
        </li>
        <li>
            <a href="/tags/hot%20dogs">hot dogs</a>
        </li>
    </ul>
</header>
```

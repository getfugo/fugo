---
title: urls.Decode
description: Returns the given URL-encoded string, decoded for display.
categories: []
keywords: []
action:
  aliases: [urldecode]
  related:
    - functions/urls/Encode
  returnType: string
  signatures: [urls.Decode INPUT]
aliases: [/functions/urldecode]
---

The `urls.Decode` function decodes a URL-encoded string back to its original form.

## Example

Consider this front matter:

{{< code-toggle file=content/blog/neohugo.md fm=true >}}
title = "A Forked of Hugo"
location = "กรุงเทพ ไทย"
{{< /code-toggle >}}

Use the `urldecode` function to decode a URL-encoded string:

```go-html-template
<header>
    <h1>{{ .Title }}</h1>
    {{ with .Params.location }}
        <div><a href="/locations/{{ urldecode ( . | urlize) }}">{{ . }}</a></div>
    {{ end }}
</header>
```

Hugo renders this to:

```html
<header>
    <h1>A Forked of Hugo</h1>
    <div><a href="/locations/กรุงเทพ-ไทย">กรุงเทพ ไทย</a></div>
</header>
```

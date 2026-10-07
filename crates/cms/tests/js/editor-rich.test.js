// The rich text editor of a page's text (assets/admin/cms.js; editor/markdown.ts,
// tomarkdown.ts, richtext.ts), in happy-dom against a fake API (ui.js): Markdown drawn as
// elements and written back, block by block, and the text edited in it.

import { before, test } from "node:test";
import assert from "node:assert/strict";

import { $, $$, document, go, sent, skip, startEditor, STYLE, text, until, window } from "./ui.js";

const SITE = {
  title: "Snacks",
  site_url: "https://example.org/",
  workflow: "review",
  languages: [{ key: "en", name: "English" }],
  default_language: "en",
  taxonomies: [],
  fields: {},
  content_dir: "content",
  media: null,
  media_ref: null,
  upload_types: ["jpg"],
  max_upload: 1048576,
  sections: [{ key: "posts", title: "Posts", count: 1, style: STYLE, keys: [{ key: "title", kind: "string" }] }],
  entries: [
    { key: "posts/crisps", section: "posts", kind: "page", bundle: false, title: "Crisps", files: [{ lang: "en", path: "content/posts/crisps.md", url: "/posts/crisps/" }], resources: [] },
  ],
};
const FILES = { "content/posts/crisps.md": "---\ntitle: Crisps\n---\n\nThey *crunch*.\n\n<!-- more -->\n\nSalt, then [more](/salt/). ![Salt](salt.jpg)\n" };

let cms;
before(async () => {
  if (skip) return;
  await startEditor(SITE, FILES);
  cms = await import("../../assets/admin/cms.js");
});

const url = (src) => new URL(src, "https://example.org/posts/crisps/").href;

/** `markdown` drawn into a new box; `write` gives its Markdown as it is then. */
function draw(markdown) {
  const box = document.createElement("div");
  document.body.append(box);
  const drawn = cms.drawMarkdown(box, markdown, url, true);
  return { box, drawn, write: () => cms.writeMarkdown(box, drawn) };
}

/** A new block at the end of `box`, from its HTML. */
function append(box, html) {
  const t = document.createElement("template");
  t.innerHTML = html;
  box.append(...t.content.childNodes);
}

const SAMPLE = `
Intro, with _emphasis_, __strong__, \`code\`, a [reference][1] and a footnote[^1].
A second line, \\*not emphasis\\*.

{{< figure src="cover.jpg" >}}

# Heading {#anchor}


- one
- two
  - nested

1. first
2. second

> quoted
> text

~~~go
fmt.Println("hi")
~~~

<div class="note">Raw HTML</div>

| left | right |
|:-----|------:|
| a    | b     |

[1]: https://example.org "Example"
[^1]: The footnote.

***
Last <kbd>Ctrl</kbd> www.example.com
after a break ![a photo](photo.jpg "Photo")

- [ ] to do
- [x] done`;

test("a text nobody edits is written as it was, byte for byte", { skip }, () => {
  for (const markdown of [SAMPLE, `${SAMPLE}\n`, SAMPLE.replace(/\n/g, "\r\n"), "", "\n", "One line", "\tIndented code\n"]) {
    assert.equal(draw(markdown).write(), markdown);
  }
});

test("an edited block is written anew; the others, and the lines between them, as they were", { skip }, () => {
  const { box, write } = draw(SAMPLE);
  const first = box.querySelector("p");
  first.lastChild.data = first.lastChild.data.replace("A second line", "A changed line");
  const lines = write().split("\n");
  // Its marks as written (`_`, `__`, the reference), and its escapes where they are needed.
  assert.deepEqual(lines.slice(0, 4), [
    "",
    "Intro, with _emphasis_, __strong__, `code`, a [reference][1] and a footnote[^1].",
    "A changed line, \\*not emphasis\\*.",
    "",
  ]);
  assert.equal(lines.slice(3).join("\n"), SAMPLE.split("\n").slice(3).join("\n"));
});

test("a block split in two by editing: the part as drawn stays as written", { skip }, () => {
  const { box, write } = draw("First.\n\nSecond.\n");
  const second = box.querySelectorAll("p")[1];
  const copy = second.cloneNode(false);
  copy.textContent = "Inserted.";
  second.before(copy);
  assert.equal(write(), "First.\n\nInserted.\n\nSecond.\n");
});

test("text gets a backslash only where it would otherwise start Markdown", { skip }, () => {
  const { box, write } = draw("");
  box.querySelector("p").textContent =
    '1. a *b* _c_ snake_case 2 * 3 [d](e) [f] [^2] `g` <h> a < b & &amp; \\* ~~x~~ {{< ref "a_b" >}} [x]({{< ref "y" >}})';
  append(box, "<p>- not a list</p><p># not a heading</p><p>&gt; not a quote</p><p>---</p>");
  assert.equal(
    write(),
    [
      '1\\. a \\*b\\* \\_c\\_ snake_case 2 * 3 \\[d](e) [f] [^2] \\`g\\` \\<h> a < b & &amp;amp; \\\\\\* \\~\\~x\\~\\~ {{< ref "a_b" >}} [x]({{< ref "y" >}})',
      "\\- not a list",
      "\\# not a heading",
      "\\> not a quote",
      "\\---",
    ].join("\n\n") + "\n",
  );
});

test("new blocks: headings, lists, quotes, code, tables, links and images", { skip }, () => {
  const { box, write } = draw("");
  box.replaceChildren();
  append(
    box,
    [
      "<h2>A <b>bold</b> heading #</h2>",
      "<ul><li>one <i>it</i></li><li>two<ul><li>nested</li></ul></li></ul>",
      '<ol start="3"><li>three</li><li>four</li></ol>',
      "<blockquote><p>said</p><p>twice</p></blockquote>",
      "<pre>let a = 1;<br>```<br>done</pre>",
      '<table><tr><th>a|b</th><th style="text-align: right">c</th></tr><tr><td>1</td><td><a href="https://example.org/a b">l[i]nk</a></td></tr></table>',
      '<p>See <a href="https://example.org" title="An &quot;example&quot;">this</a>, <a href="https://example.org">https://example.org</a>, ' +
        '<img src="x" data-md-src="photo one.jpg" alt="A [photo]"> and <s>gone</s> <code>a`b</code>.<br>Next line.</p>',
      "<hr>",
    ].join(""),
  );
  assert.equal(
    write(),
    [
      "## A **bold** heading \\#",
      "- one *it*\n- two\n  - nested",
      "3. three\n4. four",
      "> said\n>\n> twice",
      "````\nlet a = 1;\n```\ndone\n````",
      "| a\\|b | c |\n| --- | --: |\n| 1 | [l\\[i\\]nk](<https://example.org/a b>) |",
      'See [this](https://example.org "An \\"example\\""), <https://example.org>, ![A \\[photo\\]](<photo one.jpg>) and ~~gone~~ ``a`b``.\\\nNext line.',
      "---",
    ].join("\n\n") + "\n",
  );
});

test("lists as the browser's editing makes them lose no text", { skip }, () => {
  const { box, write } = draw("");
  box.replaceChildren();
  // Indenting an item: a list right inside the list. A new list: inside the paragraph it was.
  // Text that left a quote: between the items.
  append(box, "<ul><li>one</li><ul><li>two</li></ul><li>three</li></ul>");
  const p = document.createElement("p");
  p.append(document.createElement("ol"));
  p.firstChild.innerHTML = "<li>first</li>";
  box.append(p);
  append(box, "<ul>Loose text <b>here</b><li>item</li></ul>");
  append(box, '<ul><li><input type="checkbox" checked=""> done</li><li><input type="checkbox"></li></ul>');
  assert.equal(write(), "- one\n  - two\n- three\n\n1. first\n\n- Loose text **here**\n- item\n\n- [x] done\n- [ ]\n");
});

test("HTML and shortcodes show as their source, never as elements, and are edited as text", { skip }, () => {
  const markdown = '<script>alert(1)</script>\n\n<img src=x onerror="alert(1)">\n\n{{< youtube abc >}}\n\nText <span onclick="alert(1)">here</span>.\n';
  const { box, write } = draw(markdown);
  assert.equal(box.querySelector("script, img, [onclick], [onerror]"), null);
  const sources = [...box.querySelectorAll(".md-raw")];
  assert.deepEqual(sources.map((s) => s.getAttribute("data-md-raw")), ["HTML", "HTML", "Shortcode"]);
  assert.deepEqual(sources.map((s) => s.textContent), ["<script>alert(1)</script>", '<img src=x onerror="alert(1)">', "{{< youtube abc >}}"]);
  assert.deepEqual([...box.querySelectorAll(".md-tag")].map((t) => t.textContent), ['<span onclick="alert(1)">', "</span>"]);
  assert.equal(sources[2].querySelector("[data-md-source]").getAttribute("contenteditable"), "plaintext-only");
  sources[2].querySelector("[data-md-source]").textContent = "{{< youtube xyz >}}";
  assert.equal(write(), markdown.replace("abc", "xyz"));
  // Read only: nothing is editable, the source neither.
  const shown = document.createElement("div");
  cms.drawMarkdown(shown, markdown, url, false);
  assert.ok([...shown.querySelectorAll("[data-md-source]")].every((s) => s.getAttribute("contenteditable") === "false"));
});

test("images show from the page's address, and keep their name in the text", { skip }, () => {
  const { box, write } = draw("![Crisps](crisps.jpg)\n");
  const img = box.querySelector("img");
  assert.equal(img.getAttribute("src"), "https://example.org/posts/crisps/crisps.jpg");
  img.setAttribute("alt", "A bowl of crisps");
  assert.equal(write(), "![A bowl of crisps](crisps.jpg)\n");
});

test("pasted HTML becomes what Markdown can hold of it", { skip }, () => {
  const { drawn } = draw("");
  const pasted =
    '<meta charset="utf-8"><b style="font-weight:normal;" id="docs-internal-guid-1"><p dir="ltr"><span style="font-weight:700">Bold</span>' +
    '<span> and </span><span style="font-style:italic">italic</span></p><ul><li><p>one</p></li><li><p>two</p></li></ul>' +
    '<script>alert(1)</script><p><img src="https://elsewhere.example/x.png" onerror="alert(1)" alt="x"></p></b>';
  const box = document.createElement("div");
  box.innerHTML = cms.pastedHtml(pasted, url, drawn.cx);
  assert.equal(box.querySelector("script, [onerror], [data-md-b], [data-md-i]"), null);
  assert.equal(
    box.innerHTML.replace(/\n/g, ""),
    '<p><strong>Bold</strong> and <em>italic</em></p><ul><li>one</li><li>two</li></ul><p><img src="https://elsewhere.example/x.png" data-md-src="https://elsewhere.example/x.png" alt="x"></p>',
  );
});

test("a page's text opens as rich text, with a toolbar", { skip }, async () => {
  await go("#/e/posts%2Fcrisps", () => $(".rich-box p"));
  assert.deepEqual($$(".modes button").map((b) => [text(b), b.getAttribute("aria-pressed")]), [
    ["Rich text", "true"],
    ["Markdown", "false"],
  ]);
  assert.equal($(".rich-box").getAttribute("contenteditable"), "true");
  assert.deepEqual($$(".rich-box > *").map((el) => el.tagName), ["P", "DIV", "P"]);
  assert.equal(text($(".rich-box .md-raw")), "<!-- more -->");
  assert.deepEqual($$(".toolbar button").map(text), ["B", "I", "S", "</>", "Link", "• List", "1. List", "Image", "―"]);
  assert.deepEqual($$(".toolbar select option").map(text), ["Paragraph", "Heading 1", "Heading 2", "Heading 3", "Heading 4", "Quote", "Code block"]);
});

test("Markdown shows the text as typed in rich text, and rich text draws what is typed there", { skip }, () => {
  const p = $(".rich-box p");
  p.lastChild.data = ", loudly.";
  p.dispatchEvent(new window.Event("input", { bubbles: true }));
  $$(".modes button").find((b) => text(b) === "Markdown").click();
  assert.equal($(".rich-box"), null);
  const area = $("textarea.body");
  assert.equal(area.value, "\nThey *crunch*, loudly.\n\n<!-- more -->\n\nSalt, then [more](/salt/). ![Salt](salt.jpg)\n");
  area.value = "## Salt\n\nThey crunch.\n";
  area.dispatchEvent(new window.Event("input", { bubbles: true }));
  $$(".modes button").find((b) => text(b) === "Rich text").click();
  assert.equal(text($(".rich-box h2")), "Salt");
  assert.equal(window.localStorage.getItem("cms-text-mode"), "rich");
});

test("the image button opens a form with the page's files", { skip }, () => {
  const button = $$(".toolbar button").find((b) => text(b) === "Image");
  button.click();
  assert.ok($(".image-form input[name=src]"));
  assert.equal(button.getAttribute("aria-expanded"), "true");
  $$(".image-form button").find((b) => text(b) === "Cancel").click();
  assert.equal($(".image-form"), null);
});

test("what is typed in rich text is saved", { skip }, async () => {
  const p = $(".rich-box p");
  p.textContent = "They crunch, and crackle.";
  p.dispatchEvent(new window.Event("input", { bubbles: true }));
  sent.length = 0;
  $(".actions .primary").click();
  await until(() => sent.length);
  assert.equal(sent[0].body.changes[0].content, "---\ntitle: Crisps\n---\n## Salt\n\nThey crunch, and crackle.\n");
  // The page opens again, its images from its own address still.
  const img = await until(() => $(".rich-box img"));
  assert.equal(img.getAttribute("src"), "https://example.org/posts/crisps/salt.jpg");
});

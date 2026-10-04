// common.ts (in the built Worker, assets/worker.js): globs (the cases the Rust globs are tested on too), paths, areas, trailers, base64.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  areaOf,
  base64Size,
  base64ToText,
  bytesToBase64,
  cleanPath,
  commitMessage,
  compileGlob,
  extensionOf,
  matchGlob,
  mayEdit,
  parseTrailers,
  textToBase64,
} from "../../assets/worker.js";

test("globs match the shared cases like ssg_base::glob", () => {
  const cases = JSON.parse(readFileSync(new URL("../glob-cases.json", import.meta.url), "utf8"));
  for (const [pattern, path, want] of cases) {
    assert.equal(matchGlob(pattern, path), want, `${pattern} on ${path}`);
  }
});

test("invalid globs throw like the Rust compiler rejects them", () => {
  for (const bad of ["a[", "a[!", "a[]", "a[z-a]", "a[a-b"]) {
    assert.throws(() => compileGlob(bad), /glob/, bad);
  }
  for (const ok of ["a[b]", "{", "}", ",", "\\"]) compileGlob(ok);
});

test("clean paths", () => {
  for (const ok of ["content/a.md", "content/ไทย/หน้า.md", "a"]) assert.equal(cleanPath(ok), ok);
  assert.equal(cleanPath("content/\ud800.md"), null, "a lone surrogate");
  for (const bad of ["", "/a", "a//b", "a/./b", "a/../b", ".env", "a/.git/x", "a\\b", "a\nb", null, 3, "x".repeat(1025)]) {
    assert.equal(cleanPath(bad), null, String(bad));
  }
  assert.equal(extensionOf("a/b.JPG"), "jpg");
  assert.equal(extensionOf("a/.b"), "");
  assert.equal(extensionOf("a.b/c"), "");
});

test("areas and edit globs", () => {
  const settings = {
    areas: [
      { kind: "content", glob: "content/**", ext: ["md", "jpg"] },
      { kind: "config", glob: "config/_default/{params,menus,menu}.*", ext: ["toml"] },
    ],
    deny: ["**/_content.*"],
  };
  assert.equal(areaOf(settings, "content/a.md").kind, "content");
  assert.equal(areaOf(settings, "config/_default/params.en.toml").kind, "config");
  for (const p of ["content/a.svg", "content/_content.md", "content/_Content.md", "content/x/_CONTENT.th.MD", "layouts/a.md", "config/_default/config.toml", "config/production/params.toml"]) {
    assert.equal(areaOf(settings, p), null, p);
  }
  assert.equal(mayEdit(settings, ["**"], "content/a.md"), true);
  assert.equal(mayEdit(settings, ["**"], "layouts/a.md"), false);
  assert.equal(mayEdit(settings, ["content/blog/**"], "content/a.md"), false);
  assert.equal(mayEdit(settings, [], "content/a.md"), false);
});

test("commit trailers", () => {
  const m = commitMessage("Edit  a\nb", [
    ["CMS-Entry", "blog/post"],
    ["CMS-Title", "  A  title "],
    ["Empty", ""],
    ["Missing", undefined],
  ]);
  assert.equal(m, "Edit a b\n\nCMS-Entry: blog/post\nCMS-Title: A title\n");
  assert.deepEqual(parseTrailers(m), { "cms-entry": "blog/post", "cms-title": "A title" });
  assert.deepEqual(parseTrailers("just a subject"), {});
  assert.deepEqual(parseTrailers(undefined), {});
});

test("base64", () => {
  const text = "ไทย and English ✓";
  assert.equal(base64ToText(textToBase64(text)), text);
  const bytes = new Uint8Array(100_000).map((_, i) => i % 256);
  const b64 = bytesToBase64(bytes);
  assert.equal(b64, Buffer.from(bytes).toString("base64"));
  assert.equal(base64Size(b64), 100_000);
  assert.equal(base64Size("YQ=="), 1);
  assert.equal(base64Size("YWI="), 2);
  assert.equal(base64Size("YWJj"), 3);
});

// codec.ts (in the built editor, assets/admin/cms.js): splitting content files, decoding their front matter, and writing them back.

import { test } from "node:test";
import assert from "node:assert/strict";

import { split, join, decode, encode, create, clone, equal, isTomlDate, tomlDate } from "../../assets/admin/cms.js";

const YAML_PAGE = `---
title: Honey Butter Almond
# a comment the editor keeps
countries:
  - South Korea
date: 2020-10-25T12:00:07.092Z
brands: Tom
percentage: "73.32"
nutrition:
  fat:
    total: 8
---
Body **text**.
`;

test("split and join give the file back", () => {
  for (const text of [
    YAML_PAGE,
    "+++\ntitle = \"x\"\n+++\nbody\n",
    "{\n  \"title\": \"x\"\n}\nbody\n",
    "no front matter\n",
    "﻿\n\n---\ntitle: bom\n---\n",
    "---\r\ntitle: crlf\r\n---\r\nbody\r\n",
    "---\n---\nempty front matter\n",
    "---\ntitle: unclosed\n",
  ]) {
    assert.equal(join(split(text)), text, JSON.stringify(text));
  }
  assert.equal(split(YAML_PAGE).format, "yaml");
  assert.equal(split("+++\na = 1\n+++\n").format, "toml");
  assert.equal(split('{"a": "}"}\nx').format, "json");
  assert.equal(split('{"a": "}"}\nx').body, "\nx");
  assert.equal(split("---\ntitle: unclosed\n").format, "none");
});

test("YAML: strings stay strings, and an edit keeps everything else", () => {
  const parts = split(YAML_PAGE);
  const original = decode(parts);
  assert.equal(original.date, "2020-10-25T12:00:07.092Z");
  assert.equal(original.percentage, "73.32");
  assert.deepEqual(original.nutrition, { fat: { total: 8 } });
  assert.equal(join(encode(parts, original, clone(original))), YAML_PAGE, "unchanged");

  const data = clone(original);
  data.title = "New: title";
  data.countries.push("Thailand");
  delete data.brands;
  data.added = true;
  const out = join(encode(parts, original, data));
  assert.equal(
    out,
    `---
title: "New: title"
# a comment the editor keeps
countries:
  - South Korea
  - Thailand
date: 2020-10-25T12:00:07.092Z
percentage: "73.32"
nutrition:
  fat:
    total: 8
added: true
---
Body **text**.
`,
  );
  assert.deepEqual(decode(split(out)), data);
});

test("YAML: keys that did not change keep their text, folded lines included", () => {
  const text = `---
title: Old
smell: When open the package, will get smell the butter with honey but
  butter smell is more intense.
list: [a, b]   # flow style and a comment
nested:
  deep:  1
---
`;
  const parts = split(text);
  const original = decode(parts);
  const data = clone(original);
  data.title = "New";
  data.nested.deep = 2;
  const out = join(encode(parts, original, data));
  assert.equal(
    out,
    `---
title: New
smell: When open the package, will get smell the butter with honey but
  butter smell is more intense.
list: [a, b]   # flow style and a comment
nested:
  deep: 2
---
`,
  );
  // A flow-style map is written by the library as a whole.
  const flow = split("---\n{title: a, n: 1}\n---\n");
  const fo = decode(flow);
  assert.deepEqual(decode(split(join(encode(flow, fo, { ...fo, n: 2 })))), { title: "a", n: 2 });
});

test("TOML keeps its dates", () => {
  const text = '+++\ntitle = "x"\ndate = 2024-01-02T03:04:05Z\nday = 2024-01-02\ntags = ["a"]\n+++\nbody\n';
  const parts = split(text);
  const original = decode(parts);
  assert.ok(isTomlDate(original.date));
  assert.equal(original.day.toISOString(), "2024-01-02");
  const data = clone(original);
  data.title = "y";
  data.date = tomlDate("2025-02-03T04:05:06Z");
  const out = join(encode(parts, original, data));
  const back = decode(split(out));
  assert.equal(back.title, "y");
  assert.equal(back.date.toISOString(), "2025-02-03T04:05:06.000Z");
  assert.equal(back.day.toISOString(), "2024-01-02");
  assert.ok(out.endsWith("+++\nbody\n"));
  assert.equal(tomlDate("not a date"), null);
});

test("JSON front matter", () => {
  const text = '{\n  "title": "x",\n  "n": 1\n}\nbody\n';
  const parts = split(text);
  const original = decode(parts);
  const data = { ...original, n: 2 };
  assert.equal(join(encode(parts, original, data)), '{\n  "title": "x",\n  "n": 2\n}\nbody\n');
});

test("a file without front matter gets YAML when keys are added", () => {
  const parts = split("Just text.\n");
  const original = decode(parts);
  assert.deepEqual(original, {});
  assert.equal(join(encode(parts, original, {})), "Just text.\n");
  assert.equal(join(encode(parts, original, { title: "T" })), "---\ntitle: T\n---\nJust text.\n");
});

test("invalid front matter throws", () => {
  assert.throws(() => decode(split("---\ntitle: [unclosed\n---\n")));
  assert.throws(() => decode(split("---\n- a list\n---\n")), /not a map/);
  assert.throws(() => decode(split("+++\n= bad\n+++\n")));
});

test("new files", () => {
  assert.equal(join(create("yaml", { title: "T", tags: [] }, "Hi\n")), "---\ntitle: T\ntags: []\n---\nHi\n");
  assert.equal(join(create("toml", { title: "T" }, "")), '+++\ntitle = "T"\n+++\n');
  assert.equal(join(create("json", { title: "T" }, "Hi")), '{\n  "title": "T"\n}\nHi');
});

test("equal and clone", () => {
  const a = { x: [1, { y: "z" }], d: tomlDate("2024-01-02") };
  const b = clone(a);
  assert.ok(equal(a, b));
  b.x[1].y = "w";
  assert.ok(!equal(a, b));
  assert.equal(a.x[1].y, "z");
  assert.ok(!equal({ a: 1, b: 2 }, { b: 2, a: 1 }), "key order counts");
  assert.ok(!equal([1], [1, 2]));
  assert.ok(!equal(null, {}));
});

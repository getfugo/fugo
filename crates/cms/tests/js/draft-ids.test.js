// The draft ids of `tests/draft-id-cases.json`, which the local API (`src/local/rules.rs`) is
// tested on too: a page's draft is the same branch whichever of them made it.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { draftId } from "../../assets/worker.js";

test("draft ids match the shared cases", async () => {
  const cases = JSON.parse(readFileSync(new URL("../draft-id-cases.json", import.meta.url), "utf8"));
  assert.ok(cases.length >= 10);
  for (const [entry, id] of cases) assert.equal(await draftId(entry), id, entry);
});

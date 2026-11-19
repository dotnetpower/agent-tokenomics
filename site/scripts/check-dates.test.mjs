import { test } from "node:test";
import assert from "node:assert/strict";
import { dateProblems } from "./check-dates.mjs";

const doc = (fields) => `---\n${fields}\n---\nBody`;
test("requires publication, modification, and provenance fields", () => {
  assert.equal(dateProblems(doc("title: Example"), "2026-11-19").length, 3);
});
test("rejects impossible, future, and reversed dates", () => {
  assert.ok(dateProblems(doc("created: 2026-02-30\nupdated: 2026-11-20\ndateSource: git"), "2026-11-19").length === 2);
  assert.ok(dateProblems(doc("created: 2026-11-19\nupdated: 2026-11-03\ndateSource: git"), "2026-11-19")
    .some((error) => error.includes("precede")));
});
test("verification is independent of modification and may predate first recording", () => {
  assert.deepEqual(dateProblems(doc('created: "2026-11-19"\nupdated: 2026-11-19\nverified: 2026-11-03\ndateSource: recorded'), "2026-11-19"), []);
  assert.ok(dateProblems(doc("created: 2026-11-03\nupdated: 2026-11-19\nverified: 2026-11-20\ndateSource: git"), "2026-11-19")
    .some((error) => error.startsWith("verified")));
  assert.ok(dateProblems(doc("created: 2026-11-03\nupdated: 2026-11-19\nverified:\ndateSource: git"), "2026-11-19")
    .some((error) => error.startsWith("verified")));
});

const test = require("node:test");
const assert = require("node:assert/strict");

const {
  verifyArtifacts,
  verifyNarrative,
  verifyPrivateBoundary,
} = require("../../scripts/verify-repository");

test("repository exposes the exact production artifact set", () => {
  const result = verifyArtifacts();
  assert.equal(result.docs, 7);
  assert.equal(result.diagrams >= 14, true);
  assert.equal(result.bannerBytes >= 100_000, true);
});

test("public narrative stays inside the product boundary", () => {
  assert.equal(verifyNarrative().files > 60, true);
});

test("private material is absent from tracked files", () => {
  assert.deepEqual(verifyPrivateBoundary(), { trackedPrivateFiles: 0 });
});

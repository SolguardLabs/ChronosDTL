const test = require("node:test");
const assert = require("node:assert/strict");

const { readProjectFile } = require("../helpers/chronosCargo");

test("Cargo.toml declares the expected crate", () => {
  const cargo = readProjectFile("Cargo.toml");
  assert.match(cargo, /name = "chronos_dtl"/);
  assert.match(cargo, /edition = "2024"/);
  assert.match(cargo, /publish = false/);
});

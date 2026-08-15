const test = require("node:test");
const assert = require("node:assert/strict");

const { run } = require("../helpers/chronosCargo");

test("cargo metadata resolves with the lockfile", () => {
  const metadata = JSON.parse(run("cargo", ["metadata", "--locked", "--format-version", "1"]));
  assert.equal(
    metadata.packages.some((pkg) => pkg.name === "chronos_dtl"),
    true,
  );
});

test("cargo check compiles the library", () => {
  run("cargo", ["check", "--locked"], { stdio: "inherit" });
});

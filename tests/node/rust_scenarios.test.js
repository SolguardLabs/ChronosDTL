const test = require("node:test");

const { run } = require("../helpers/chronosCargo");

test("Rust lifecycle scenarios pass", () => {
  run("cargo", ["test", "--locked", "--test", "chronos_flow"], { stdio: "inherit" });
});

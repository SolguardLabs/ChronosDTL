const test = require("node:test");

const { run } = require("../helpers/chronosCargo");

test("los escenarios Rust de ciclo de vida pasan", () => {
  run("cargo", ["test", "--locked", "--test", "chronos_flow"], { stdio: "inherit" });
});

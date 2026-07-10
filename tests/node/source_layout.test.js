const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");

const { projectPath, readProjectFile, srcLineCount } = require("../helpers/chronosCargo");

test("usa lib.rs como crate de libreria", () => {
  assert.equal(fs.existsSync(projectPath("src", "lib.rs")), true);
  assert.equal(fs.existsSync(projectPath("src", "main.rs")), false);
});

test("src queda en el rango de tamano esperado", () => {
  const lines = srcLineCount();
  assert.ok(lines >= 5000, `src tiene ${lines} lineas`);
  assert.ok(lines <= 6000, `src tiene ${lines} lineas`);
});

test("la estructura separa dominios temporales y financieros", () => {
  for (const directory of [
    "accounts",
    "amount",
    "debt",
    "ledger",
    "locks",
    "rates",
    "settlement",
    "time",
    "expiry",
    "analytics",
  ]) {
    assert.equal(fs.existsSync(projectPath("src", directory, "mod.rs")), true);
  }
});

test("lib.rs exporta la API principal", () => {
  const lib = readProjectFile("src", "lib.rs");
  for (const exportName of [
    "ChronosLedger",
    "DebtCalculator",
    "LockRequest",
    "OpenPositionRequest",
    "SettlementReceipt",
    "ExpiryReceipt",
    "RateModel",
    "PortfolioReport",
  ]) {
    assert.equal(lib.includes(exportName), true, exportName);
  }
});

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");

const { projectPath, readProjectFile, srcLineCount } = require("../helpers/chronosCargo");

test("the crate uses lib.rs as its library entry point", () => {
  assert.equal(fs.existsSync(projectPath("src", "lib.rs")), true);
  assert.equal(fs.existsSync(projectPath("src", "main.rs")), false);
});

test("source size stays within the production range", () => {
  const lines = srcLineCount();
  assert.ok(lines >= 6400, `src has ${lines} lines`);
  assert.ok(lines <= 7600, `src has ${lines} lines`);
});

test("source layout separates temporal and financial domains", () => {
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
    "capital",
    "governance",
  ]) {
    assert.equal(fs.existsSync(projectPath("src", directory, "mod.rs")), true);
  }
});

test("lib.rs exports the primary API", () => {
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
    "TemporalStressEngine",
    "GovernanceRegistry",
  ]) {
    assert.equal(lib.includes(exportName), true, exportName);
  }
});

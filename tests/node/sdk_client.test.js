const test = require("node:test");
const assert = require("node:assert/strict");

const {
  ChronosClient,
  atomic,
  computeTemporalStress,
  jsonBody,
} = require("../../sdk/chronosClient");

const policy = {
  collateralHaircutBps: 1_000,
  liquidationCostBps: 500,
  rateShockBpsPerEpoch: 100,
  concentrationAddonBps: 1_000,
  targetCoverageBps: 12_000,
  horizonEpochs: 3,
  operationalBuffer: 10_000_000n,
};

const positions = [
  {
    id: "pos-1",
    borrower: "acct-1",
    pool: "pool-1",
    principal: 100_000_000n,
    quotedInterest: 5_000_000n,
    quotedPenalty: 1_000_000n,
    collateral: 150_000_000n,
    maturityEpoch: 10n,
  },
  {
    id: "pos-2",
    borrower: "acct-2",
    pool: "pool-1",
    principal: 50_000_000n,
    quotedInterest: 2_000_000n,
    quotedPenalty: 0n,
    collateral: 80_000_000n,
    maturityEpoch: 5n,
  },
];

test("offline temporal stress matches the Rust reference table", () => {
  const report = computeTemporalStress({
    generatedEpoch: 2n,
    pools: [{ id: "pool-1", availableLiquidity: 200_000_000n, reserveBalance: 20_000_000n }],
    positions,
    policy,
  });
  assert.deepEqual(report.pools[0], {
    pool: "pool-1",
    positionCount: 2n,
    grossClaim: 158_000_000n,
    eligibleCollateral: 195_500_000n,
    projectedInterest: 4_740_000n,
    concentrationAddon: 10_600_000n,
    stressedObligation: 173_340_000n,
    eligibleResources: 415_500_000n,
    requiredCoverage: 218_008_000n,
    surplus: 197_492_000n,
    shortfall: 0n,
    coverageBps: 19_058n,
    largestBorrowerShareBps: 6_708n,
    hhiBps: 5_582n,
    weightedMaturityMilliEpochs: 6_354n,
    policySatisfied: true,
  });
  assert.equal(report.digest.length, 64);
});

test("portfolio decision preserves pool-level deficits", () => {
  const report = computeTemporalStress({
    generatedEpoch: 2n,
    pools: [
      { id: "pool-1", availableLiquidity: 5_000_000n, reserveBalance: 0n },
      { id: "pool-2", availableLiquidity: 1_000_000_000n, reserveBalance: 0n },
    ],
    positions,
    policy,
  });
  assert.equal(report.pools[0].policySatisfied, false);
  assert.equal(report.pools[1].policySatisfied, true);
  assert.equal(report.policySatisfied, false);
});

test("atomic values reject decimals and values outside u128", () => {
  assert.equal(atomic("340282366920938463463374607431768211455"), (1n << 128n) - 1n);
  assert.throws(() => atomic("1.25"), /unsigned integer/);
  assert.throws(() => atomic(-1), /outside/);
  assert.throws(() => atomic(1n << 128n), /outside/);
});

test("JSON transport encodes bigint values as canonical decimal strings", () => {
  assert.equal(
    jsonBody({ amount: 1_000_000_000n, epoch: 8n }),
    '{"amount":"1000000000","epoch":"8"}',
  );
});

test("client applies auth, idempotency, timeout signal, and canonical paths", async () => {
  const calls = [];
  const fetchImpl = async (url, options) => {
    calls.push({ url: String(url), options });
    return {
      ok: true,
      status: 200,
      headers: { get: () => "application/json; charset=utf-8" },
      json: async () => ({ accepted: true }),
    };
  };
  const client = new ChronosClient({
    baseUrl: "https://chronos.example/v1-root/",
    fetchImpl,
    token: "test-token",
    timeoutMs: 2_000,
  });
  const response = await client.settlePosition(
    { position: "pos-7", payer: "acct-3", maxTotalDue: 2_000_000_000n },
    "settlement:2026:0001",
  );
  assert.deepEqual(response, { accepted: true });
  assert.equal(calls[0].url, "https://chronos.example/v1/positions/pos-7/settle");
  assert.equal(calls[0].options.method, "POST");
  assert.equal(calls[0].options.headers.Authorization, "Bearer test-token");
  assert.equal(calls[0].options.headers["Idempotency-Key"], "settlement:2026:0001");
  assert.equal(calls[0].options.signal instanceof AbortSignal, true);
});

test("client rejects non-JSON and structured service errors", async () => {
  const nonJson = new ChronosClient({
    baseUrl: "http://localhost:3000",
    fetchImpl: async () => ({
      ok: true,
      status: 200,
      headers: { get: () => "text/plain" },
      json: async () => ({}),
    }),
  });
  await assert.rejects(() => nonJson.quotePosition("pos-1", 8), /not JSON/);

  const failed = new ChronosClient({
    baseUrl: "https://chronos.example",
    fetchImpl: async () => ({
      ok: false,
      status: 409,
      headers: { get: () => "application/json" },
      json: async () => ({ code: "state_conflict" }),
    }),
  });
  await assert.rejects(
    () =>
      failed.createLock({
        position: "pos-1",
        owner: "acct-1",
        releaseEpoch: 10n,
        mode: "rollover",
      }),
    /state_conflict/,
  );
});

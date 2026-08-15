"use strict";

const { createHash } = require("node:crypto");

const BPS = 10_000n;
const MAX_U128 = (1n << 128n) - 1n;

function invariant(condition, message) {
  if (!condition) {
    throw new TypeError(message);
  }
}

function atomic(value, field = "amount") {
  let parsed;
  if (typeof value === "bigint") {
    parsed = value;
  } else if (typeof value === "number" && Number.isSafeInteger(value)) {
    parsed = BigInt(value);
  } else if (typeof value === "string" && /^(0|[1-9][0-9]*)$/.test(value)) {
    parsed = BigInt(value);
  } else {
    throw new TypeError(
      `${field} must be an unsigned integer encoded as bigint, safe number, or decimal string`,
    );
  }
  invariant(parsed >= 0n && parsed <= MAX_U128, `${field} is outside the unsigned 128-bit domain`);
  return parsed;
}

function policyInteger(value, field, minimum, maximum) {
  const parsed = atomic(value, field);
  invariant(
    parsed >= BigInt(minimum) && parsed <= BigInt(maximum),
    `${field} is outside its policy range`,
  );
  return Number(parsed);
}

function ceilRatio(value, numerator, denominator = BPS) {
  invariant(
    value >= 0n && numerator >= 0n && denominator > 0n,
    "ceilRatio received an invalid domain",
  );
  const product = value * numerator;
  invariant(product <= MAX_U128, "stress arithmetic overflow");
  return product / denominator + (product % denominator === 0n ? 0n : 1n);
}

function canonicalId(value, field) {
  invariant(
    typeof value === "string" && /^[a-z][a-z0-9-]{0,62}$/.test(value),
    `${field} is not canonical`,
  );
  return value;
}

function normalizePolicy(policy) {
  invariant(policy && typeof policy === "object", "stress policy is required");
  const normalized = {
    collateralHaircutBps: policyInteger(
      policy.collateralHaircutBps,
      "collateralHaircutBps",
      0,
      10_000,
    ),
    liquidationCostBps: policyInteger(policy.liquidationCostBps, "liquidationCostBps", 0, 10_000),
    rateShockBpsPerEpoch: policyInteger(
      policy.rateShockBpsPerEpoch,
      "rateShockBpsPerEpoch",
      0,
      100_000,
    ),
    concentrationAddonBps: policyInteger(
      policy.concentrationAddonBps,
      "concentrationAddonBps",
      0,
      100_000,
    ),
    targetCoverageBps: policyInteger(policy.targetCoverageBps, "targetCoverageBps", 10_000, 30_000),
    horizonEpochs: policyInteger(policy.horizonEpochs, "horizonEpochs", 1, 365),
    operationalBuffer: atomic(policy.operationalBuffer, "operationalBuffer"),
  };
  invariant(
    normalized.collateralHaircutBps + normalized.liquidationCostBps <= 10_000,
    "collateral deductions exceed full collateral value",
  );
  return normalized;
}

function normalizePosition(position, generatedEpoch) {
  invariant(position && typeof position === "object", "stress position is required");
  const normalized = {
    id: canonicalId(position.id, "position id"),
    borrower: canonicalId(position.borrower, "borrower id"),
    pool: canonicalId(position.pool, "pool id"),
    principal: atomic(position.principal, "principal"),
    quotedInterest: atomic(position.quotedInterest, "quotedInterest"),
    quotedPenalty: atomic(position.quotedPenalty, "quotedPenalty"),
    collateral: atomic(position.collateral, "collateral"),
    maturityEpoch: atomic(position.maturityEpoch, "maturityEpoch"),
  };
  invariant(
    normalized.principal > 0n && normalized.collateral > 0n,
    "principal and collateral must be non-zero",
  );
  normalized.claim = normalized.principal + normalized.quotedInterest + normalized.quotedPenalty;
  invariant(normalized.claim <= MAX_U128, "position claim overflow");
  normalized.epochsToMaturity =
    normalized.maturityEpoch > generatedEpoch ? normalized.maturityEpoch - generatedEpoch : 0n;
  return normalized;
}

function computeTemporalStress({ generatedEpoch, pools, positions, policy: policyValue }) {
  const epoch = atomic(generatedEpoch, "generatedEpoch");
  const policy = normalizePolicy(policyValue);
  invariant(
    Array.isArray(pools) && pools.length > 0 && pools.length <= 128,
    "pools must contain 1..128 entries",
  );
  invariant(
    Array.isArray(positions) && positions.length <= 4_096,
    "positions exceed the client limit",
  );

  const normalizedPools = pools.map((pool) => ({
    id: canonicalId(pool.id, "pool id"),
    availableLiquidity: atomic(pool.availableLiquidity, "availableLiquidity"),
    reserveBalance: atomic(pool.reserveBalance, "reserveBalance"),
  }));
  invariant(
    new Set(normalizedPools.map((pool) => pool.id)).size === normalizedPools.length,
    "pool ids must be unique",
  );
  const normalizedPositions = positions.map((position) => normalizePosition(position, epoch));
  const poolIds = new Set(normalizedPools.map((pool) => pool.id));
  invariant(
    normalizedPositions.every((position) => poolIds.has(position.pool)),
    "position references an unmapped pool",
  );

  const reports = normalizedPools
    .map((pool) => {
      const local = normalizedPositions.filter((position) => position.pool === pool.id);
      const grossClaim = local.reduce((sum, position) => sum + position.claim, 0n);
      const collateral = local.reduce((sum, position) => sum + position.collateral, 0n);
      const eligibleRate =
        10_000n - BigInt(policy.collateralHaircutBps) - BigInt(policy.liquidationCostBps);
      const eligibleCollateral = (collateral * eligibleRate) / BPS;
      const projectedInterest = ceilRatio(
        grossClaim,
        BigInt(policy.rateShockBpsPerEpoch) * BigInt(policy.horizonEpochs),
      );

      const byBorrower = new Map();
      for (const position of local) {
        byBorrower.set(
          position.borrower,
          (byBorrower.get(position.borrower) ?? 0n) + position.claim,
        );
      }
      const claims = [...byBorrower.values()];
      const largestClaim = claims.reduce(
        (largest, claim) => (claim > largest ? claim : largest),
        0n,
      );
      const largestBorrowerShareBps = grossClaim === 0n ? 0n : (largestClaim * BPS) / grossClaim;
      const concentrationAddon = ceilRatio(largestClaim, BigInt(policy.concentrationAddonBps));
      const stressedObligation = grossClaim + projectedInterest + concentrationAddon;
      const requiredCoverage =
        ceilRatio(stressedObligation, BigInt(policy.targetCoverageBps)) + policy.operationalBuffer;
      const eligibleResources = pool.availableLiquidity + pool.reserveBalance + eligibleCollateral;
      const surplus =
        eligibleResources >= requiredCoverage ? eligibleResources - requiredCoverage : 0n;
      const shortfall =
        requiredCoverage > eligibleResources ? requiredCoverage - eligibleResources : 0n;
      const coverageBps =
        requiredCoverage === 0n
          ? eligibleResources === 0n
            ? BPS
            : 30_000n
          : (eligibleResources * BPS) / requiredCoverage;
      const hhiBps =
        grossClaim === 0n
          ? 0n
          : claims.reduce((sum, claim) => {
              const share = (claim * BPS) / grossClaim;
              return sum + (share * share) / BPS;
            }, 0n);
      const weightedMaturityMilliEpochs =
        grossClaim === 0n
          ? 0n
          : local.reduce(
              (sum, position) => sum + position.claim * position.epochsToMaturity * 1_000n,
              0n,
            ) / grossClaim;
      return {
        pool: pool.id,
        positionCount: BigInt(local.length),
        grossClaim,
        eligibleCollateral,
        projectedInterest,
        concentrationAddon,
        stressedObligation,
        eligibleResources,
        requiredCoverage,
        surplus,
        shortfall,
        coverageBps,
        largestBorrowerShareBps,
        hhiBps,
        weightedMaturityMilliEpochs,
        policySatisfied: shortfall === 0n,
      };
    })
    .sort((left, right) => left.pool.localeCompare(right.pool));

  const totalEligibleResources = reports.reduce(
    (sum, report) => sum + report.eligibleResources,
    0n,
  );
  const totalRequiredCoverage = reports.reduce((sum, report) => sum + report.requiredCoverage, 0n);
  const totalShortfall =
    totalRequiredCoverage > totalEligibleResources
      ? totalRequiredCoverage - totalEligibleResources
      : 0n;
  const canonical = reports
    .map(
      (report) =>
        `${report.pool}:${report.grossClaim}:${report.requiredCoverage}:${report.shortfall}`,
    )
    .join("|");
  return {
    generatedEpoch: epoch,
    policy,
    pools: reports,
    totalGrossClaim: reports.reduce((sum, report) => sum + report.grossClaim, 0n),
    totalEligibleResources,
    totalRequiredCoverage,
    totalSurplus:
      totalEligibleResources >= totalRequiredCoverage
        ? totalEligibleResources - totalRequiredCoverage
        : 0n,
    totalShortfall,
    policySatisfied: totalShortfall === 0n && reports.every((report) => report.policySatisfied),
    digest: createHash("sha256").update(`CHRONOS_STRESS_V1|${epoch}|${canonical}`).digest("hex"),
  };
}

function jsonBody(value) {
  return JSON.stringify(value, (_key, item) => (typeof item === "bigint" ? item.toString() : item));
}

function normalizeBaseUrl(value) {
  invariant(typeof value === "string" && value.length > 0, "baseUrl is required");
  const url = new URL(value);
  invariant(
    url.protocol === "https:" || url.hostname === "127.0.0.1" || url.hostname === "localhost",
    "baseUrl must use HTTPS outside localhost",
  );
  url.pathname = url.pathname.replace(/\/+$/, "");
  return url;
}

class ChronosClient {
  constructor({ baseUrl, fetchImpl = globalThis.fetch, token, timeoutMs = 10_000 } = {}) {
    this.baseUrl = normalizeBaseUrl(baseUrl);
    invariant(typeof fetchImpl === "function", "fetch implementation is required");
    invariant(
      token === undefined || (typeof token === "string" && token.length > 0),
      "token is invalid",
    );
    invariant(
      Number.isInteger(timeoutMs) && timeoutMs >= 100 && timeoutMs <= 120_000,
      "timeoutMs is outside 100..120000",
    );
    this.fetchImpl = fetchImpl;
    this.token = token;
    this.timeoutMs = timeoutMs;
  }

  async request(method, path, { body, idempotencyKey } = {}) {
    invariant(
      typeof path === "string" && path.startsWith("/") && !path.includes(".."),
      "request path is invalid",
    );
    invariant(
      idempotencyKey === undefined || /^[A-Za-z0-9._:-]{8,128}$/.test(idempotencyKey),
      "idempotency key is invalid",
    );
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), this.timeoutMs);
    const headers = { Accept: "application/json" };
    if (body !== undefined) headers["Content-Type"] = "application/json";
    if (this.token) headers.Authorization = `Bearer ${this.token}`;
    if (idempotencyKey) headers["Idempotency-Key"] = idempotencyKey;
    try {
      const response = await this.fetchImpl(new URL(path, this.baseUrl), {
        method,
        headers,
        body: body === undefined ? undefined : jsonBody(body),
        signal: controller.signal,
        redirect: "error",
      });
      const contentType = response.headers.get("content-type") ?? "";
      invariant(
        contentType.toLowerCase().includes("application/json"),
        "response content type is not JSON",
      );
      const payload = await response.json();
      if (!response.ok) {
        const code = typeof payload?.code === "string" ? payload.code : "request_failed";
        throw new Error(`ChronosDTL request failed (${response.status}): ${code}`);
      }
      return payload;
    } finally {
      clearTimeout(timeout);
    }
  }

  quotePosition(positionId, epoch) {
    const id = canonicalId(positionId, "position id");
    return this.request("POST", `/v1/positions/${id}/quote`, {
      body: { epoch: atomic(epoch, "epoch") },
    });
  }

  createLock(request, idempotencyKey) {
    invariant(request && typeof request === "object", "lock request is required");
    return this.request("POST", "/v1/locks", {
      body: {
        position: canonicalId(request.position, "position id"),
        owner: canonicalId(request.owner, "owner id"),
        releaseEpoch: atomic(request.releaseEpoch, "releaseEpoch"),
        mode: request.mode,
        reference: request.reference ?? "",
      },
      idempotencyKey,
    });
  }

  settlePosition(request, idempotencyKey) {
    invariant(request && typeof request === "object", "settlement request is required");
    return this.request(
      "POST",
      `/v1/positions/${canonicalId(request.position, "position id")}/settle`,
      {
        body: {
          payer: canonicalId(request.payer, "payer id"),
          maxTotalDue: atomic(request.maxTotalDue, "maxTotalDue"),
        },
        idempotencyKey,
      },
    );
  }

  stressPortfolio(request) {
    return this.request("POST", "/v1/risk/temporal-stress", { body: request });
  }
}

module.exports = {
  ChronosClient,
  atomic,
  computeTemporalStress,
  jsonBody,
  normalizePolicy,
};

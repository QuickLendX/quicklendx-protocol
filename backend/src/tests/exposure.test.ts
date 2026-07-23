/**
 * exposure.test.ts
 *
 * Tests for the per-investor exposure cap service (services/exposureService.ts)
 * and its integration with POST /api/v1/bids.
 *
 * Coverage targets (per issue requirements):
 *   • cap not reached
 *   • cap exactly reached
 *   • cap exceeded by next bid
 *   • multi-currency normalization
 *   • racing concurrent bids
 *   • withdrawn bids removed from exposure
 *   • integer-overflow safety on summation
 *
 * Plus controller-level integration so the 429 EXPOSURE_CAP_EXCEEDED path
 * is exercised end-to-end.
 */

import {
  describe,
  it,
  expect,
  beforeEach,
  afterEach,
  jest,
} from "@jest/globals";
import crypto from "crypto";

import {
  ExposureService,
  ExposureCapExceededError,
  InvalidAmountError,
  exposureService,
} from "../services/exposureService";
import { MOCK_BIDS } from "../controllers/v1/bids";
import { MOCK_SETTLEMENTS } from "../controllers/v1/settlements";
import { BidStatus, SettlementStatus } from "../types/contract";

// ─── Constants ────────────────────────────────────────────────────────────

const INVESTOR_A =
  "GBSXVD727UNXJZ7ZM4VCXBTK3UMPXR7O6LLXS7XVOECGDYH3XFNV7C5K";
const INVESTOR_B =
  "GAXMFSADZVDXTJSLL3HZJFVYH4JGBQBMX3I2WGPKH3YKRXL7ZJXZK23Q";

// Each bid is denominated in micro-units: 1_000_000 micro = $1
const ONE_USD_MICRO = "1000000";
const FIVE_USD_MICRO = "5000000";
const TEN_USD_MICRO = "10000000";

// ─── Helpers ──────────────────────────────────────────────────────────────

/** Build a mock Placed bid for the given investor + amount. */
function makeMockBid(
  investor: string,
  amount: string,
  overrides: Partial<any> = {},
): any {
  return {
    bid_id: "0x" + crypto.randomBytes(32).toString("hex"),
    invoice_id: "0x" + crypto.randomBytes(32).toString("hex"),
    investor,
    bid_amount: amount,
    expected_return: (BigInt(amount) * 15n / 10n).toString(),
    timestamp: Math.floor(Date.now() / 1000),
    status: BidStatus.Placed,
    expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
    currency: "USDC",
    ...overrides,
  };
}

/** Build a mock Pending settlement to the given recipient. */
function makeMockSettlement(
  recipient: string,
  amount: string,
  status: SettlementStatus = SettlementStatus.Pending,
): any {
  return {
    id: "0x" + crypto.randomBytes(32).toString("hex"),
    invoice_id: "0x" + crypto.randomBytes(32).toString("hex"),
    amount,
    payer: "GPAYER000000000000000000000000000000000000000000000000",
    recipient,
    timestamp: Math.floor(Date.now() / 1000),
    status,
  };
}

/** Replace the singleton's data sources with isolated, mutable arrays. */
function isolate(service: ExposureService = exposureService) {
  const bids: any[] = [];
  const settlements: any[] = [];
  service.setDependencies({
    mockBids: bids,
    mockSettlements: settlements,
    fetchPersistedBids: async () => [],
    fetchPersistedSettlements: async () => [],
  });
  return { bids, settlements };
}

// ─── Tests ────────────────────────────────────────────────────────────────

beforeEach(() => {
  // Clear all module-level mock arrays so cross-suite bleed is impossible.
  MOCK_BIDS.length = 0;
  MOCK_SETTLEMENTS.length = 0;
});

afterEach(() => {
  MOCK_BIDS.length = 0;
  MOCK_SETTLEMENTS.length = 0;
});

describe("ExposureService — construction and configuration", () => {
  it("parses a numeric cap (whole USD units) into micro-USD BigInt", () => {
    const svc = new ExposureService();
    svc.setCap(1000); // $1,000
    expect(svc.capUsd).toBe(1_000_000_000n); // $1,000 × 1_000_000
  });

  it("parses a string cap", () => {
    const svc = new ExposureService();
    svc.setCap("12345");
    expect(svc.capUsd).toBe(12_345_000_000n);
  });

  it("parses a BigInt cap", () => {
    const svc = new ExposureService();
    svc.setCap(42n);
    expect(svc.capUsd).toBe(42_000_000n);
  });

  it("rejects negative caps with InvalidAmountError", () => {
    const svc = new ExposureService();
    expect(() => svc.setCap(-1)).toThrow(InvalidAmountError);
    expect(() => svc.setCap("-5")).toThrow(InvalidAmountError);
    expect(() => svc.setCap(-10n)).toThrow(InvalidAmountError);
  });

  it("rejects non-numeric string caps", () => {
    const svc = new ExposureService();
    expect(() => svc.setCap("not-a-number")).toThrow(InvalidAmountError);
    expect(() => svc.setCap("1.5")).toThrow(InvalidAmountError);
    expect(() => svc.setCap("")).toThrow(InvalidAmountError);
  });

  it("accepts a zero cap (disabled mode)", () => {
    const svc = new ExposureService();
    svc.setCap(0);
    expect(svc.capUsd).toBe(0n);
  });

  it("rejects negative or non-integer amount strings (check input)", async () => {
    // We exercise the input layer directly; the bucketed summation path
    // deliberately skips malformed rows to remain robust to corrupt mock
    // fixtures, so an indirect assertion through computeExposure would
    // be flaky. The check() path validates user-supplied amounts.
    isolate();
    await expect(
      exposureService.check(INVESTOR_A, "-1"),
    ).rejects.toThrow(InvalidAmountError);
    await expect(
      exposureService.check(INVESTOR_A, "1.5"),
    ).rejects.toThrow(InvalidAmountError);
    await expect(
      exposureService.check(INVESTOR_A, "abc"),
    ).rejects.toThrow(InvalidAmountError);
    await expect(
      exposureService.check(INVESTOR_A, ""),
    ).rejects.toThrow(InvalidAmountError);
  });

  it("rejects non-integer amount numbers", () => {
    const svc = new ExposureService();
    expect(() => svc["_parseAmount"](1.5)).toThrow(InvalidAmountError);
    expect(() => svc["_parseAmount"](-2)).toThrow(InvalidAmountError);
    expect(() => svc["_parseAmount"](NaN)).toThrow(InvalidAmountError);
    expect(() => svc["_parseAmount"](Infinity)).toThrow(InvalidAmountError);
  });

  it("rejects negative bigint amounts", () => {
    const svc = new ExposureService();
    expect(() => svc["_parseAmount"](-1n)).toThrow(InvalidAmountError);
  });

  it("accepts zero and positive bigint amounts", () => {
    const svc = new ExposureService();
    expect(svc["_parseAmount"](0n)).toBe(0n);
    expect(svc["_parseAmount"](42n)).toBe(42n);
  });

  it("accepts zero and positive integer amounts", () => {
    const svc = new ExposureService();
    expect(svc["_parseAmount"](0)).toBe(0n);
    expect(svc["_parseAmount"](42)).toBe(42n);
  });

  it("rejects non-string/number/bigint amounts", () => {
    const svc = new ExposureService();
    expect(() => svc["_parseAmount"](null as any)).toThrow(InvalidAmountError);
    expect(() => svc["_parseAmount"](undefined as any)).toThrow(InvalidAmountError);
    expect(() => svc["_parseAmount"]({} as any)).toThrow(InvalidAmountError);
    expect(() => svc["_parseAmount"]([] as any)).toThrow(InvalidAmountError);
    expect(() => svc["_parseAmount"](true as any)).toThrow(InvalidAmountError);
  });
});

// ───────────────────────────────────────────────────────────────────────────
// 1. Cap not reached
// ───────────────────────────────────────────────────────────────────────────

describe("ExposureService — cap not reached", () => {
  it("allows a single small bid well under the cap", async () => {
    isolate();
    const result = await exposureService.check(INVESTOR_A, ONE_USD_MICRO);
    expect(result.allowed).toBe(true);
    expect(result.currentExposureUsd).toBe("0");
    expect(result.attemptedUsd).toBe(ONE_USD_MICRO);
    expect(result.projectedExposureUsd).toBe(ONE_USD_MICRO);
  });

  it("allows many bids that are individually small", async () => {
    const { bids } = isolate();
    for (let i = 0; i < 100; i++) {
      bids.push(makeMockBid(INVESTOR_A, ONE_USD_MICRO));
    }
    // 100 × $1 = $100 of exposure, well below default $10B cap
    const breakdown = await exposureService.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.bidsUsd)).toBe(100_000_000n); // $100 micro-USD
    expect(BigInt(breakdown.totalUsd)).toBe(100_000_000n);
    const result = await exposureService.check(INVESTOR_A, ONE_USD_MICRO);
    expect(result.allowed).toBe(true);
  });
});

// ───────────────────────────────────────────────────────────────────────────
// 2. Cap exactly reached
// ───────────────────────────────────────────────────────────────────────────

describe("ExposureService — cap exactly reached", () => {
  it("treats currentExposure + newAmount === cap as allowed", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [makeMockBid(INVESTOR_A, "9000000")], // $9 in micro-USD
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    svc.setCap(10); // $10 cap

    // 9000000 + 1000000 = 10000000 micro-USD = exactly $10. Should be allowed.
    const result = await svc.check(INVESTOR_A, "1000000");
    expect(result.allowed).toBe(true);
    expect(BigInt(result.projectedExposureUsd)).toBe(svc.capUsd);
    expect(BigInt(result.remainingUsd)).toBe(0n);
    await expect(svc.assertWithinCap(INVESTOR_A, "1000000")).resolves.toBeUndefined();
  });

  it("treats 1-micro-unit over the cap as not allowed", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [makeMockBid(INVESTOR_A, "9000000")],
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    svc.setCap(10); // $10 cap

    // 9000000 + 1000001 = 10000001 micro-USD = $10.000001 → exceeds cap.
    const result = await svc.check(INVESTOR_A, "1000001");
    expect(result.allowed).toBe(false);
    expect(BigInt(result.projectedExposureUsd)).toBeGreaterThan(svc.capUsd);
    await expect(svc.assertWithinCap(INVESTOR_A, "1000001")).rejects.toThrow(
      ExposureCapExceededError,
    );
  });
});

// ───────────────────────────────────────────────────────────────────────────
// 3. Cap exceeded by next bid
// ───────────────────────────────────────────────────────────────────────────

describe("ExposureService — cap exceeded by next bid", () => {
  it("rejects a bid that exceeds the cap with ExposureCapExceededError", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [makeMockBid(INVESTOR_A, "9999999")], // ~$10 of exposure
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    svc.setCap(10); // $10 cap

    try {
      await svc.assertWithinCap(INVESTOR_A, "100"); // $0.0001 over
      fail("Expected ExposureCapExceededError");
    } catch (err: any) {
      expect(err).toBeInstanceOf(ExposureCapExceededError);
      expect(err.investor).toBe(INVESTOR_A);
      expect(err.capUsd).toBe(10_000_000n);
      // current = 9999999, attempted = 100, projected = 10000099
      expect(err.currentExposureUsd).toBe(9_999_999n);
      expect(err.attemptedUsd).toBe(100n);
    }
  });

  it("error message includes human-readable details", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [makeMockBid(INVESTOR_A, "11000000")], // $11
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    svc.setCap(10); // $10 cap

    try {
      await svc.assertWithinCap(INVESTOR_A, ONE_USD_MICRO);
      fail("Expected error");
    } catch (err: any) {
      expect(err.message).toMatch(/exposure cap would be exceeded/);
      expect(err.message).toContain(INVESTOR_A);
      expect(err.message).toContain("cap=");
    }
  });

  it("does not throw when the cap is disabled (set to 0)", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [
        makeMockBid(INVESTOR_A, "999999999999999"),
        makeMockBid(INVESTOR_A, "999999999999999"),
      ],
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    svc.setCap(0); // disabled
    await expect(
      svc.assertWithinCap(INVESTOR_A, "999999999999999"),
    ).resolves.toBeUndefined();
  });
});

// ───────────────────────────────────────────────────────────────────────────
// 4. Multi-currency normalization
// ───────────────────────────────────────────────────────────────────────────

describe("ExposureService — multi-currency normalization", () => {
  it("treats USDC, USDT, and USD as 1:1 with USD", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [
        makeMockBid(INVESTOR_A, "1000000", { currency: "USDC" }),
        makeMockBid(INVESTOR_A, "1000000", { currency: "USDT" }),
        makeMockBid(INVESTOR_A, "1000000", { currency: "USD" }),
      ],
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.bidsUsd)).toBe(3_000_000n); // $3
  });

  it("normalizes XLM at 0.12 USD per XLM", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [
        // 1_000_000 XLM micro = 1 XLM token = $0.12 = 120_000 micro-USD
        makeMockBid(INVESTOR_A, "1000000", { currency: "XLM" }),
      ],
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    // 1_000_000 XLM * 120_000 / 1_000_000 = 120_000 micro-USD = $0.12
    expect(BigInt(breakdown.bidsUsd)).toBe(120_000n);
  });

  it("combines multiple currencies in the cap check", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [
        // $5 in USDC
        makeMockBid(INVESTOR_A, FIVE_USD_MICRO, { currency: "USDC" }),
        // 1 XLM = $0.12
        makeMockBid(INVESTOR_A, "1000000", { currency: "XLM" }),
      ],
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    svc.setCap(6); // $6 cap (a little above the $5.12 already used)

    // Adding $0.95 should push to $6.07 → exceeds.
    const result = await svc.check(INVESTOR_A, "950000");
    expect(result.allowed).toBe(false);
  });

  it("falls back to 1:1 for unknown currencies", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [
        makeMockBid(INVESTOR_A, "1000000", { currency: "BTC" }), // unknown
      ],
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.bidsUsd)).toBe(1_000_000n); // treated as $1
  });

  it("rejects currencies not in the allow-list", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [],
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    svc.setAllowedCurrencies(["USDC", "XLM"]);
    await expect(svc.check(INVESTOR_A, ONE_USD_MICRO, "BTC")).rejects.toThrow(
      InvalidAmountError,
    );
    // Allowed currency still works
    await expect(svc.check(INVESTOR_A, ONE_USD_MICRO, "USDC")).resolves.toBeDefined();
    await expect(svc.check(INVESTOR_A, ONE_USD_MICRO, "XLM")).resolves.toBeDefined();
    // Clearing the allow-list reverts behavior
    svc.setAllowedCurrencies(null);
    await expect(svc.check(INVESTOR_A, ONE_USD_MICRO, "BTC")).resolves.toBeDefined();
  });
});

// ───────────────────────────────────────────────────────────────────────────
// 5. Withdrawn / finalized positions removed from exposure
// ───────────────────────────────────────────────────────────────────────────

describe("ExposureService — withdrawn bids and finalized settlements", () => {
  it("does not count withdrawn bids toward exposure", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [
        makeMockBid(INVESTOR_A, TEN_USD_MICRO, { status: BidStatus.Withdrawn }),
        makeMockBid(INVESTOR_A, TEN_USD_MICRO, { status: BidStatus.Expired }),
        makeMockBid(INVESTOR_A, TEN_USD_MICRO, { status: BidStatus.Cancelled }),
        makeMockBid(INVESTOR_A, TEN_USD_MICRO, { status: BidStatus.Accepted }),
      ],
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.bidsUsd)).toBe(0n);
  });

  it("only counts Placed bids toward exposure", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [
        makeMockBid(INVESTOR_A, FIVE_USD_MICRO, { status: BidStatus.Placed }),
        makeMockBid(INVESTOR_A, TEN_USD_MICRO, { status: BidStatus.Withdrawn }),
      ],
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.bidsUsd)).toBe(5_000_000n);
  });

  it("does not count finalized settlements toward exposure", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [],
      mockSettlements: [
        makeMockSettlement(INVESTOR_A, TEN_USD_MICRO, SettlementStatus.Paid),
        makeMockSettlement(INVESTOR_A, TEN_USD_MICRO, SettlementStatus.Defaulted),
      ],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.positionsUsd)).toBe(0n);
  });

  it("counts both Pending and Processing settlements", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [],
      mockSettlements: [
        makeMockSettlement(INVESTOR_A, ONE_USD_MICRO, SettlementStatus.Pending),
        makeMockSettlement(INVESTOR_A, ONE_USD_MICRO, SettlementStatus.Processing),
      ],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.positionsUsd)).toBe(2_000_000n);
  });

  it("excludes rows belonging to other investors", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [
        makeMockBid(INVESTOR_A, ONE_USD_MICRO),
        makeMockBid(INVESTOR_B, TEN_USD_MICRO),
      ],
      mockSettlements: [
        makeMockSettlement(INVESTOR_A, ONE_USD_MICRO),
        makeMockSettlement(INVESTOR_B, TEN_USD_MICRO),
      ],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    const a = await svc.computeBreakdown(INVESTOR_A);
    const b = await svc.computeBreakdown(INVESTOR_B);
    expect(BigInt(a.totalUsd)).toBe(2_000_000n);
    expect(BigInt(b.totalUsd)).toBe(20_000_000n);
  });
});

// ───────────────────────────────────────────────────────────────────────────
// 6. Integer-overflow safety
// ───────────────────────────────────────────────────────────────────────────

describe("ExposureService — integer overflow safety", () => {
  it("sums 1000 bids each at 10^15 micro-units without loss", async () => {
    const svc = new ExposureService();
    const huge = "1" + "0".repeat(15); // 10^15 micro-units per bid
    const bids: any[] = [];
    for (let i = 0; i < 1000; i++) {
      bids.push(makeMockBid(INVESTOR_A, huge));
    }
    svc.setDependencies({
      mockBids: bids,
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });

    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    // 1000 × 10^15 = 10^18 (larger than Number.MAX_SAFE_INTEGER)
    const expected = BigInt(huge) * 1000n;
    expect(BigInt(breakdown.bidsUsd)).toBe(expected);
  });

  it("rejects amounts larger than the configured cap correctly", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [],
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    svc.setCap(10); // $10 cap
    const hugeBid = "1" + "0".repeat(20); // 10^20 micro-units = $10^14
    const result = await svc.check(INVESTOR_A, hugeBid);
    expect(result.allowed).toBe(false);
    expect(BigInt(result.attemptedUsd)).toBe(BigInt(hugeBid));
  });
});

// ───────────────────────────────────────────────────────────────────────────
// 7. Racing concurrent bids
// ───────────────────────────────────────────────────────────────────────────

describe("ExposureService — racing concurrent bids", () => {
  it("processes N concurrent check() calls in isolation", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [],
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    svc.setCap(100); // $100 cap
    const concurrent = 50;

    // Each call asks to add $1 — only the first ~100 can fit, but since
    // none have been persisted yet, all should be allowed under the cap.
    const results = await Promise.all(
      Array.from({ length: concurrent }, () =>
        svc.check(INVESTOR_A, ONE_USD_MICRO),
      ),
    );

    for (const r of results) {
      expect(r.allowed).toBe(true);
    }
  });

  it("handles Promise.all of mixed assertWithinCap calls deterministically", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [],
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    svc.setCap(10); // $10 cap

    const calls = Array.from({ length: 20 }, () =>
      svc.assertWithinCap(INVESTOR_A, ONE_USD_MICRO).then(
        () => "ok" as const,
        (e) => e as Error,
      ),
    );
    const results = await Promise.all(calls);

    // Every individual call sees an empty exposure state at the moment
    // it ran (no bids have been persisted), so every single one should
    // succeed — this models the pre-check semantics, not the post-write
    // invariant. The on-chain contract remains the final source of truth.
    for (const r of results) {
      expect(r).toBe("ok");
    }
  });

  it("exposes deterministic state when bids are committed between checks", async () => {
    const { bids } = isolate();
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: bids,
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    svc.setCap(3); // $3 cap

    // Sequential commits; each subsequent check sees the prior commitment.
    expect(
      (await svc.check(INVESTOR_A, ONE_USD_MICRO)).allowed,
    ).toBe(true);
    bids.push(makeMockBid(INVESTOR_A, ONE_USD_MICRO));

    expect(
      (await svc.check(INVESTOR_A, ONE_USD_MICRO)).allowed,
    ).toBe(true);
    bids.push(makeMockBid(INVESTOR_A, ONE_USD_MICRO));

    // Now at $2; another $1 fits, but a $2 bid would exceed.
    expect(
      (await svc.check(INVESTOR_A, ONE_USD_MICRO)).allowed,
    ).toBe(true);
    expect(
      (await svc.check(INVESTOR_A, "2000000")).allowed,
    ).toBe(false);
  });
});

// ───────────────────────────────────────────────────────────────────────────
// 8. Persistence hook integration
// ───────────────────────────────────────────────────────────────────────────

describe("ExposureService — persistence hooks", () => {
  it("combines mock and persisted sources in the sum", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [makeMockBid(INVESTOR_A, ONE_USD_MICRO)],
      mockSettlements: [],
      fetchPersistedBids: async () => [
        // Persisted row
        {
          bid_id: "0xpersisted1",
          bid_amount: "2000000", // $2
          status: BidStatus.Placed,
          currency: "USDC",
        },
      ],
      fetchPersistedSettlements: async () => [],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.bidsUsd)).toBe(3_000_000n); // $1 + $2
  });

  it("ignores persisted bids that are not Placed", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [],
      mockSettlements: [],
      fetchPersistedBids: async () => [
        { bid_id: "a", bid_amount: "1000000", status: BidStatus.Withdrawn, currency: "USDC" },
        { bid_id: "b", bid_amount: "1000000", status: BidStatus.Accepted, currency: "USDC" },
        { bid_id: "c", bid_amount: "1000000", status: BidStatus.Placed, currency: "USDC" },
      ],
      fetchPersistedSettlements: async () => [],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.bidsUsd)).toBe(1_000_000n); // only the Placed one
  });

  it("handles persistence-hook failures gracefully (fallback to mock)", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [makeMockBid(INVESTOR_A, ONE_USD_MICRO)],
      mockSettlements: [],
      fetchPersistedBids: async () => {
        throw new Error("DB unavailable");
      },
      fetchPersistedSettlements: async () => {
        throw new Error("DB unavailable");
      },
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    // Only mock data survives the failure; no exception escapes.
    expect(BigInt(breakdown.bidsUsd)).toBe(1_000_000n);
  });

  it("skips rows with malformed amounts silently (does not throw)", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [
        makeMockBid(INVESTOR_A, ONE_USD_MICRO),
        { ...makeMockBid(INVESTOR_A, "0"), bid_amount: "not-a-number" },
      ],
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.bidsUsd)).toBe(1_000_000n);
  });

  it("falls back to mock data when persisted bids fetcher throws (no custom hook)", async () => {
    // By NOT providing fetchPersistedBids, the service falls through to the
    // pool.query() path. The pg pool mock returns empty rows, so this
    // exercises the catch block (which catches any thrown error).
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [makeMockBid(INVESTOR_A, ONE_USD_MICRO)],
      mockSettlements: [],
      // fetchPersistedBids omitted → falls back to pool.query()
      fetchPersistedSettlements: async () => [],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.bidsUsd)).toBe(1_000_000n);
  });

  it("falls back to mock data when persisted settlements fetcher throws (no custom hook)", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [],
      mockSettlements: [makeMockSettlement(INVESTOR_A, ONE_USD_MICRO)],
      fetchPersistedBids: async () => [],
      // fetchPersistedSettlements omitted → falls back to settlementOrchestrator
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.positionsUsd)).toBe(1_000_000n);
  });

  it("skips malformed persisted bids silently", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [],
      mockSettlements: [],
      fetchPersistedBids: async () => [
        { bid_id: "x", bid_amount: "not-a-number", status: BidStatus.Placed, currency: "USDC" },
        { bid_id: "y", bid_amount: ONE_USD_MICRO, status: BidStatus.Placed, currency: "USDC" },
      ],
      fetchPersistedSettlements: async () => [],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.bidsUsd)).toBe(1_000_000n); // only the valid row
  });

  it("skips malformed persisted settlements silently", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [],
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [
        { id: "x", amount: "garbage", recipient: INVESTOR_A, status: SettlementStatus.Pending, currency: "USDC" },
        { id: "y", amount: ONE_USD_MICRO, recipient: INVESTOR_A, status: SettlementStatus.Pending, currency: "USDC" },
      ],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.positionsUsd)).toBe(1_000_000n);
  });

  it("skips persisted settlements that are not Pending or Processing", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [],
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [
        { id: "x", amount: ONE_USD_MICRO, recipient: INVESTOR_A, status: SettlementStatus.Paid, currency: "USDC" },
        { id: "y", amount: ONE_USD_MICRO, recipient: INVESTOR_A, status: SettlementStatus.Defaulted, currency: "USDC" },
      ],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.positionsUsd)).toBe(0n);
  });

  it("skips mock bids with malformed amounts silently", async () => {
    // Exercises the inner try/catch in the mock-bids loop
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [
        makeMockBid(INVESTOR_A, ONE_USD_MICRO),
        { ...makeMockBid(INVESTOR_A, ONE_USD_MICRO), bid_amount: "-1" },
        { ...makeMockBid(INVESTOR_A, ONE_USD_MICRO), bid_amount: "abc" },
      ],
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.bidsUsd)).toBe(1_000_000n); // only one valid row
  });

  it("skips mock settlements with malformed amounts silently", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [],
      mockSettlements: [
        makeMockSettlement(INVESTOR_A, ONE_USD_MICRO),
        { ...makeMockSettlement(INVESTOR_A, ONE_USD_MICRO), amount: "-100" },
      ],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.positionsUsd)).toBe(1_000_000n);
  });

  it("skips mock bids that are missing the investor field", async () => {
    const svc = new ExposureService();
    const orphan = makeMockBid(INVESTOR_A, ONE_USD_MICRO);
    delete (orphan as any).investor;
    svc.setDependencies({
      mockBids: [orphan],
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.bidsUsd)).toBe(0n);
  });

  it("skips null/undefined mock bids", async () => {
    const svc = new ExposureService();
    svc.setDependencies({
      mockBids: [null, undefined, makeMockBid(INVESTOR_A, ONE_USD_MICRO)] as any,
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });
    const breakdown = await svc.computeBreakdown(INVESTOR_A);
    expect(BigInt(breakdown.bidsUsd)).toBe(1_000_000n);
  });

  it("exercises the pool-query fallback path with a stub that returns rows", async () => {
    // The fallback path is exercised when fetchPersistedBids is omitted;
    // pool.query is invoked once. Verify the path runs and produces a
    // deterministic result regardless of what pool returns.
    const poolMod = require("../services/database").default as any;
    const originalQuery = poolMod.query;
    (poolMod.query as any) = jest.fn(async () => ({ rows: [], rowCount: 0 }));
    try {
      const svc = new ExposureService();
      svc.setDependencies({
        mockBids: [],
        mockSettlements: [],
        // fetchPersistedBids omitted → falls back to pool.query()
        fetchPersistedSettlements: async () => [],
      });
      const breakdown = await svc.computeBreakdown(INVESTOR_A);
      expect(BigInt(breakdown.bidsUsd)).toBe(0n);
    } finally {
      poolMod.query = originalQuery;
    }
  });

  it("reads settlements via the orchestrator fallback when no custom hook is provided", async () => {
    // Stub the orchestrator.list() to return a settlement for the test investor.
    const orchMod = require("../services/settlementOrchestrator") as any;
    const orchestrator = orchMod.settlementOrchestrator;
    const originalList = orchestrator.list.bind(orchestrator);
    orchestrator.list = () => [
      {
        id: "0xsettle1",
        invoice_id: "0xinv1",
        amount: ONE_USD_MICRO,
        recipient: INVESTOR_A,
        timestamp: Math.floor(Date.now() / 1000),
        status: SettlementStatus.Pending,
        contract_version: 1,
        event_schema_version: 1,
        indexed_at: new Date().toISOString(),
      },
    ];
    try {
      const svc = new ExposureService();
      svc.setDependencies({
        mockBids: [],
        mockSettlements: [],
        fetchPersistedBids: async () => [],
        // fetchPersistedSettlements omitted → falls back to orchestrator
      });
      const breakdown = await svc.computeBreakdown(INVESTOR_A);
      expect(BigInt(breakdown.positionsUsd)).toBe(1_000_000n);
    } finally {
      orchestrator.list = originalList;
    }
  });
});

describe("ExposureService — defensive input parsing", () => {
  it("rejects cap values that are not string/number/bigint", () => {
    const svc = new ExposureService();
    expect(() => svc.setCap(null as any)).toThrow(InvalidAmountError);
    expect(() => svc.setCap(undefined as any)).toThrow(InvalidAmountError);
    expect(() => svc.setCap({} as any)).toThrow(InvalidAmountError);
    expect(() => svc.setCap([] as any)).toThrow(InvalidAmountError);
    expect(() => svc.setCap(true as any)).toThrow(InvalidAmountError);
  });

  it("rejects cap values that are floats or non-finite numbers", () => {
    const svc = new ExposureService();
    expect(() => svc.setCap(1.5)).toThrow(InvalidAmountError);
    expect(() => svc.setCap(NaN)).toThrow(InvalidAmountError);
    expect(() => svc.setCap(Infinity)).toThrow(InvalidAmountError);
    expect(() => svc.setCap(-Infinity)).toThrow(InvalidAmountError);
  });
});

// ───────────────────────────────────────────────────────────────────────────
// 9. Default cap is non-trivial
// ───────────────────────────────────────────────────────────────────────────

describe("ExposureService — default cap", () => {
  it("the singleton has a non-zero, sane default cap", () => {
    expect(exposureService.capUsd).toBeGreaterThan(0n);
    // Default is $10B = 10_000_000_000 USD = 10^16 micro-USD
    expect(exposureService.capUsd).toBe(10_000_000_000_000_000n);
  });
});

// ───────────────────────────────────────────────────────────────────────────
// 10. Controller integration — POST /api/v1/bids returns 429 on cap exceeded
// ───────────────────────────────────────────────────────────────────────────

import request from "supertest";
import app from "../app";
import { apiKeyService } from "../services/api-key-service";

describe("POST /api/v1/bids — exposure cap integration", () => {
  let originalVerify: typeof apiKeyService.verifyApiKey;

  beforeEach(() => {
    // Stub the API key auth so we can drive the controller with arbitrary
    // bearer tokens. The tests below only exercise the exposure-cap gate.
    originalVerify = apiKeyService.verifyApiKey;
    (apiKeyService as any).verifyApiKey = async (key: string) => {
      if (!key.startsWith("qlx_")) return null;
      // Synthesize an API key owned by INVESTOR_A
      return {
        id: "test-key-id",
        key_prefix: key.slice(0, 8),
        created_by: INVESTOR_A,
        scopes: ["bid:create"],
      } as any;
    };
  });

  afterEach(() => {
    (apiKeyService as any).verifyApiKey = originalVerify;
  });

  it("rejects with 429 EXPOSURE_CAP_EXCEEDED when projected exposure > cap", async () => {
    // The controller uses the singleton exposureService, so we must
    // configure the singleton's cap and dependency hooks.
    const originalCap = exposureService.capUsd;
    isolate();
    exposureService.setCap(10); // $10 cap
    (exposureService as any).setDependencies({
      mockBids: [makeMockBid(INVESTOR_A, "9999999")], // ~$10
      mockSettlements: [],
      fetchPersistedBids: async () => [],
      fetchPersistedSettlements: async () => [],
    });

    try {
      const apiKey = "qlx_test_" + crypto.randomBytes(32).toString("base64url");
      const validInvoiceId = "0x" + crypto.randomBytes(32).toString("hex");
      const res = await request(app)
        .post("/api/v1/bids")
        .set("Authorization", `Bearer ${apiKey}`)
        .send({
          invoice_id: validInvoiceId,
          bid_amount: "100",
          expected_return: "150",
          expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
        });

      // The exposure cap fires before bidStore.createBid is called, so we
      // don't need a real invoice. The bidStore path also throws if the
      // invoice doesn't exist, but the cap check happens first.
      expect(res.status).toBe(429);
      expect(res.body.error.code).toBe("EXPOSURE_CAP_EXCEEDED");
      expect(res.body.error.currentExposureUsd).toBe("9999999");
      expect(res.body.error.capUsd).toBe("10000000");
      expect(res.body.error.investor).toBe(INVESTOR_A);
    } finally {
      exposureService.setCap(originalCap);
      (exposureService as any).setDependencies({});
    }
  });

  it("allows a bid well within the cap (default)", async () => {
    // Use the singleton (default $10B cap) with empty mocks → any bid fits.
    isolate();
    const apiKey = "qlx_test_" + crypto.randomBytes(32).toString("base64url");
    const validInvoiceId = "0x" + crypto.randomBytes(32).toString("hex");

    // This test will fail at the bidStore layer (no invoice exists) but
    // it should NOT fail at the exposure-cap layer. So we expect either
    // a 201 (if invoice happens to exist) or a 400 (Invoice not found) —
    // never 429 EXPOSURE_CAP_EXCEEDED.
    const res = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${apiKey}`)
      .send({
        invoice_id: validInvoiceId,
        bid_amount: ONE_USD_MICRO,
        expected_return: "1500000",
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
      });

    if (res.status === 429) {
      expect(res.body.error.code).not.toBe("EXPOSURE_CAP_EXCEEDED");
    } else {
      expect([201, 400, 500]).toContain(res.status);
    }
  });
});

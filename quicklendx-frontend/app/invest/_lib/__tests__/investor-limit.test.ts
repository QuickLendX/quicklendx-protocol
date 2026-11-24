/**
 * Tests for the investor-limit pre-check (#3176).
 *
 * The rule under test mirrors `exposureService.check` in the backend:
 *   allowed  <=>  cap is 0  OR  current + attempted <= cap
 *
 * Run with: npx jest app/invest/_lib/__tests__/investor-limit.test.ts
 */

import {
  checkInvestorLimit,
  parseLimitFromDetails,
  type InvestorLimit,
} from "../investor-limit";

const limit = (cap: string, current: string): InvestorLimit => ({
  capUsd: cap,
  currentExposureUsd: current,
});

const B = (n: number | string) => BigInt(n);

describe("checkInvestorLimit", () => {
  describe("the boundary", () => {
    it("allows a bid that lands exactly on the cap (<=, not <)", () => {
      expect(checkInvestorLimit(B(400), "USDC", limit("1000", "600"))).toEqual({
        status: "ok",
      });
    });

    it("refuses a bid one unit over the cap", () => {
      expect(checkInvestorLimit(B(401), "USDC", limit("1000", "600"))).toEqual({
        status: "exceeded",
        capUsd: "1000",
        currentExposureUsd: "600",
        attemptedUsd: "401",
        remainingUsd: "400",
      });
    });

    it("allows a bid well under the cap", () => {
      expect(checkInvestorLimit(B(1), "USDC", limit("1000", "0")).status).toBe(
        "ok"
      );
    });
  });

  describe("remaining headroom", () => {
    it("reports what is left under the cap", () => {
      const r = checkInvestorLimit(B(500), "USDC", limit("1000", "800"));
      expect(r.status === "exceeded" && r.remainingUsd).toBe("200");
    });

    it("is 0, never negative, when exposure already passed the cap", () => {
      const r = checkInvestorLimit(B(1), "USDC", limit("1000", "1200"));
      expect(r.status === "exceeded" && r.remainingUsd).toBe("0");
    });

    it("is 0 when exposure is exactly the cap", () => {
      const r = checkInvestorLimit(B(1), "USDC", limit("1000", "1000"));
      expect(r.status === "exceeded" && r.remainingUsd).toBe("0");
    });
  });

  describe("a cap of 0", () => {
    it("means no cap, exactly as the server reads it", () => {
      expect(
        checkInvestorLimit(B("999999999999999999"), "USDC", limit("0", "5"))
      ).toEqual({
        status: "ok",
      });
    });
  });

  describe("when the answer cannot be known", () => {
    it("is unknown with no limit", () => {
      expect(checkInvestorLimit(B(1), "USDC", null)).toEqual({
        status: "unknown",
      });
      expect(checkInvestorLimit(B(1), "USDC", undefined)).toEqual({
        status: "unknown",
      });
    });

    it.each(["XLM", "EURC", "", "NOPE"])(
      "is unknown for %j, which the server converts with a rate the client does not hold",
      (currency) => {
        expect(
          checkInvestorLimit(B(10 ** 9), currency, limit("1", "0"))
        ).toEqual({
          status: "unknown",
        });
      }
    );

    it.each([
      ["a non-numeric cap", limit("lots", "0")],
      ["a negative cap", limit("-5", "0")],
      ["a decimal cap", limit("1.5", "0")],
      ["a non-numeric exposure", limit("1000", "some")],
      ["a negative exposure", limit("1000", "-1")],
    ])("is unknown for %s, rather than guessing", (_name, bad) => {
      expect(checkInvestorLimit(B(1), "USDC", bad)).toEqual({
        status: "unknown",
      });
    });
  });

  describe("currencies pegged 1:1 to USD", () => {
    it.each(["USDC", "USDT", "USD", "usdc", " USDC "])(
      "treats %j as par",
      (currency) => {
        expect(
          checkInvestorLimit(B(500), currency, limit("1000", "600")).status
        ).toBe("exceeded");
      }
    );
  });

  describe("large amounts", () => {
    it("compares exactly beyond Number.MAX_SAFE_INTEGER", () => {
      const cap = "9007199254740993000"; // above 2^53
      const current = "9007199254740992000";
      expect(
        checkInvestorLimit(B("1000"), "USDC", limit(cap, current)).status
      ).toBe("ok");
      expect(
        checkInvestorLimit(B("1001"), "USDC", limit(cap, current)).status
      ).toBe("exceeded");
    });
  });
});

describe("parseLimitFromDetails", () => {
  it("reads the numbers the 429 EXPOSURE_CAP_EXCEEDED body carries", () => {
    expect(
      parseLimitFromDetails({
        currentExposureUsd: "1100000000000000",
        attemptedUsd: "500000000000000",
        capUsd: "1500000000000000",
        investor: "GAX",
      })
    ).toEqual({
      capUsd: "1500000000000000",
      currentExposureUsd: "1100000000000000",
    });
  });

  it.each([
    ["undefined", undefined],
    ["null", null],
    ["a string", "capUsd"],
    ["missing the cap", { currentExposureUsd: "1" }],
    ["missing the exposure", { capUsd: "1" }],
    ["a non-numeric cap", { capUsd: "x", currentExposureUsd: "1" }],
    ["a number instead of a string", { capUsd: 1, currentExposureUsd: 1 }],
  ])("returns null for %s", (_name, details) => {
    expect(parseLimitFromDetails(details)).toBeNull();
  });
});

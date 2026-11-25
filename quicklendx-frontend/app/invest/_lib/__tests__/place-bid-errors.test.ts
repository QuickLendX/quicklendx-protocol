/**
 * Tests for mapping place-bid failures to form feedback (#3176).
 *
 * The errors are built the way `ApiClient.handleAxiosError` builds them (same
 * classes, same context shape), so this exercises the real contract between the
 * client and the form.
 *
 * Run with: npx jest app/invest/_lib/__tests__/place-bid-errors.test.ts
 */

import {
  AuthenticationError,
  AuthorizationError,
  NetworkError,
  SystemError,
  ValidationError,
} from "@/app/lib/errors";
import { describePlaceBidError, formatUsdMicro } from "../place-bid-errors";

const ctx = (additionalData?: Record<string, unknown>) => ({
  action: "POST",
  additionalData: { url: "/bids", ...additionalData },
});

/** A 429, as ApiClient builds it: NetworkError with the server code and retryAfter. */
const tooMany = (
  code: string | undefined,
  data: Record<string, unknown> = {}
) =>
  new NetworkError(
    "Too many requests",
    ctx({ retryAfter: undefined, ...data }),
    0,
    code
  );

describe("formatUsdMicro", () => {
  it.each([
    ["1500000000000", "1,500,000"],
    ["1500000", "1.5"],
    ["1", "0.000001"],
    ["0", "0"],
    ["1234567", "1.234567"],
    ["999999", "0.999999"],
  ])("formats %s micro-USD as %s", (input, expected) => {
    expect(formatUsdMicro(input)).toBe(expected);
  });
});

describe("401: authentication", () => {
  it.each([
    "MISSING_SIGNATURE",
    "INVALID_SIGNATURE",
    "EXPIRED_SIGNATURE",
    "REPLAY_DETECTED",
  ])("explains a signature rejection (%s) and is not ambiguous", (code) => {
    const failure = describePlaceBidError(
      new AuthenticationError("nope", ctx(), code)
    );
    expect(failure.kind).toBe("auth");
    expect(failure.code).toBe(code);
    expect(failure.ambiguous).toBe(false);
    expect(failure.message).toMatch(/signature/i);
  });

  it.each([
    "UNAUTHORIZED",
    "INVALID_API_KEY",
    "INVALID_AUTH_FORMAT",
    undefined,
  ])("tells the investor to sign in for %s", (code) => {
    const failure = describePlaceBidError(
      new AuthenticationError("nope", ctx(), code)
    );
    expect(failure.kind).toBe("auth");
    expect(failure.message).toMatch(/sign(ed)? in/i);
    expect(failure.ambiguous).toBe(false);
  });
});

describe("403: authorization", () => {
  it("explains a key with no signing secret", () => {
    const failure = describePlaceBidError(
      new AuthorizationError("x", ctx(), "SIGNATURE_REQUIRED")
    );
    expect(failure.kind).toBe("forbidden");
    expect(failure.message).toMatch(/rotate the key/i);
    expect(failure.ambiguous).toBe(false);
  });

  it("gives a plain refusal for any other 403", () => {
    const failure = describePlaceBidError(
      new AuthorizationError("x", ctx(), "FORBIDDEN")
    );
    expect(failure.kind).toBe("forbidden");
    expect(failure.message).toMatch(/not permitted/i);
  });
});

describe("429: exposure cap", () => {
  const details = {
    currentExposureUsd: "1100000000000",
    attemptedUsd: "500000000000",
    capUsd: "1500000000000",
    investor: "GINVESTOR",
  };

  it("explains the limit with the numbers and remembers it", () => {
    const failure = describePlaceBidError(
      tooMany("EXPOSURE_CAP_EXCEEDED", { details })
    );

    expect(failure.kind).toBe("limit");
    expect(failure.ambiguous).toBe(false);
    expect(failure.learnedLimit).toEqual({
      capUsd: "1500000000000",
      currentExposureUsd: "1100000000000",
    });
    expect(failure.message).toContain("1,500,000 USD");
    expect(failure.message).toContain("1,100,000 USD");
    expect(failure.message).toContain("400,000 USD remains");
  });

  it("says 0 remains when exposure is already at or over the cap", () => {
    const failure = describePlaceBidError(
      tooMany("EXPOSURE_CAP_EXCEEDED", {
        details: { ...details, currentExposureUsd: "1600000000000" },
      })
    );
    expect(failure.message).toContain("0 USD remains");
  });

  it("still reports the limit refusal when the body carried no numbers", () => {
    const failure = describePlaceBidError(tooMany("EXPOSURE_CAP_EXCEEDED"));
    expect(failure.kind).toBe("limit");
    expect(failure.learnedLimit).toBeUndefined();
    expect(failure.message).toMatch(/over your exposure limit/i);
  });
});

describe("429: rate limiting", () => {
  it("reads Retry-After in seconds", () => {
    const failure = describePlaceBidError(
      tooMany("RATE_LIMITED", { retryAfter: "30" })
    );
    expect(failure.kind).toBe("rate-limit");
    expect(failure.retryAfterSeconds).toBe(30);
    expect(failure.message).toMatch(/30 seconds/);
    expect(failure.ambiguous).toBe(false);
  });

  it("uses the singular for one second", () => {
    const failure = describePlaceBidError(
      tooMany("RATE_LIMITED", { retryAfter: "1" })
    );
    expect(failure.message).toMatch(/1 second\./);
  });

  it("reads Retry-After as an HTTP date", () => {
    const inTenSeconds = new Date(Date.now() + 10_000).toUTCString();
    const failure = describePlaceBidError(
      tooMany("RATE_LIMITED", { retryAfter: inTenSeconds })
    );
    expect(failure.retryAfterSeconds).toBeGreaterThanOrEqual(8);
    expect(failure.retryAfterSeconds).toBeLessThanOrEqual(11);
  });

  it("copes with no Retry-After at all", () => {
    const failure = describePlaceBidError(tooMany(undefined));
    expect(failure.kind).toBe("rate-limit");
    expect(failure.retryAfterSeconds).toBeUndefined();
    expect(failure.message).toMatch(/wait a moment/i);
  });

  it.each(["soon", "-5", ""])(
    "ignores an unusable Retry-After (%j)",
    (retryAfter) => {
      const failure = describePlaceBidError(
        tooMany("RATE_LIMITED", { retryAfter })
      );
      expect(failure.retryAfterSeconds).toBeUndefined();
    }
  );
});

describe("400 / 422: validation", () => {
  it.each([
    [
      "Investor GABC already has an active bid on this invoice",
      /already have an active bid/i,
    ],
    [
      "Cannot place bid on invoice with status Funded. Only Verified invoices accept bids.",
      /not open for bids/i,
    ],
    ["Invoice not found: 0xabc", /could not be found/i],
  ])("translates the server's refusal %j", (serverMessage, expected) => {
    const failure = describePlaceBidError(
      new ValidationError(serverMessage, ctx(), "INVALID_BID")
    );
    expect(failure.kind).toBe("validation");
    expect(failure.message).toMatch(expected);
    expect(failure.ambiguous).toBe(false);
  });

  it("ties an expected-return refusal to that field", () => {
    const failure = describePlaceBidError(
      new ValidationError(
        "Expected return must be greater than or equal to bid amount",
        ctx(),
        "INVALID_BID"
      )
    );
    expect(failure.fieldErrors).toEqual({
      expectedReturn:
        "Expected return must be greater than or equal to bid amount",
    });
  });

  it("ties a bid-amount refusal to that field", () => {
    const failure = describePlaceBidError(
      new ValidationError("Bid amount must be at least 1", ctx(), "INVALID_BID")
    );
    expect(failure.fieldErrors).toEqual({
      amount: "Bid amount must be at least 1",
    });
  });

  it("maps schema issues to form fields when the server sends them", () => {
    const failure = describePlaceBidError(
      new ValidationError(
        "Validation failed",
        ctx({
          details: [
            {
              field: "bid_amount",
              message: "Must be a positive numeric value as string",
              code: "invalid_string",
            },
            {
              field: "expiration_timestamp",
              message: "Too small",
              code: "too_small",
            },
            {
              field: "currency",
              message: "ignored: not a form field",
              code: "x",
            },
          ],
        }),
        "VALIDATION_ERROR"
      )
    );
    expect(failure.fieldErrors).toEqual({
      amount: "Must be a positive numeric value as string",
      expiresAt: "Too small",
    });
    expect(failure.message).toMatch(/highlighted fields/i);
  });

  it("falls back to the server's message when there are no usable details", () => {
    const failure = describePlaceBidError(
      new ValidationError("Validation failed", ctx(), "VALIDATION_ERROR")
    );
    expect(failure.fieldErrors).toBeUndefined();
    expect(failure.message).toBe("Validation failed");
  });

  it("keeps the first complaint when a field is reported twice", () => {
    const failure = describePlaceBidError(
      new ValidationError(
        "Validation failed",
        ctx({
          details: [
            { field: "bid_amount", message: "first", code: "a" },
            { field: "bid_amount", message: "second", code: "b" },
          ],
        })
      )
    );
    expect(failure.fieldErrors).toEqual({ amount: "first" });
  });
});

describe("outcome unknown: the server may have processed the request", () => {
  it("treats a timeout as ambiguous, never as a plain failure", () => {
    const failure = describePlaceBidError(
      new NetworkError(
        "Request timed out. Please try again.",
        ctx({ timeout: 10000 }),
        0
      )
    );
    expect(failure.kind).toBe("network");
    expect(failure.ambiguous).toBe(true);
    expect(failure.message).toMatch(/check the ranked bids/i);
  });

  it("treats a dropped connection as ambiguous", () => {
    const failure = describePlaceBidError(
      new NetworkError("Network connection failed.", ctx())
    );
    expect(failure.ambiguous).toBe(true);
  });

  it("treats a 5xx as ambiguous", () => {
    const failure = describePlaceBidError(
      new SystemError("Server error", ctx({ status: 500 }))
    );
    expect(failure.kind).toBe("server");
    expect(failure.ambiguous).toBe(true);
    expect(failure.message).toMatch(/check the ranked bids/i);
  });

  it("surfaces an unreadable success reply with its own message, and as ambiguous", () => {
    const failure = describePlaceBidError(
      new SystemError(
        "The bid may have been placed, but the server's reply was not understood.",
        ctx(),
        "BID_RESPONSE_INVALID"
      )
    );
    expect(failure.code).toBe("BID_RESPONSE_INVALID");
    expect(failure.ambiguous).toBe(true);
    expect(failure.message).toMatch(/may have been placed/i);
  });

  it.each([
    ["a plain Error", new Error("boom")],
    ["a string", "boom"],
    ["undefined", undefined],
    ["null", null],
  ])("treats %s as an ambiguous server failure", (_name, thrown) => {
    const failure = describePlaceBidError(thrown);
    expect(failure.kind).toBe("server");
    expect(failure.ambiguous).toBe(true);
  });
});

describe("what is and is not ambiguous", () => {
  it.each([
    ["401", new AuthenticationError("x", ctx(), "UNAUTHORIZED")],
    ["403", new AuthorizationError("x", ctx())],
    ["400", new ValidationError("x", ctx(), "INVALID_BID")],
    ["429 limit", tooMany("EXPOSURE_CAP_EXCEEDED")],
    ["429 rate limit", tooMany("RATE_LIMITED", { retryAfter: "5" })],
  ])("%s is a definite refusal: nothing was recorded", (_name, error) => {
    expect(describePlaceBidError(error).ambiguous).toBe(false);
  });
});

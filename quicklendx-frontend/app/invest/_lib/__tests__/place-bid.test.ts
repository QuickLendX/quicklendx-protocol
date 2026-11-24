/**
 * Tests for the place-bid logic (#3176).
 *
 * Run with: npx jest app/invest/_lib/__tests__/place-bid.test.ts
 */

import {
  MIN_EXPIRY_LEAD_SECONDS,
  compareBids,
  fromDatetimeLocal,
  mergePlacedBid,
  parseWholeNumber,
  previewRank,
  rankBids,
  toDatetimeLocal,
  validatePlaceBid,
  type PlaceBidContext,
  type PlaceBidFormValues,
} from "../place-bid";
import type { Bid } from "../view-model";

const NOW = 1_770_000_000; // unix seconds, minute-aligned
const ctx: PlaceBidContext = {
  invoiceId: "0xabc123",
  invoiceAmount: "1000000",
  nowSeconds: NOW,
};

/** A valid form: expiry a day out. */
function values(
  overrides: Partial<PlaceBidFormValues> = {}
): PlaceBidFormValues {
  return {
    amount: "1000",
    expectedReturn: "1050",
    expiresAt: toDatetimeLocal(NOW + 86_400),
    ...overrides,
  };
}

function bid(overrides: Partial<Bid>): Bid {
  return {
    bid_id: "0x01",
    invoice_id: "0xabc123",
    investor: "GINVESTOR",
    bid_amount: "1000",
    expected_return: "1050",
    timestamp: 1000,
    status: "Placed",
    expiration_timestamp: NOW + 86_400,
    ...overrides,
  };
}

describe("parseWholeNumber", () => {
  it.each([
    ["0", BigInt(0)],
    ["1000", BigInt(1000)],
    ["  42  ", BigInt(42)],
    ["007", BigInt(7)],
    [
      "170141183460469231731687303715884105727",
      BigInt("170141183460469231731687303715884105727"),
    ],
  ])("reads %j", (input, expected) => {
    expect(parseWholeNumber(input)).toBe(expected);
  });

  it.each([
    "",
    " ",
    "1.5",
    "1,000",
    "1 000",
    "-5",
    "+5",
    "1e3",
    "0x10",
    "abc",
    "١٢٣",
  ])("refuses %j", (input) => {
    expect(parseWholeNumber(input)).toBeNull();
  });
});

describe("datetime-local helpers", () => {
  it("round-trips to the minute", () => {
    expect(fromDatetimeLocal(toDatetimeLocal(NOW + 86_400))).toBe(NOW + 86_400);
  });

  it("formats as YYYY-MM-DDTHH:mm", () => {
    expect(toDatetimeLocal(NOW)).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/);
  });

  it.each(["", "   ", "not a date", "2026-13-45T99:99"])(
    "rejects %j",
    (input) => {
      expect(fromDatetimeLocal(input)).toBeNull();
    }
  );
});

describe("validatePlaceBid: the exact payload", () => {
  it("builds exactly the four fields the API requires, as the right types", () => {
    const result = validatePlaceBid(values(), ctx);
    expect(result.ok).toBe(true);
    if (!result.ok) return;

    expect(result.payload).toEqual({
      invoice_id: "0xabc123",
      bid_amount: "1000",
      expected_return: "1050",
      expiration_timestamp: NOW + 86_400,
    });
    expect(Object.keys(result.payload).sort()).toEqual([
      "bid_amount",
      "expected_return",
      "expiration_timestamp",
      "invoice_id",
    ]);
    expect(typeof result.payload.bid_amount).toBe("string");
    expect(typeof result.payload.expected_return).toBe("string");
    expect(Number.isInteger(result.payload.expiration_timestamp)).toBe(true);
  });

  it("normalises amounts to plain digit strings, matching the API's /^[0-9]+$/", () => {
    const result = validatePlaceBid(
      values({ amount: " 0100 ", expectedReturn: "000150" }),
      ctx
    );
    expect(result.ok && result.payload.bid_amount).toBe("100");
    expect(result.ok && result.payload.expected_return).toBe("150");
    if (result.ok) {
      expect(result.payload.bid_amount).toMatch(/^[0-9]+$/);
      expect(result.payload.expected_return).toMatch(/^[0-9]+$/);
    }
  });

  it("accepts a hex invoice id and an inv_ ULID, the two the schema allows", () => {
    expect(
      validatePlaceBid(values(), { ...ctx, invoiceId: "0xDEADbeef" }).ok
    ).toBe(true);
    expect(
      validatePlaceBid(values(), {
        ...ctx,
        invoiceId: "inv_01ARZ3NDEKTSV4RRFFQ69G5FAV",
      }).ok
    ).toBe(true);
  });

  it("refuses an invoice id the API would reject", () => {
    const result = validatePlaceBid(values(), {
      ...ctx,
      invoiceId: "not-an-id",
    });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.formError).toMatch(/ID/);
  });

  it("sends an amount equal to the invoice amount (the ceiling is inclusive)", () => {
    const result = validatePlaceBid(
      values({ amount: "1000000", expectedReturn: "1000000" }),
      ctx
    );
    expect(result.ok).toBe(true);
  });

  it("allows an expected return equal to the bid (zero profit)", () => {
    expect(validatePlaceBid(values({ expectedReturn: "1000" }), ctx).ok).toBe(
      true
    );
  });
});

describe("validatePlaceBid: bid amount", () => {
  const cases: [string, string, RegExp][] = [
    ["empty", "", /enter a bid amount/i],
    ["blank", "   ", /enter a bid amount/i],
    ["decimal", "10.5", /whole numbers/i],
    ["comma", "1,000", /whole numbers/i],
    ["negative", "-5", /whole numbers/i],
    ["text", "abc", /whole numbers/i],
    ["zero", "0", /at least 1/i],
    ["over the invoice", "1000001", /cannot exceed the invoice amount/i],
    ["beyond i128", "170141183460469231731687303715884105728", /too large/i],
  ];

  it.each(cases)("rejects %s", (_name, amount, message) => {
    const result = validatePlaceBid(values({ amount }), ctx);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.errors.amount).toMatch(message);
  });

  it("names the invoice amount in the over-the-ceiling message", () => {
    const result = validatePlaceBid(values({ amount: "2000000" }), ctx);
    if (!result.ok) expect(result.errors.amount).toContain("1,000,000");
  });
});

describe("validatePlaceBid: expected return", () => {
  it("rejects an empty value", () => {
    const result = validatePlaceBid(values({ expectedReturn: "" }), ctx);
    expect(!result.ok && result.errors.expectedReturn).toMatch(
      /enter the expected return/i
    );
  });

  it("rejects a non-integer value", () => {
    const result = validatePlaceBid(values({ expectedReturn: "10.5" }), ctx);
    expect(!result.ok && result.errors.expectedReturn).toMatch(
      /whole numbers/i
    );
  });

  it("rejects an expected return below the bid, as the server does", () => {
    const result = validatePlaceBid(values({ expectedReturn: "999" }), ctx);
    expect(!result.ok && result.errors.expectedReturn).toMatch(
      /at least the bid/i
    );
  });

  it("does not compare against a bid that is itself invalid", () => {
    const result = validatePlaceBid(
      values({ amount: "abc", expectedReturn: "1" }),
      ctx
    );
    expect(!result.ok && result.errors.amount).toBeTruthy();
    expect(!result.ok && result.errors.expectedReturn).toBeUndefined();
  });
});

describe("validatePlaceBid: expiry", () => {
  it("rejects an empty expiry", () => {
    const result = validatePlaceBid(values({ expiresAt: "" }), ctx);
    expect(!result.ok && result.errors.expiresAt).toMatch(/choose when/i);
  });

  it("rejects something that is not a date", () => {
    const result = validatePlaceBid(values({ expiresAt: "tomorrow" }), ctx);
    expect(!result.ok && result.errors.expiresAt).toMatch(/valid date/i);
  });

  it("rejects an expiry in the past", () => {
    const result = validatePlaceBid(
      values({ expiresAt: toDatetimeLocal(NOW - 3600) }),
      ctx
    );
    expect(!result.ok && result.errors.expiresAt).toMatch(/at least a minute/i);
  });

  it("rejects an expiry less than a minute away", () => {
    const result = validatePlaceBid(
      values({
        expiresAt: toDatetimeLocal(NOW + MIN_EXPIRY_LEAD_SECONDS - 60),
      }),
      ctx
    );
    expect(result.ok).toBe(false);
  });

  it("accepts an expiry exactly a minute away", () => {
    const result = validatePlaceBid(
      values({ expiresAt: toDatetimeLocal(NOW + MIN_EXPIRY_LEAD_SECONDS) }),
      ctx
    );
    expect(result.ok).toBe(true);
  });
});

describe("validatePlaceBid: reporting every problem at once", () => {
  it("returns an error for each bad field, not just the first", () => {
    const result = validatePlaceBid(
      { amount: "", expectedReturn: "", expiresAt: "" },
      ctx
    );
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(Object.keys(result.errors).sort()).toEqual([
        "amount",
        "expectedReturn",
        "expiresAt",
      ]);
    }
  });
});

describe("compareBids: the server's ranking order", () => {
  const make = (id: string, amount: string, ret: string, timestamp = 1000) => ({
    bid_id: id,
    bid_amount: amount,
    expected_return: ret,
    timestamp,
  });

  it("ranks the higher profit first, whatever the amounts", () => {
    // profit 200 beats profit 100 even though the other bid is much larger
    const small = make("a", "1000", "1200");
    const large = make("b", "9000", "9100");
    expect(rankBids([large, small]).map((b) => b.bid_id)).toEqual(["a", "b"]);
  });

  it("breaks a profit tie by the higher expected return", () => {
    const a = make("a", "1000", "1100"); // profit 100, return 1100
    const b = make("b", "2000", "2100"); // profit 100, return 2100
    expect(rankBids([a, b]).map((x) => x.bid_id)).toEqual(["b", "a"]);
  });

  it("puts the newer of two otherwise identical bids first", () => {
    // Equal profit and equal expected return force equal amounts, so the amount
    // rule is never the tiebreaker; identical bids fall through to the timestamp.
    const a = make("a", "1000", "1100", 1);
    const b = make("b", "1000", "1100", 2);
    expect(rankBids([a, b]).map((x) => x.bid_id)).toEqual(["b", "a"]);
  });

  it("puts the newer bid first on a full tie, then orders by bid_id ascending", () => {
    const older = make("z", "1000", "1100", 5);
    const newer = make("y", "1000", "1100", 9);
    expect(rankBids([older, newer])[0].bid_id).toBe("y");

    const x = make("x", "1000", "1100", 5);
    const w = make("w", "1000", "1100", 5);
    expect(rankBids([x, w]).map((b) => b.bid_id)).toEqual(["w", "x"]);
  });

  it("does not change the order of an already ranked list, and does not mutate", () => {
    const ranked = [
      make("a", "1", "300"),
      make("b", "1", "200"),
      make("c", "1", "100"),
    ];
    const input = [...ranked].reverse();
    const copy = [...input];
    expect(rankBids(input).map((b) => b.bid_id)).toEqual(["a", "b", "c"]);
    expect(input).toEqual(copy);
  });

  it("compares amounts beyond Number's safe range exactly", () => {
    const huge = make(
      "a",
      "170141183460469231731687303715884105000",
      "170141183460469231731687303715884105700"
    );
    const hugeLess = make(
      "b",
      "170141183460469231731687303715884105000",
      "170141183460469231731687303715884105699"
    );
    expect(compareBids(huge, hugeLess)).toBeLessThan(0);
  });
});

describe("previewRank", () => {
  const existing = [
    bid({ bid_id: "a", bid_amount: "1000", expected_return: "1300" }), // profit 300
    bid({ bid_id: "b", bid_amount: "1000", expected_return: "1200" }), // profit 200
    bid({ bid_id: "c", bid_amount: "1000", expected_return: "1100" }), // profit 100
  ];
  const candidate = (b: number, r: number) => ({
    bid_amount: BigInt(b),
    expected_return: BigInt(r),
  });

  it("is first when nothing beats it", () => {
    expect(previewRank(existing, candidate(1000, 1500))).toEqual({
      rank: 1,
      total: 4,
    });
  });

  it("is last when everything beats it", () => {
    expect(previewRank(existing, candidate(1000, 1050))).toEqual({
      rank: 4,
      total: 4,
    });
  });

  it("lands between bids", () => {
    expect(previewRank(existing, candidate(1000, 1250))).toEqual({
      rank: 2,
      total: 4,
    });
    expect(previewRank(existing, candidate(1000, 1150))).toEqual({
      rank: 3,
      total: 4,
    });
  });

  it("ranks ahead of an exact tie, because the new bid is the newest", () => {
    expect(previewRank(existing, candidate(1000, 1200))).toEqual({
      rank: 2,
      total: 4,
    });
  });

  it("is rank 1 of 1 on an invoice with no bids", () => {
    expect(previewRank([], candidate(500, 600))).toEqual({ rank: 1, total: 1 });
  });

  it("only counts Placed bids, the set the ranked endpoint returns", () => {
    const mixed = [
      ...existing,
      bid({ bid_id: "d", expected_return: "9999", status: "Withdrawn" }),
      bid({ bid_id: "e", expected_return: "9999", status: "Expired" }),
    ];
    expect(previewRank(mixed, candidate(1000, 1500))).toEqual({
      rank: 1,
      total: 4,
    });
  });

  it("ignores a bid whose amounts cannot be read", () => {
    const broken = [...existing, bid({ bid_id: "x", bid_amount: "nope" })];
    expect(previewRank(broken, candidate(1000, 1250))).toEqual({
      rank: 2,
      total: 4,
    });
  });

  it("agrees with where rankBids puts the bid once it is on the list", () => {
    for (const ret of [1050, 1100, 1150, 1200, 1250, 1300, 1500]) {
      const preview = previewRank(existing, candidate(1000, ret));
      const placed = bid({
        bid_id: "new",
        bid_amount: "1000",
        expected_return: String(ret),
        timestamp: 99_999, // newest
      });
      const position =
        rankBids([...existing, placed]).findIndex((b) => b.bid_id === "new") +
        1;
      expect(preview.rank).toBe(position);
    }
  });
});

describe("mergePlacedBid", () => {
  const server = [
    bid({ bid_id: "a", expected_return: "1300" }),
    bid({ bid_id: "b", expected_return: "1100" }),
  ];

  it("adds a bid the server list does not contain yet, in ranked position", () => {
    const placed = bid({
      bid_id: "new",
      expected_return: "1200",
      timestamp: 2000,
    });
    expect(mergePlacedBid(server, placed).map((b) => b.bid_id)).toEqual([
      "a",
      "new",
      "b",
    ]);
  });

  it("does not duplicate a bid the server already returned", () => {
    const placed = bid({ bid_id: "b", expected_return: "1100" });
    const merged = mergePlacedBid(server, placed);
    expect(merged.filter((b) => b.bid_id === "b")).toHaveLength(1);
    expect(merged).toHaveLength(2);
  });

  it("works on an empty list", () => {
    const placed = bid({ bid_id: "only" });
    expect(mergePlacedBid([], placed)).toEqual([placed]);
  });
});

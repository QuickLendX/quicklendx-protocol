import type { Settlement } from "../data";
import { selectRows, sumAmounts } from "../selectRows";

function settlement(
  id: string,
  overrides: Partial<Settlement> = {}
): Settlement {
  return {
    id,
    invoice_id: `invoice-${id}`,
    amount: "100",
    payer: "GADDRESS",
    recipient: "GOTHER",
    timestamp: 100,
    status: "Paid",
    ...overrides,
  };
}

function select(
  all: Settlement[],
  overrides: Partial<Parameters<typeof selectRows>[1]> = {}
) {
  return selectRows(all, {
    address: "GADDRESS",
    status: "All",
    range: "all",
    sort: "date_desc",
    page: 1,
    ...overrides,
  });
}

describe("selectRows", () => {
  it("filters by status and wallet address", () => {
    const rows = [
      settlement("paid", { status: "Paid" }),
      settlement("pending-payer", { status: "Pending" }),
      settlement("pending-recipient", {
        payer: "GOTHER",
        recipient: "GADDRESS",
        status: "Pending",
      }),
      settlement("outside", { payer: "GTHIRD", recipient: "GFOURTH" }),
    ];

    expect(select(rows, { status: "Pending" }).map((row) => row.id)).toEqual([
      "pending-payer",
      "pending-recipient",
    ]);
  });

  it("includes exact wallet matches in payer or recipient", () => {
    const rows = [
      settlement("payer-match", { payer: "GADDRESS", recipient: "GOTHER" }),
      settlement("recipient-match", { payer: "GOTHER", recipient: "GADDRESS" }),
      settlement("outside", { payer: "GTHIRD", recipient: "GFOURTH" }),
    ];

    expect(select(rows).map((row) => row.id)).toEqual([
      "payer-match",
      "recipient-match",
    ]);
  });

  it("keeps sort ties stable by timestamp", () => {
    const rows = [
      settlement("older-tied", { amount: "500", timestamp: 100 }),
      settlement("newer-tied", { amount: "500", timestamp: 300 }),
      settlement("smaller", { amount: "100", timestamp: 200 }),
    ];

    expect(select(rows, { sort: "amount_desc" }).map((row) => row.id)).toEqual([
      "newer-tied",
      "older-tied",
      "smaller",
    ]);
    expect(select(rows, { sort: "date_asc" }).map((row) => row.id)).toEqual([
      "older-tied",
      "smaller",
      "newer-tied",
    ]);
  });

  it("applies the date range and page limit", () => {
    const now = 100 * 24 * 60 * 60 * 1000;
    const rows = [
      settlement("recent", { timestamp: now / 1000 - 5 * 24 * 60 * 60 }),
      settlement("old", { timestamp: now / 1000 - 40 * 24 * 60 * 60 }),
    ];

    expect(
      select(rows, { range: "30d", now }).map((row) => row.id)
    ).toEqual(["recent"]);
  });

  it("returns an empty array for empty input", () => {
    expect(select([])).toEqual([]);
  });
});

describe("sumAmounts", () => {
  it("sums large integer amounts without precision loss", () => {
    expect(
      sumAmounts([
        settlement("one", { amount: "9007199254740993" }),
        settlement("two", { amount: "7" }),
      ])
    ).toBe(9007199254741000n);
  });
});

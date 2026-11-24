import {
  DisputesListResponseSchema,
  RankedBidsResponseSchema,
  SettlementsListResponseSchema,
  listInvoiceDisputes,
  listSettlements,
  type Dispute,
  type Settlement,
} from "../endpoints";

afterEach(() => {
  jest.restoreAllMocks();
});

describe("invoice detail API schemas", () => {
  it("parses ranked bids using integer-string amounts", () => {
    const response = RankedBidsResponseSchema.parse({
      data: [
        {
          bid_id: "bid-1",
          invoice_id: "invoice-1",
          investor: "GABC",
          bid_amount: "50000000000",
          expected_return: "52500000000",
          timestamp: 1710000000,
          status: "Placed",
          expiration_timestamp: 1711000000,
        },
      ],
    });

    expect(response.data[0].bid_amount).toBe("50000000000");
    expect(() =>
      RankedBidsResponseSchema.parse({
        data: [{ ...response.data[0], bid_amount: 50000000000 }],
      })
    ).toThrow();
  });

  it("parses paginated disputes, including optional version metadata", () => {
    const dispute: Dispute = {
      id: "dispute-1",
      invoice_id: "invoice-1",
      initiator: "GABC",
      reason: "Goods were not delivered",
      status: "UnderReview",
      created_at: 1710000000,
      contract_version: 1,
      event_schema_version: 1,
      indexed_at: "2026-06-23T17:00:00Z",
    };

    expect(
      DisputesListResponseSchema.parse({
        data: [dispute],
        next_cursor: null,
        has_more: false,
      })
    ).toEqual({ data: [dispute], next_cursor: null, has_more: false });
  });

  it("parses paginated settlements while preserving amounts as strings", () => {
    const settlement: Settlement = {
      id: "settlement-1",
      invoice_id: "invoice-1",
      amount: "50000000000",
      payer: "GPAYER",
      recipient: "GRECIPIENT",
      timestamp: 1710000000,
      status: "Processing",
    };

    const response = SettlementsListResponseSchema.parse({
      data: [settlement],
      next_cursor: "1710000000_settlement-1",
      has_more: true,
      freshness: {
        lastIndexedLedger: 1042857,
        indexLagSeconds: 10,
        lastUpdatedAt: "2026-06-23T17:00:00Z",
        cursor: "100000_0",
      },
    });

    expect(response.data[0].amount).toBe("50000000000");
    expect(() =>
      SettlementsListResponseSchema.parse({
        ...response,
        data: [{ ...settlement, amount: 50000000000 }],
      })
    ).toThrow();
  });
});

describe("invoice detail API pagination", () => {
  it("passes cursors and limits to dispute and settlement endpoints", async () => {
    const fetchMock = jest.spyOn(global, "fetch").mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        data: [],
        next_cursor: null,
        has_more: false,
      }),
    } as Response);

    await listInvoiceDisputes("invoice-1", { cursor: "dispute-cursor", limit: 5 });
    await listSettlements({
      invoice_id: "invoice-1",
      cursor: "settlement-cursor",
      limit: 10,
    });

    expect(String(fetchMock.mock.calls[0][0])).toMatch(
      /\/invoices\/invoice-1\/disputes\?cursor=dispute-cursor&limit=5$/
    );
    expect(String(fetchMock.mock.calls[1][0])).toMatch(
      /\/settlements\?invoice_id=invoice-1&cursor=settlement-cursor&limit=10$/
    );
  });
});

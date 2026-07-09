import { getPortfolio } from "../controllers/v1/portfolio";
import { getDisputes } from "../controllers/v1/disputes";
import { derivedTableStore } from "../services/replayService";
import { invoiceStore } from "../services/invoiceStore";
import { BidStatus, DisputeStatus, InvoiceStatus } from "../types/contract";

jest.mock("../services/replayService", () => ({
  derivedTableStore: {
    listBids: jest.fn(),
    listInvoices: jest.fn(),
    listDisputes: jest.fn(),
  },
}));

jest.mock("../services/invoiceStore", () => ({
  invoiceStore: {
    findInvoices: jest.fn(),
  },
}));

type MockResponse = {
  status: jest.Mock;
  json: jest.Mock;
  end: jest.Mock;
  setHeader: jest.Mock;
};

const makeResponse = (): MockResponse => {
  const res: MockResponse = {
    status: jest.fn(),
    json: jest.fn(),
    end: jest.fn(),
    setHeader: jest.fn(),
  };
  res.status.mockReturnValue(res);
  return res;
};

const next = jest.fn();

describe("portfolio persistence-backed controller", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    (derivedTableStore.listBids as jest.Mock).mockResolvedValue([]);
    (derivedTableStore.listInvoices as jest.Mock).mockResolvedValue([]);
    (invoiceStore.findInvoices as jest.Mock).mockReturnValue([]);
  });

  it("derives accepted investor positions from indexed bids and invoices", async () => {
    (derivedTableStore.listBids as jest.Mock).mockResolvedValue([
      {
        bid_id: "bid-accepted-paid",
        invoice_id: "invoice-paid",
        investor: "investor-a",
        bid_amount: "1000",
        expected_return: "1100",
        timestamp: 200,
        status: BidStatus.Accepted,
      },
      {
        bid_id: "bid-placed",
        invoice_id: "invoice-paid",
        investor: "investor-a",
        bid_amount: "500",
        expected_return: "550",
        timestamp: 300,
        status: BidStatus.Placed,
      },
      {
        bid_id: "bid-other-investor",
        invoice_id: "invoice-paid",
        investor: "investor-b",
        bid_amount: "700",
        expected_return: "770",
        timestamp: 400,
        status: BidStatus.Accepted,
      },
    ]);
    (derivedTableStore.listInvoices as jest.Mock).mockResolvedValue([
      { id: "invoice-paid", status: InvoiceStatus.Paid },
    ]);

    const req = {
      query: { investor: "investor-a" },
      headers: {},
    } as any;
    const res = makeResponse();

    await getPortfolio(req, res as any, next);

    expect(res.json).toHaveBeenCalledWith({
      data: [
        {
          id: "bid-accepted-paid",
          investor: "investor-a",
          invoice_id: "invoice-paid",
          invested_amount: "1000",
          expected_return: "1100",
          status: "Completed",
          invested_at: 200,
        },
      ],
      next_cursor: null,
      has_more: false,
    });
    expect(next).not.toHaveBeenCalled();
  });

  it("falls back to invoiceStore when indexed invoices are empty", async () => {
    (derivedTableStore.listBids as jest.Mock).mockResolvedValue([
      {
        bid_id: "bid-accepted-defaulted",
        invoice_id: "invoice-defaulted",
        investor: "investor-a",
        bid_amount: "1000",
        expected_return: "1100",
        timestamp: 100,
        status: BidStatus.Accepted,
      },
    ]);
    (invoiceStore.findInvoices as jest.Mock).mockReturnValue([
      { id: "invoice-defaulted", status: InvoiceStatus.Defaulted },
    ]);

    const req = {
      query: { investor: "investor-a" },
      headers: {},
    } as any;
    const res = makeResponse();

    await getPortfolio(req, res as any, next);

    expect(res.json.mock.calls[0][0].data[0].status).toBe("Defaulted");
  });

  it("returns an empty page for investors without accepted positions", async () => {
    (derivedTableStore.listBids as jest.Mock).mockResolvedValue([
      {
        bid_id: "bid-other",
        invoice_id: "invoice-1",
        investor: "investor-b",
        bid_amount: "1000",
        expected_return: "1100",
        timestamp: 100,
        status: BidStatus.Accepted,
      },
    ]);

    const req = {
      query: { investor: "investor-a" },
      headers: {},
    } as any;
    const res = makeResponse();

    await getPortfolio(req, res as any, next);

    expect(res.json).toHaveBeenCalledWith({
      data: [],
      next_cursor: null,
      has_more: false,
    });
  });

  it("preserves pagination over persistence-backed portfolio entries", async () => {
    (derivedTableStore.listBids as jest.Mock).mockResolvedValue([
      {
        bid_id: "bid-new",
        invoice_id: "invoice-1",
        investor: "investor-a",
        bid_amount: "1000",
        expected_return: "1100",
        timestamp: 300,
        status: BidStatus.Accepted,
      },
      {
        bid_id: "bid-old",
        invoice_id: "invoice-2",
        investor: "investor-a",
        bid_amount: "2000",
        expected_return: "2200",
        timestamp: 200,
        status: BidStatus.Accepted,
      },
    ]);

    const req = {
      query: { investor: "investor-a", limit: "1" },
      headers: {},
    } as any;
    const res = makeResponse();

    await getPortfolio(req, res as any, next);

    const body = res.json.mock.calls[0][0];
    expect(body.data).toHaveLength(1);
    expect(body.data[0].id).toBe("bid-new");
    expect(body.has_more).toBe(true);
    expect(body.next_cursor).toEqual(expect.any(String));
  });
});

describe("disputes persistence-backed controller", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    (derivedTableStore.listDisputes as jest.Mock).mockResolvedValue([]);
  });

  it("filters indexed disputes by invoice id", async () => {
    (derivedTableStore.listDisputes as jest.Mock).mockResolvedValue([
      {
        id: "dispute-1",
        invoice_id: "invoice-1",
        initiator: "buyer",
        reason: "late shipment",
        status: DisputeStatus.UnderReview,
        created_at: 200,
      },
      {
        id: "dispute-2",
        invoice_id: "invoice-2",
        initiator: "seller",
        reason: "payment issue",
        status: DisputeStatus.Resolved,
        created_at: 300,
      },
    ]);

    const req = {
      params: { id: "invoice-1" },
      query: {},
      headers: {},
    } as any;
    const res = makeResponse();

    await getDisputes(req, res as any, next);

    expect(res.json).toHaveBeenCalledWith({
      data: [
        {
          id: "dispute-1",
          invoice_id: "invoice-1",
          initiator: "buyer",
          reason: "late shipment",
          status: DisputeStatus.UnderReview,
          created_at: 200,
        },
      ],
      next_cursor: null,
      has_more: false,
    });
  });

  it("returns an empty dispute page when no indexed records match", async () => {
    const req = {
      params: { id: "invoice-missing" },
      query: {},
      headers: {},
    } as any;
    const res = makeResponse();

    await getDisputes(req, res as any, next);

    expect(res.json).toHaveBeenCalledWith({
      data: [],
      next_cursor: null,
      has_more: false,
    });
  });
});

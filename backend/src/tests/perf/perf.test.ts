/**
 * Performance regression tests for marketplace endpoints.
 *
 * Targets (p95) — measured against in-process supertest (includes HTTP stack overhead):
 *   - GET /api/v1/invoices        < 150ms
 *   - GET /api/v1/invoices/:id    < 150ms
 *   - GET /api/v1/bids            < 150ms
 *   - GET /api/v1/settlements     < 150ms
 *
 * Dataset: 100 invoices, 500 bids, 50 settlements (deterministic, no I/O).
 * Iterations: 200 per endpoint to get stable percentiles.
 */
import { describe, it, expect, beforeAll } from "@jest/globals";
import { seedInvoices, seedBids, seedSettlements } from "./seed";
import { measure } from "./harness";

// --- seed data ---
const INVOICES = seedInvoices(100);
const BIDS = seedBids(INVOICES, 5);
const SETTLEMENTS = seedSettlements(INVOICES);

// --- mock controllers before app is imported ---
jest.mock("../../controllers/v1/invoices", () => ({
  getInvoices: (req: any, res: any) => {
    const { business, status } = req.query;
    let result = INVOICES;
    if (business) result = result.filter((i) => i.business === business);
    if (status) result = result.filter((i) => i.status === status);
    res.json(result);
  },
  getInvoiceById: (req: any, res: any) => {
    const invoice = INVOICES.find((i) => i.id === req.params.id);
    if (!invoice)
      return res
        .status(404)
        .json({ error: { message: "Not found", code: "INVOICE_NOT_FOUND" } });
    res.json(invoice);
  },
}));

jest.mock("../../controllers/v1/bids", () => ({
  getBids: (req: any, res: any) => {
    const { invoice_id, investor } = req.query;
    let result = BIDS;
    if (invoice_id) result = result.filter((b) => b.invoice_id === invoice_id);
    if (investor) result = result.filter((b) => b.investor === investor);
    res.json(result);
  },
}));

jest.mock("../../controllers/v1/settlements", () => ({
  getSettlements: (req: any, res: any) => {
    const { invoice_id } = req.query;
    let result = SETTLEMENTS;
    if (invoice_id) result = result.filter((s) => s.invoice_id === invoice_id);
    res.json(result);
  },
  getSettlementById: (req: any, res: any) => {
    const s = SETTLEMENTS.find((s) => s.id === req.params.id);
    if (!s)
      return res.status(404).json({
        error: { message: "Not found", code: "SETTLEMENT_NOT_FOUND" },
      });
    res.json(s);
  },
}));

jest.mock("../../controllers/v1/disputes", () => ({
  getDisputes: (_req: any, res: any) => res.json([]),
}));

// import app after mocks are in place
import app from "../../app";

// p95 targets in milliseconds (calibrated for in-process supertest; allows for CI contention)
const TARGETS = {
  invoiceList: 150,
  invoiceDetail: 150,
  bidList: 150,
  settlementList: 150,
};

const ITERATIONS = 200;

describe("Performance: p95 latency targets", () => {
  let stats: Record<string, ReturnType<typeof measure> extends Promise<infer T> ? T : never>;

  beforeAll(async () => {
    const firstInvoiceId = INVOICES[0].id;
    // Run sequentially to avoid contention skewing percentiles
    const invoiceList = await measure(app, "/api/v1/invoices", ITERATIONS);
    const invoiceDetail = await measure(app, `/api/v1/invoices/${firstInvoiceId}`, ITERATIONS);
    const bidList = await measure(app, "/api/v1/bids", ITERATIONS);
    const settlementList = await measure(app, "/api/v1/settlements", ITERATIONS);

    stats = { invoiceList, invoiceDetail, bidList, settlementList };

    // Print summary for CI logs
    for (const [name, s] of Object.entries(stats)) {
      console.log(
        `[perf] ${name}: p50=${s.p50.toFixed(1)}ms p95=${s.p95.toFixed(1)}ms p99=${s.p99.toFixed(1)}ms (n=${s.samples})`
      );
    }
  }, 60_000);

  it("GET /api/v1/invoices p95 < 150ms", () => {
    expect(stats.invoiceList.p95).toBeLessThan(TARGETS.invoiceList);
  });

  it("GET /api/v1/invoices/:id p95 < 150ms", () => {
    expect(stats.invoiceDetail.p95).toBeLessThan(TARGETS.invoiceDetail);
  });

  it("GET /api/v1/bids p95 < 150ms", () => {
    expect(stats.bidList.p95).toBeLessThan(TARGETS.bidList);
  });

  it("GET /api/v1/settlements p95 < 150ms", () => {
    expect(stats.settlementList.p95).toBeLessThan(TARGETS.settlementList);
  });

  // Sanity: p99 should not be more than 10x p50 (catches complete hangs only)
  it("invoice list p99/p50 ratio < 10", () => {
    const { p50, p99 } = stats.invoiceList;
    expect(p99 / p50).toBeLessThan(10);
  });
});

import { describe, it, expect, beforeEach, vi } from "vitest";
import request from "supertest";
import express from "express";
import { getInvoices, getInvoiceById, MOCK_INVOICES } from "../controllers/v1/invoices";
import * as cacheHeaders from "../middleware/cache-headers";
import { invoiceStore } from "../services/invoiceStore";

const app = express();
app.use(express.json());
app.get("/invoices", getInvoices);
app.get("/invoices/:id", getInvoiceById);

app.use((err: any, req: any, res: any, next: any) => {
  res.status(500).json({ error: err.message });
});

vi.mock("../middleware/cache-headers", () => ({
  applyCacheHeaders: vi.fn().mockReturnValue(false),
  CC_SHORT: "short",
  CC_LONG: "long",
  CC_NO_STORE: "no-store",
  computeETag: vi.fn(),
  extractLastModified: vi.fn(),
  isNotModified: vi.fn(),
}));

vi.mock("../services/invoiceStore", () => ({
  invoiceStore: {
    findInvoices: vi.fn(),
    listPage: vi.fn(),
    findInvoiceById: vi.fn(),
    insertInvoice: vi.fn(),
    deleteAll: vi.fn(),
  },
}));

const KNOWN_ID = MOCK_INVOICES[0].id;
const KNOWN_BUSINESS = MOCK_INVOICES[0].business;
const UNKNOWN_ID = "inv_01ARZ3NDEKTSV4RRFFQ69G5FAW";

describe("invoices controller", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(cacheHeaders.applyCacheHeaders).mockReturnValue(false);
    vi.mocked(invoiceStore.listPage).mockImplementation(({ filters, limit }: any) => {
      const data = MOCK_INVOICES.filter((inv: any) => {
        if (filters?.business && inv.business !== filters.business) return false;
        if (filters?.status && inv.status !== filters.status) return false;
        return true;
      });
      return { data: data.slice(0, limit), next_cursor: null, has_more: false };
    });
    vi.mocked(invoiceStore.findInvoiceById).mockImplementation((id: string) =>
      MOCK_INVOICES.find((i: any) => i.id === id) || undefined
    );
  });

  describe("getInvoices", () => {
    it("should return invoices filtered by business", async () => {
      const res = await request(app).get(`/invoices?business=${KNOWN_BUSINESS}`);
      expect(res.status).toBe(200);
      expect(res.body.data).toHaveLength(1);
      expect(res.body.data[0].id).toBe(KNOWN_ID);
    });

    it("should return invoices filtered by status", async () => {
      const res = await request(app).get("/invoices?status=Pending");
      expect(res.status).toBe(200);
      expect(Array.isArray(res.body.data)).toBe(true);
    });

    it("should return empty list if no match", async () => {
      const res = await request(app).get("/invoices?business=unknown-business");
      expect(res.status).toBe(200);
      expect(res.body.data).toHaveLength(0);
    });

    it("should return all invoices with no filter", async () => {
      const res = await request(app).get("/invoices");
      expect(res.status).toBe(200);
      expect(res.body.data.length).toBeGreaterThanOrEqual(1);
    });

    it("should return 304 if cache headers match", async () => {
      vi.mocked(cacheHeaders.applyCacheHeaders).mockReturnValue(true);
      const res = await request(app).get("/invoices");
      expect(res.status).toBe(304);
    });
  });

  describe("getInvoiceById", () => {
    it("should return the invoice if found", async () => {
      const res = await request(app).get(`/invoices/${KNOWN_ID}`);
      expect(res.status).toBe(200);
      expect(res.body.id).toBe(KNOWN_ID);
    });

    it("should return 404 if not found", async () => {
      const res = await request(app).get(`/invoices/${UNKNOWN_ID}`);
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("INVOICE_NOT_FOUND");
    });

    it("should return 304 if cache headers match", async () => {
      vi.mocked(cacheHeaders.applyCacheHeaders).mockReturnValue(true);
      const res = await request(app).get(`/invoices/${KNOWN_ID}`);
      expect(res.status).toBe(304);
    });
  });
});

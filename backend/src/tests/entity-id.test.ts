import supertest from "supertest";
import express from "express";
import { ENTITY_PREFIXES, assertBidId } from "../lib/entityId";
import { getBestBid, getTopBids } from "../controllers/v1/bids";
import { getSettlementById } from "../controllers/v1/settlements";
import { downloadExport } from "../controllers/v1/exports";
import { errorHandler } from "../middleware/error-handler";

function createTestApp() {
  const app = express();
  app.use(express.json());

  app.get("/bids/:invoiceId/best", getBestBid);
  app.get("/bids/:invoiceId/ranked", getTopBids);
  app.get("/settlements/:id", getSettlementById);
  app.get("/exports/download/:token", downloadExport);

  app.get("/bids/check/:bidId", (req, res, next) => {
    try {
      assertBidId(req.params.bidId);
      res.json({ valid: true });
    } catch (err) {
      next(err);
    }
  });

  app.use(errorHandler);
  return app;
}

let request: supertest.SuperTest<supertest.Test>;

beforeAll(() => {
  request = supertest(createTestApp());
});

const VALID_ULID = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const VALID_ULID_LOWER = "01arz3ndektsv4rrffq69g5fav";
const INVALID_CROCKFORD = "I".repeat(26);
const LONG_INVOICE = `${ENTITY_PREFIXES.INVOICE}${"1".repeat(26)}${"2".repeat(970)}`;
const LONG_SETTLEMENT = `${ENTITY_PREFIXES.SETTLEMENT}${"1".repeat(26)}${"2".repeat(970)}`;
const LONG_EXPORT = `${ENTITY_PREFIXES.EXPORT_TOKEN}${"1".repeat(26)}${"2".repeat(970)}`;
const LONG_BID = `${ENTITY_PREFIXES.BID}${"1".repeat(26)}${"2".repeat(970)}`;
const SQL_INVOICE = encodeURIComponent(`${ENTITY_PREFIXES.INVOICE}' OR '1'='1`);
const SQL_SETTLEMENT = encodeURIComponent(`${ENTITY_PREFIXES.SETTLEMENT}' OR '1'='1`);
const SQL_EXPORT = encodeURIComponent(`${ENTITY_PREFIXES.EXPORT_TOKEN}' OR '1'='1`);
const SQL_BID = encodeURIComponent(`${ENTITY_PREFIXES.BID}' OR '1'='1`);

describe("assertInvoiceId", () => {
  describe("GET /bids/:invoiceId/best", () => {
    it("accepts a valid ULID (not 400)", async () => {
      const res = await request.get(`/bids/${ENTITY_PREFIXES.INVOICE}${VALID_ULID}/best`);
      expect(res.status).not.toBe(400);
    });
    it("accepts lowercase (not 400)", async () => {
      const res = await request.get(`/bids/${ENTITY_PREFIXES.INVOICE}${VALID_ULID_LOWER}/best`);
      expect(res.status).not.toBe(400);
    });
    it("rejects wrong prefix", async () => {
      const res = await request.get(`/bids/${ENTITY_PREFIXES.BID}${VALID_ULID}/best`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects '123'", async () => {
      const res = await request.get(`/bids/${ENTITY_PREFIXES.INVOICE}123/best`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects '1'", async () => {
      const res = await request.get(`/bids/${ENTITY_PREFIXES.INVOICE}1/best`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects invalid Crockford chars", async () => {
      const res = await request.get(`/bids/${ENTITY_PREFIXES.INVOICE}${INVALID_CROCKFORD}/best`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects 1000 chars", async () => {
      const res = await request.get(`/bids/${LONG_INVOICE}/best`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects SQL injection", async () => {
      const res = await request.get(`/bids/${SQL_INVOICE}/best`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
  });

  describe("GET /bids/:invoiceId/ranked", () => {
    it("accepts a valid ULID", async () => {
      const res = await request.get(`/bids/${ENTITY_PREFIXES.INVOICE}${VALID_ULID}/ranked`);
      expect(res.status).toBe(200);
    });
    it("rejects wrong prefix", async () => {
      const res = await request.get(`/bids/${ENTITY_PREFIXES.BID}${VALID_ULID}/ranked`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
  });
});

describe("assertBidId", () => {
  describe("GET /bids/check/:bidId", () => {
    it("accepts a valid ULID", async () => {
      const res = await request.get(`/bids/check/${ENTITY_PREFIXES.BID}${VALID_ULID}`);
      expect(res.status).toBe(200);
      expect(res.body.valid).toBe(true);
    });
    it("accepts lowercase", async () => {
      const res = await request.get(`/bids/check/${ENTITY_PREFIXES.BID}${VALID_ULID_LOWER}`);
      expect(res.status).toBe(200);
      expect(res.body.valid).toBe(true);
    });
    it("rejects wrong prefix", async () => {
      const res = await request.get(`/bids/check/${ENTITY_PREFIXES.INVOICE}${VALID_ULID}`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects '123'", async () => {
      const res = await request.get(`/bids/check/${ENTITY_PREFIXES.BID}123`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects '1'", async () => {
      const res = await request.get("/bids/check/1");
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects invalid Crockford chars", async () => {
      const res = await request.get(`/bids/check/${ENTITY_PREFIXES.BID}${INVALID_CROCKFORD}`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects 1000 chars", async () => {
      const res = await request.get(`/bids/check/${LONG_BID}`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects SQL injection", async () => {
      const res = await request.get(`/bids/check/${SQL_BID}`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
  });
});

describe("assertSettlementId", () => {
  describe("GET /settlements/:id", () => {
    it("accepts a valid ULID (not 400)", async () => {
      const res = await request.get(`/settlements/${ENTITY_PREFIXES.SETTLEMENT}${VALID_ULID}`);
      expect(res.status).not.toBe(400);
    });
    it("accepts lowercase (not 400)", async () => {
      const res = await request.get(`/settlements/${ENTITY_PREFIXES.SETTLEMENT}${VALID_ULID_LOWER}`);
      expect(res.status).not.toBe(400);
    });
    it("rejects wrong prefix", async () => {
      const res = await request.get(`/settlements/${ENTITY_PREFIXES.INVOICE}${VALID_ULID}`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects '123'", async () => {
      const res = await request.get(`/settlements/${ENTITY_PREFIXES.SETTLEMENT}123`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects '1'", async () => {
      const res = await request.get(`/settlements/${ENTITY_PREFIXES.SETTLEMENT}1`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects invalid Crockford chars", async () => {
      const res = await request.get(`/settlements/${ENTITY_PREFIXES.SETTLEMENT}${INVALID_CROCKFORD}`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects 1000 chars", async () => {
      const res = await request.get(`/settlements/${LONG_SETTLEMENT}`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects SQL injection", async () => {
      const res = await request.get(`/settlements/${SQL_SETTLEMENT}`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
  });
});

describe("assertExportToken", () => {
  describe("GET /exports/download/:token", () => {
    it("accepts a valid ULID (not 400)", async () => {
      const res = await request.get(`/exports/download/${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID}`);
      expect(res.status).not.toBe(400);
    });
    it("accepts lowercase (not 400)", async () => {
      const res = await request.get(`/exports/download/${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID_LOWER}`);
      expect(res.status).not.toBe(400);
    });
    it("rejects wrong prefix", async () => {
      const res = await request.get(`/exports/download/${ENTITY_PREFIXES.SETTLEMENT}${VALID_ULID}`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects '123'", async () => {
      const res = await request.get(`/exports/download/${ENTITY_PREFIXES.EXPORT_TOKEN}123`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects '1'", async () => {
      const res = await request.get(`/exports/download/${ENTITY_PREFIXES.EXPORT_TOKEN}1`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects invalid Crockford chars", async () => {
      const res = await request.get(`/exports/download/${ENTITY_PREFIXES.EXPORT_TOKEN}${INVALID_CROCKFORD}`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects 1000 chars", async () => {
      const res = await request.get(`/exports/download/${LONG_EXPORT}`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
    it("rejects SQL injection", async () => {
      const res = await request.get(`/exports/download/${SQL_EXPORT}`);
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_ENTITY_ID");
    });
  });
});

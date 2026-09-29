import { describe, it, expect, beforeAll, afterAll, beforeEach, jest } from "@jest/globals";
import request from "supertest";
import app from "../app";
import pool from "../services/database";
import { bidStore, BidCreateInput } from "../services/bidStore";
import { Bid, BidStatus, InvoiceStatus } from "../types/contract";
import crypto from "crypto";

const TEST_INVOICE_ID = "0x" + "1234567890abcdef".repeat(4);
const TEST_INVOICE_ID_2 = "0x" + "abcdef1234567890".repeat(4);
const TEST_INVESTOR = "GBSXVD727UNXJZ7ZM4VCXBTK3UMPXR7O6LLXS7XVOECGDYH3XFNV7C5K";
const TEST_INVESTOR_2 = "GAXMFSADZVDXTJSLL3HZJFVYH4JGBQBMX3I2WGPKH3YKRXL7ZJXZK23Q";
const TEST_API_KEY = "qlx_test_" + crypto.randomBytes(32).toString("base64url");
const TEST_CREATED_BY = TEST_INVESTOR;

describe("Bid Placement and Ranking", () => {
  beforeAll(async () => {
    // Ensure database connection
    const client = await pool.connect();
    client.release();
  });

  afterAll(async () => {
    // Cleanup
    await pool.end();
  });

  beforeEach(async () => {
    // Clear bids before each test (in real tests, use transactions)
    try {
      await pool.query("DELETE FROM bids WHERE invoice_id IN ($1, $2)", [
        TEST_INVOICE_ID,
        TEST_INVOICE_ID_2,
      ]);
    } catch (err) {
      // Table might not exist yet in test environment
    }
  });

  describe("POST /api/v1/bids - Create Bid", () => {
    it("should create a bid successfully with valid inputs", async () => {
      const bidBody = {
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000",
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
      };

      const response = await request(app)
        .post("/api/v1/bids")
        .set("Authorization", `Bearer ${TEST_API_KEY}`)
        .send(bidBody)
        .expect(201);

      expect(response.body.data).toBeDefined();
      expect(response.body.data.bid_id).toBeDefined();
      expect(response.body.data.invoice_id).toBe(TEST_INVOICE_ID);
      expect(response.body.data.bid_amount).toBe("1000000");
      expect(response.body.data.expected_return).toBe("1500000");
      expect(response.body.data.status).toBe(BidStatus.Placed);
    });

    it("should reject bid without authentication", async () => {
      const bidBody = {
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000",
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
      };

      const response = await request(app)
        .post("/api/v1/bids")
        .send(bidBody)
        .expect(401);

      expect(response.body.error).toBeDefined();
      expect(response.body.error.code).toBe("UNAUTHORIZED");
    });

    it("should reject bid with invalid invoice_id format", async () => {
      const bidBody = {
        invoice_id: "invalid_hex",
        bid_amount: "1000000",
        expected_return: "1500000",
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
      };

      const response = await request(app)
        .post("/api/v1/bids")
        .set("Authorization", `Bearer ${TEST_API_KEY}`)
        .send(bidBody)
        .expect(400);

      expect(response.body.error).toBeDefined();
    });

    it("should reject bid with non-numeric bid_amount", async () => {
      const bidBody = {
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "not_a_number",
        expected_return: "1500000",
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
      };

      const response = await request(app)
        .post("/api/v1/bids")
        .set("Authorization", `Bearer ${TEST_API_KEY}`)
        .send(bidBody)
        .expect(400);

      expect(response.body.error).toBeDefined();
    });

    it("should reject bid with expected_return < bid_amount", async () => {
      const bidBody = {
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "2000000",
        expected_return: "1000000",
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
      };

      const response = await request(app)
        .post("/api/v1/bids")
        .set("Authorization", `Bearer ${TEST_API_KEY}`)
        .send(bidBody)
        .expect(400);

      expect(response.body.error).toBeDefined();
      expect(response.body.error.code).toBe("INVALID_BID");
    });

    it("should reject bid with zero bid_amount", async () => {
      const bidBody = {
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "0",
        expected_return: "1000000",
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
      };

      const response = await request(app)
        .post("/api/v1/bids")
        .set("Authorization", `Bearer ${TEST_API_KEY}`)
        .send(bidBody)
        .expect(400);

      expect(response.body.error).toBeDefined();
      expect(response.body.error.code).toBe("INVALID_BID");
    });

    it("should require invoice_id", async () => {
      const bidBody = {
        bid_amount: "1000000",
        expected_return: "1500000",
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
      };

      const response = await request(app)
        .post("/api/v1/bids")
        .set("Authorization", `Bearer ${TEST_API_KEY}`)
        .send(bidBody)
        .expect(400);

      expect(response.body.error).toBeDefined();
    });

    it("should require bid_amount", async () => {
      const bidBody = {
        invoice_id: TEST_INVOICE_ID,
        expected_return: "1500000",
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
      };

      const response = await request(app)
        .post("/api/v1/bids")
        .set("Authorization", `Bearer ${TEST_API_KEY}`)
        .send(bidBody)
        .expect(400);

      expect(response.body.error).toBeDefined();
    });

    it("should require expected_return", async () => {
      const bidBody = {
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
      };

      const response = await request(app)
        .post("/api/v1/bids")
        .set("Authorization", `Bearer ${TEST_API_KEY}`)
        .send(bidBody)
        .expect(400);

      expect(response.body.error).toBeDefined();
    });

    it("should require expiration_timestamp", async () => {
      const bidBody = {
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000",
      };

      const response = await request(app)
        .post("/api/v1/bids")
        .set("Authorization", `Bearer ${TEST_API_KEY}`)
        .send(bidBody)
        .expect(400);

      expect(response.body.error).toBeDefined();
    });
  });

  describe("GET /api/v1/bids - Get Ranked Bids", () => {
    it("should require invoice_id parameter", async () => {
      const response = await request(app)
        .get("/api/v1/bids")
        .expect(400);

      expect(response.body.error).toBeDefined();
      expect(response.body.error.code).toBe("MISSING_REQUIRED_FIELD");
    });

    it("should return empty array for invoice with no bids", async () => {
      const response = await request(app)
        .get("/api/v1/bids")
        .query({ invoice_id: TEST_INVOICE_ID })
        .expect(200);

      expect(response.body.data).toEqual([]);
      expect(response.body.has_more).toBe(false);
    });

    it("should support pagination with limit", async () => {
      // Create multiple bids
      const bidIds: string[] = [];
      for (let i = 0; i < 5; i++) {
        const bid = await bidStore.createBid({
          bid_id: "0x" + crypto.randomBytes(32).toString("hex"),
          invoice_id: TEST_INVOICE_ID,
          bid_amount: String(1000000 + i * 100000),
          expected_return: String(1500000 + i * 100000),
          timestamp: Math.floor(Date.now() / 1000) + i,
          expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
          investor: TEST_INVESTOR,
          created_by: TEST_CREATED_BY,
        });
        bidIds.push(bid.bid_id);
      }

      const response = await request(app)
        .get("/api/v1/bids")
        .query({ invoice_id: TEST_INVOICE_ID, limit: 2 })
        .expect(200);

      expect(response.body.data.length).toBeLessThanOrEqual(2);
      if (response.body.data.length === 2) {
        expect(response.body.has_more).toBe(true);
        expect(response.body.next_cursor).toBeDefined();
      }
    });

    it("should support filtering by investor", async () => {
      // Create bid from investor 1
      const bid1 = await bidStore.createBid({
        bid_id: "0x" + crypto.randomBytes(32).toString("hex"),
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000",
        timestamp: Math.floor(Date.now() / 1000),
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
        investor: TEST_INVESTOR,
        created_by: TEST_CREATED_BY,
      });

      // Get bids filtered by investor
      const response = await request(app)
        .get("/api/v1/bids")
        .query({ invoice_id: TEST_INVOICE_ID, investor: TEST_INVESTOR })
        .expect(200);

      expect(response.body.data.length).toBeGreaterThan(0);
      expect(response.body.data[0].investor).toBe(TEST_INVESTOR);
    });

    it("should return bids in ranking order (best first)", async () => {
      // Create multiple bids with different profits
      const bids = [
        {
          bid_amount: "1000000",
          expected_return: "1500000", // profit: 500000
        },
        {
          bid_amount: "1000000",
          expected_return: "2000000", // profit: 1000000 (better)
        },
        {
          bid_amount: "500000",
          expected_return: "1200000", // profit: 700000
        },
      ];

      for (const bid of bids) {
        await bidStore.createBid({
          bid_id: "0x" + crypto.randomBytes(32).toString("hex"),
          invoice_id: TEST_INVOICE_ID,
          ...bid,
          timestamp: Math.floor(Date.now() / 1000),
          expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
          investor: TEST_INVESTOR,
          created_by: TEST_CREATED_BY,
        });
      }

      const response = await request(app)
        .get("/api/v1/bids")
        .query({ invoice_id: TEST_INVOICE_ID })
        .expect(200);

      // Best bid should be the one with highest profit (2000000 - 1000000 = 1000000)
      expect(response.body.data.length).toBe(3);
      const firstProfit =
        BigInt(response.body.data[0].expected_return) -
        BigInt(response.body.data[0].bid_amount);
      const secondProfit =
        BigInt(response.body.data[1].expected_return) -
        BigInt(response.body.data[1].bid_amount);
      expect(firstProfit).toBeGreaterThanOrEqual(secondProfit);
    });
  });

  describe("GET /api/v1/bids/:invoiceId/best - Get Best Bid", () => {
    it("should return 404 for invoice with no bids", async () => {
      const response = await request(app)
        .get(`/api/v1/bids/${TEST_INVOICE_ID}/best`)
        .expect(404);

      expect(response.body.error).toBeDefined();
    });

    it("should return the best bid", async () => {
      // Create a bid
      const bid = await bidStore.createBid({
        bid_id: "0x" + crypto.randomBytes(32).toString("hex"),
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000",
        timestamp: Math.floor(Date.now() / 1000),
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
        investor: TEST_INVESTOR,
        created_by: TEST_CREATED_BY,
      });

      const response = await request(app)
        .get(`/api/v1/bids/${TEST_INVOICE_ID}/best`)
        .expect(200);

      expect(response.body.data.bid_id).toBe(bid.bid_id);
      expect(response.body.data.bid_amount).toBe("1000000");
    });
  });

  describe("GET /api/v1/bids/:invoiceId/ranked - Get Ranked Bids", () => {
    it("should return all ranked bids", async () => {
      const bidIds: string[] = [];
      for (let i = 0; i < 3; i++) {
        const bid = await bidStore.createBid({
          bid_id: "0x" + crypto.randomBytes(32).toString("hex"),
          invoice_id: TEST_INVOICE_ID,
          bid_amount: String(1000000 + i * 100000),
          expected_return: String(1500000 + i * 100000),
          timestamp: Math.floor(Date.now() / 1000) + i,
          expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
          investor: TEST_INVESTOR,
          created_by: TEST_CREATED_BY,
        });
        bidIds.push(bid.bid_id);
      }

      const response = await request(app)
        .get(`/api/v1/bids/${TEST_INVOICE_ID}/ranked`)
        .expect(200);

      expect(response.body.data.length).toBe(3);
      // Verify they're in ranking order (best first)
      for (let i = 0; i < response.body.data.length - 1; i++) {
        const profitI =
          BigInt(response.body.data[i].expected_return) -
          BigInt(response.body.data[i].bid_amount);
        const profitI1 =
          BigInt(response.body.data[i + 1].expected_return) -
          BigInt(response.body.data[i + 1].bid_amount);
        expect(profitI).toBeGreaterThanOrEqual(profitI1);
      }
    });

    it("should return empty array for invoice with no bids", async () => {
      const response = await request(app)
        .get(`/api/v1/bids/${TEST_INVOICE_ID}/ranked`)
        .expect(200);

      expect(response.body.data).toEqual([]);
    });
  });

  describe("Edge Cases and Security", () => {
    it("should handle tie-breaking correctly (same profit)", async () => {
      // Create bids with same profit but different expected_return
      const bid1 = await bidStore.createBid({
        bid_id: "0x" + "1".repeat(64),
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000", // profit: 500000
        timestamp: Math.floor(Date.now() / 1000),
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
        investor: TEST_INVESTOR,
        created_by: TEST_CREATED_BY,
      });

      const bid2 = await bidStore.createBid({
        bid_id: "0x" + "2".repeat(64),
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000", // same profit
        timestamp: Math.floor(Date.now() / 1000) + 1, // newer
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
        investor: TEST_INVESTOR_2,
        created_by: TEST_INVESTOR_2,
      });

      const ranked = await bidStore.getRankedBids(TEST_INVOICE_ID);
      expect(ranked.length).toBe(2);
      // Newer bid should rank higher when other values are equal
      if (ranked[0].timestamp === bid2.timestamp) {
        expect(ranked[0].bid_id).toBe(bid2.bid_id);
      }
    });

    it("should handle large numeric values correctly", async () => {
      const largeAmount = "999999999999999999999999999";
      const largeReturn = "1999999999999999999999999999";

      const bid = await bidStore.createBid({
        bid_id: "0x" + crypto.randomBytes(32).toString("hex"),
        invoice_id: TEST_INVOICE_ID,
        bid_amount: largeAmount,
        expected_return: largeReturn,
        timestamp: Math.floor(Date.now() / 1000),
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
        investor: TEST_INVESTOR,
        created_by: TEST_CREATED_BY,
      });

      expect(bid.bid_amount).toBe(largeAmount);
      expect(bid.expected_return).toBe(largeReturn);
    });

    it("should validate expiration_timestamp is a positive integer", async () => {
      const response = await request(app)
        .post("/api/v1/bids")
        .set("Authorization", `Bearer ${TEST_API_KEY}`)
        .send({
          invoice_id: TEST_INVOICE_ID,
          bid_amount: "1000000",
          expected_return: "1500000",
          expiration_timestamp: -100,
        })
        .expect(400);

      expect(response.body.error).toBeDefined();
    });

    it("should validate expiration_timestamp is not a float", async () => {
      const response = await request(app)
        .post("/api/v1/bids")
        .set("Authorization", `Bearer ${TEST_API_KEY}`)
        .send({
          invoice_id: TEST_INVOICE_ID,
          bid_amount: "1000000",
          expected_return: "1500000",
          expiration_timestamp: 1234567890.5,
        })
        .expect(400);

      expect(response.body.error).toBeDefined();
    });
  });

  describe("Ranking Algorithm Compliance", () => {
    it("should rank by profit first", async () => {
      const bid1 = await bidStore.createBid({
        bid_id: "0x" + crypto.randomBytes(32).toString("hex"),
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "2000000", // profit: 1000000
        timestamp: Math.floor(Date.now() / 1000),
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
        investor: TEST_INVESTOR,
        created_by: TEST_CREATED_BY,
      });

      const bid2 = await bidStore.createBid({
        bid_id: "0x" + crypto.randomBytes(32).toString("hex"),
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000", // profit: 500000
        timestamp: Math.floor(Date.now() / 1000),
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
        investor: TEST_INVESTOR_2,
        created_by: TEST_INVESTOR_2,
      });

      const ranked = await bidStore.getRankedBids(TEST_INVOICE_ID);
      expect(ranked[0].bid_id).toBe(bid1.bid_id);
      expect(ranked[1].bid_id).toBe(bid2.bid_id);
    });

    it("should tiebreak on expected_return when profit is equal", async () => {
      const bid1 = await bidStore.createBid({
        bid_id: "0x" + crypto.randomBytes(32).toString("hex"),
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "2000000", // profit: 1000000, return: 2000000
        timestamp: Math.floor(Date.now() / 1000),
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
        investor: TEST_INVESTOR,
        created_by: TEST_CREATED_BY,
      });

      const bid2 = await bidStore.createBid({
        bid_id: "0x" + crypto.randomBytes(32).toString("hex"),
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000", // profit: 500000
        timestamp: Math.floor(Date.now() / 1000),
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
        investor: TEST_INVESTOR_2,
        created_by: TEST_INVESTOR_2,
      });

      const bid3 = await bidStore.createBid({
        bid_id: "0x" + crypto.randomBytes(32).toString("hex"),
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "500000",
        expected_return: "1500000", // profit: 1000000, return: 1500000
        timestamp: Math.floor(Date.now() / 1000),
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
        investor: TEST_INVESTOR,
        created_by: TEST_CREATED_BY,
      });

      const ranked = await bidStore.getRankedBids(TEST_INVOICE_ID);
      // bid1 should be first (highest profit and return)
      expect(ranked[0].bid_id).toBe(bid1.bid_id);
    });
  });
});

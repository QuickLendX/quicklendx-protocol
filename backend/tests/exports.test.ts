import request from "supertest";
import app from "../src/app";
import { exportService, ExportFormat } from "../src/services/exportService";
import { auditLogService } from "../src/services/auditLogService";

describe("Export Endpoints", () => {
  // The invoice mock data has business: "GDVLRH4G4...7Y"
  const userId = "GDVLRH4G4...7Y";
  const authHeader = `Bearer ${userId}`;

  beforeEach(() => {
    auditLogService.clear();
  });

  // ---------------------------------------------------------------------------
  // POST /api/v1/exports/generate
  // ---------------------------------------------------------------------------
  describe("POST /api/v1/exports/generate", () => {
    it("should return 401 if no Authorization header", async () => {
      const res = await request(app)
        .post("/api/v1/exports/generate")
        .set("Content-Type", "application/json")
        .send({});
      expect(res.status).toBe(401);
      expect(res.body.error.code).toBe("UNAUTHORIZED");
    });

    it("should return 401 if Authorization header is malformed", async () => {
      const res = await request(app)
        .post("/api/v1/exports/generate")
        .set("Authorization", "Token bad-format")
        .set("Content-Type", "application/json")
        .send({});
      expect(res.status).toBe(401);
      expect(res.body.error.code).toBe("UNAUTHORIZED");
    });

    it("should generate a signed link for JSON format (default)", async () => {
      const res = await request(app)
        .post("/api/v1/exports/generate")
        .set("Authorization", authHeader)
        .set("Content-Type", "application/json")
        .send({});

      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.download_url).toContain("/api/v1/exports/download/");
      expect(res.body.expires_in).toBe("1 hour");
    });

    it("should generate a signed link for CSV format", async () => {
      const res = await request(app)
        .post("/api/v1/exports/generate?format=csv")
        .set("Authorization", authHeader)
        .set("Content-Type", "application/json")
        .send({});

      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.download_url).toContain("/api/v1/exports/download/");
    });

    it("should return 400 for invalid format", async () => {
      const res = await request(app)
        .post("/api/v1/exports/generate?format=pdf")
        .set("Authorization", authHeader)
        .set("Content-Type", "application/json")
        .send({});

      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_FORMAT");
    });

    it("should record a data_export_requested audit log entry", async () => {
      await request(app)
        .post("/api/v1/exports/generate")
        .set("Authorization", authHeader)
        .set("Content-Type", "application/json")
        .send({});

      const entries = auditLogService.listEntries();
      expect(entries.some((e) => e.action === "data_export_requested")).toBe(true);
    });
  });

  // ---------------------------------------------------------------------------
  // GET /api/v1/exports/download/:token
  // ---------------------------------------------------------------------------
  describe("GET /api/v1/exports/download/:token", () => {
    it("should return 401 for a missing/invalid token", async () => {
      const res = await request(app).get("/api/v1/exports/download/invalid-token");
      expect(res.status).toBe(401);
      expect(res.body.error.code).toBe("INVALID_TOKEN");
    });

    it("should return 401 for a tampered base64 token", async () => {
      const res = await request(app).get("/api/v1/exports/download/dGFtcGVyZWQ=");
      expect(res.status).toBe(401);
    });

    it("should serve JSON file with correct structure for valid token", async () => {
      const token = exportService.generateSignedToken(userId, ExportFormat.JSON);
      const res = await request(app).get(`/api/v1/exports/download/${token}`);

      expect(res.status).toBe(200);
      expect(res.header["content-type"]).toContain("application/json");
      expect(res.header["content-disposition"]).toContain("attachment");
      expect(res.header["content-disposition"]).toContain(".json");
      // User's invoice should be present
      expect(res.body).toHaveProperty("invoices");
      expect(res.body.invoices.length).toBeGreaterThan(0);
      expect(res.body.invoices[0].business).toBe(userId);
    });

    it("should serve CSV file for valid CSV token", async () => {
      const token = exportService.generateSignedToken(userId, ExportFormat.CSV);
      const res = await request(app).get(`/api/v1/exports/download/${token}`);

      expect(res.status).toBe(200);
      expect(res.header["content-type"]).toContain("text/csv");
      expect(res.header["content-disposition"]).toContain("attachment");
      expect(res.header["content-disposition"]).toContain(".csv");
      // CSV content sections should be present
      expect(res.text).toContain("--- INVOICES ---");
      expect(res.text).toContain("--- BIDS ---");
      expect(res.text).toContain("--- SETTLEMENTS ---");
    });

    it("should record a data_export_downloaded audit log entry", async () => {
      const token = exportService.generateSignedToken(userId, ExportFormat.JSON);
      await request(app).get(`/api/v1/exports/download/${token}`);

      const entries = auditLogService.listEntries();
      expect(entries.some((e) => e.action === "data_export_downloaded")).toBe(true);
    });

    it("should prevent IDOR – other user's token returns only their data", async () => {
      const otherUser = "GOTHERUSER999...";
      const token = exportService.generateSignedToken(otherUser, ExportFormat.JSON);
      const res = await request(app).get(`/api/v1/exports/download/${token}`);

      expect(res.status).toBe(200);
      // otherUser has no invoices, bids, or settlements in the mock data
      expect(res.body.invoices.length).toBe(0);
      expect(res.body.bids.length).toBe(0);
      expect(res.body.settlements.length).toBe(0);
    });

    it("should not expose another user's invoices when requesting own export", async () => {
      const token = exportService.generateSignedToken(userId, ExportFormat.JSON);
      const res = await request(app).get(`/api/v1/exports/download/${token}`);

      // All returned invoices must belong to this user
      for (const invoice of res.body.invoices) {
        expect(invoice.business).toBe(userId);
      }
    });
  });

  // ---------------------------------------------------------------------------
  // ExportService unit tests
  // ---------------------------------------------------------------------------
  describe("ExportService", () => {
    it("generateSignedToken produces a verifiable token", () => {
      const token = exportService.generateSignedToken(userId, ExportFormat.JSON);
      const result = exportService.validateToken(token);
      expect(result).not.toBeNull();
      expect(result?.userId).toBe(userId);
      expect(result?.format).toBe(ExportFormat.JSON);
    });

    it("validateToken returns null for tampered payload", () => {
      const token = exportService.generateSignedToken(userId, ExportFormat.JSON);
      // Corrupt the token
      const corrupted = token.slice(0, -5) + "XXXXX";
      expect(exportService.validateToken(corrupted)).toBeNull();
    });

    it("validateToken returns null for random garbage", () => {
      expect(exportService.validateToken("not-a-token")).toBeNull();
    });

    it("getUserData returns only records scoped to the userId", async () => {
      const data = await exportService.getUserData(userId);
      for (const inv of data.invoices) {
        expect(inv.business).toBe(userId);
      }
      for (const bid of data.bids) {
        expect(bid.investor).toBe(userId);
      }
    });

    it("formatData JSON round-trips correctly", async () => {
      const data = await exportService.getUserData(userId);
      const json = exportService.formatData(data, ExportFormat.JSON);
      const parsed = JSON.parse(json);
      expect(parsed).toEqual(data);
    });

    it("formatData CSV contains section headers", async () => {
      const data = await exportService.getUserData(userId);
      const csv = exportService.formatData(data, ExportFormat.CSV);
      expect(csv).toContain("--- INVOICES ---");
      expect(csv).toContain("--- BIDS ---");
      expect(csv).toContain("--- SETTLEMENTS ---");
    });

    it("formatData CSV includes invoice rows when data exists", async () => {
      const data = await exportService.getUserData(userId);
      const csv = exportService.formatData(data, ExportFormat.CSV);
      // The header row for invoices
      expect(csv).toContain("ID,Amount,Currency,Status,Due Date");
    });

    it("formatData CSV shows 'No bids found' when user has no bids", async () => {
      const data = await exportService.getUserData(userId);
      // GDVLRH4G4...7Y has no bids in mock data
      const csv = exportService.formatData(data, ExportFormat.CSV);
      expect(csv).toContain("No bids found");
    });

    it("formatData CSV shows 'No settlements found' when user has no settlements", async () => {
      const data = await exportService.getUserData(userId);
      const csv = exportService.formatData(data, ExportFormat.CSV);
      expect(csv).toContain("No settlements found");
    });

    it("getUserData returns settlements where user is payer or recipient", async () => {
      // The mock settlement has payer "GA...PAYER" and recipient "GB...RECIP"
      const payerData = await exportService.getUserData("GA...PAYER");
      expect(payerData.settlements.length).toBeGreaterThan(0);
      expect(payerData.settlements[0].payer).toBe("GA...PAYER");

      const recipientData = await exportService.getUserData("GB...RECIP");
      expect(recipientData.settlements.length).toBeGreaterThan(0);
      expect(recipientData.settlements[0].recipient).toBe("GB...RECIP");
    });

    it("formatData CSV includes settlement rows when data exists", async () => {
      const payerData = await exportService.getUserData("GA...PAYER");
      const csv = exportService.formatData(payerData, ExportFormat.CSV);
      expect(csv).toContain("ID,Invoice ID,Amount,Status,Timestamp");
    });

    it("validateToken returns null for an expired token", async () => {
      // Build an already-expired token manually
      const payload = JSON.stringify({ userId, format: ExportFormat.JSON, expiresAt: Date.now() - 1000 });
      const crypto = await import("crypto");
      const secret = "development-only-export-secret-32-chars";
      const signature = crypto.createHmac("sha256", secret).update(payload).digest("hex");
      const expired = Buffer.from(JSON.stringify({ payload, signature })).toString("base64");
      expect(exportService.validateToken(expired)).toBeNull();
    });
  });

  // ---------------------------------------------------------------------------
  // userAuth middleware direct unit tests
  // ---------------------------------------------------------------------------
  describe("requireUserAuth edge cases", () => {
    it("should return 401 if Authorization header is absent", async () => {
      const res = await request(app)
        .post("/api/v1/exports/generate")
        .set("Content-Type", "application/json")
        .send({});
      expect(res.status).toBe(401);
      expect(res.body.error.code).toBe("UNAUTHORIZED");
    });

    it("should return 401 if Authorization scheme is not Bearer", async () => {
      const res = await request(app)
        .post("/api/v1/exports/generate")
        .set("Authorization", "Basic dXNlcjpwYXNz")
        .set("Content-Type", "application/json")
        .send({});
      expect(res.status).toBe(401);
    });

    it("should return 401 for 'Bearer ' with empty token (hits empty-userId branch)", async () => {
      // Inject middleware directly to hit the empty userId path
      const { requireUserAuth } = await import("../src/middleware/userAuth");
      const mockReq: any = {
        header: (name: string) => (name === "authorization" ? "Bearer " : undefined),
      };
      const mockJson = jest.fn();
      const mockStatus = jest.fn().mockReturnValue({ json: mockJson });
      const mockRes: any = { status: mockStatus };
      const mockNext = jest.fn();

      requireUserAuth(mockReq, mockRes, mockNext);

      expect(mockStatus).toHaveBeenCalledWith(401);
      expect(mockJson).toHaveBeenCalledWith(
        expect.objectContaining({ error: expect.objectContaining({ code: "UNAUTHORIZED" }) })
      );
      expect(mockNext).not.toHaveBeenCalled();
    });

    it("should accept any non-empty Bearer token as userId", async () => {
      const res = await request(app)
        .post("/api/v1/exports/generate")
        .set("Authorization", "Bearer some-user-id")
        .set("Content-Type", "application/json")
        .send({});
      expect(res.status).toBe(200);
    });
  });

  // ---------------------------------------------------------------------------
  // getUser helper direct unit test
  // ---------------------------------------------------------------------------
  describe("getUser helper", () => {
    it("should throw if called without requireUserAuth middleware", async () => {
      const { getUser } = await import("../src/middleware/userAuth");
      const bare: any = { header: () => undefined }; // plain request, no user ctx
      expect(() => getUser(bare)).toThrow("User context not available");
    });
  });
});


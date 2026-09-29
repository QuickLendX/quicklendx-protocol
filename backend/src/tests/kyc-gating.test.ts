import request from "supertest";
import express from "express";
import { createInvoice } from "../controllers/v1/invoices";
import * as kycService from "../services/kycService";

// Setup express app for testing the controller directly
const app = express();
app.use(express.json());
app.post("/api/v1/invoices", createInvoice);

// Mock the kycService
jest.mock("../services/kycService", () => {
  return {
    ...jest.requireActual("../services/kycService"),
    getKycStatus: jest.fn(),
  };
});

describe("KYC Gating on POST /api/v1/invoices", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("should accept request when KYC is verified and not expired", async () => {
    (kycService.getKycStatus as jest.Mock).mockReturnValue({
      status: "verified",
      verifiedAt: Date.now() - 1000 * 60 * 60 * 24 * 30, // 30 days ago
    });

    const res = await request(app).post("/api/v1/invoices").send({
      business: "bus_123",
      amount: "1000",
    });

    expect(res.status).toBe(201);
    expect(res.body.success).toBe(true);
  });

  it("should reject request when KYC is pending", async () => {
    (kycService.getKycStatus as jest.Mock).mockReturnValue({
      status: "pending",
    });

    const res = await request(app).post("/api/v1/invoices").send({
      business: "bus_123",
      amount: "1000",
    });

    expect(res.status).toBe(403);
    expect(res.body.error.code).toBe("KYC_NOT_VERIFIED");
  });

  it("should reject request when KYC is rejected", async () => {
    (kycService.getKycStatus as jest.Mock).mockReturnValue({
      status: "rejected",
    });

    const res = await request(app).post("/api/v1/invoices").send({
      business: "bus_123",
      amount: "1000",
    });

    expect(res.status).toBe(403);
    expect(res.body.error.code).toBe("KYC_NOT_VERIFIED");
  });

  it("should reject request when KYC record is missing", async () => {
    (kycService.getKycStatus as jest.Mock).mockReturnValue(null);

    const res = await request(app).post("/api/v1/invoices").send({
      business: "bus_123",
      amount: "1000",
    });

    expect(res.status).toBe(403);
    expect(res.body.error.code).toBe("KYC_NOT_VERIFIED");
  });

  it("should reject request when KYC is verified but expired (older than 12 months)", async () => {
    const THIRTEEN_MONTHS_MS = 13 * 30 * 24 * 60 * 60 * 1000;
    (kycService.getKycStatus as jest.Mock).mockReturnValue({
      status: "verified",
      verifiedAt: Date.now() - THIRTEEN_MONTHS_MS,
    });

    const res = await request(app).post("/api/v1/invoices").send({
      business: "bus_123",
      amount: "1000",
    });

    expect(res.status).toBe(403);
    expect(res.body.error.code).toBe("KYC_NOT_VERIFIED");
  });

  it("should return the same error message for all rejection cases to avoid leaking business existence", async () => {
    (kycService.getKycStatus as jest.Mock).mockReturnValueOnce({ status: "rejected" });
    const res1 = await request(app).post("/api/v1/invoices").send({ business: "bus_exists" });

    (kycService.getKycStatus as jest.Mock).mockReturnValueOnce(null);
    const res2 = await request(app).post("/api/v1/invoices").send({ business: "bus_missing" });

    expect(res1.body.error.message).toBe(res2.body.error.message);
    expect(res1.body.error.code).toBe(res2.body.error.code);
    expect(res1.status).toBe(res2.status);
  });
});

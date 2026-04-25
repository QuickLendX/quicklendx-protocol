import request from "supertest";
import app from "../index";
import { statusService, StatusService } from "../services/statusService";
import { ADMIN_KEY_HEADER } from "../middleware/admin-auth";

const TEST_ADMIN_KEY = "test-status-admin-key";

describe("Status API", () => {
  beforeEach(() => {
    // Reset service state before each test
    statusService.setMaintenanceMode(false);
    statusService.updateLastIndexedLedger(100000);
    statusService.setMockCurrentLedger(100005); // 5 ledgers lag
    // Provide a known admin key so admin-protected routes are reachable.
    process.env.ADMIN_API_KEY = TEST_ADMIN_KEY;
  });

  afterEach(() => {
    delete process.env.ADMIN_API_KEY;
  });

  it("should return operational status when healthy", async () => {
    const res = await request(app).get("/api/status");
    expect(res.status).toBe(200);
    expect(res.body.status).toBe("operational");
    expect(res.body.maintenance).toBe(false);
    expect(res.body.degraded).toBe(false);
    expect(res.header["cache-control"]).toContain("max-age=30");
  });

  it("should return maintenance status when maintenance mode is enabled", async () => {
    await request(app)
      .post("/api/admin/maintenance")
      .set(ADMIN_KEY_HEADER, TEST_ADMIN_KEY)
      .send({ enabled: true });
    
    const res = await request(app).get("/api/status");
    expect(res.status).toBe(200);
    expect(res.body.status).toBe("maintenance");
    expect(res.body.maintenance).toBe(true);
  });

  it("should return degraded status when index lag is high", async () => {
    statusService.setMockCurrentLedger(100100); // 100 ledgers lag
    
    const res = await request(app).get("/api/status");
    expect(res.status).toBe(200);
    expect(res.body.status).toBe("degraded");
    expect(res.body.degraded).toBe(true);
    expect(res.body.index_lag).toBeGreaterThan(10);
  });

  it("should return 400 for invalid maintenance toggle", async () => {
    const res = await request(app)
      .post("/api/admin/maintenance")
      .set(ADMIN_KEY_HEADER, TEST_ADMIN_KEY)
      .send({ enabled: "not-a-boolean" });
    expect(res.status).toBe(400);
  });

  it("should use fallback ledger when mock ledger is not set", async () => {
    statusService.setMockCurrentLedger(null);
    const res = await request(app).get("/api/status");
    expect(res.status).toBe(200);
    expect(res.body.last_ledger).toBe(100000);
  });

  it("should handle service errors gracefully", async () => {
    jest.spyOn(statusService, "getStatus").mockRejectedValueOnce(new Error("Test error"));
    const res = await request(app).get("/api/status");
    expect(res.status).toBe(500);
    expect(res.body.error).toBe("Internal server error");
  });

  it("should cover singleton initialization", () => {
    const instance = StatusService.getInstance();
    expect(instance).toBe(statusService);
  });

  it("should cover version fallback", async () => {
    const originalVersion = process.env.npm_package_version;
    delete process.env.npm_package_version;
    const res = await request(app).get("/api/status");
    expect(res.body.version).toBe("1.0.0");
    process.env.npm_package_version = originalVersion;
  });

  it("should validate status schema", () => {
    const { StatusSchema } = require("../types/status");
    const validData = {
      status: "operational",
      maintenance: false,
      degraded: false,
      index_lag: 0,
      last_ledger: 100,
      timestamp: new Date().toISOString(),
      version: "1.0.0",
    };
    expect(StatusSchema.parse(validData)).toEqual(validData);
  });
});
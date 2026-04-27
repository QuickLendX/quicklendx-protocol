import request from "supertest";
import app from "../index";
import { getAdminContext, requireAdminRoles } from "../middleware/rbac";
import { adminControlService } from "../services/adminControlService";
import { auditLogService } from "../services/auditLogService";
import { statusService } from "../services/statusService";

const SUPPORT_TOKEN = "support-token";
const OPERATIONS_TOKEN = "operations-token";
const SUPER_ADMIN_TOKEN = "super-admin-token";

function authHeader(token: string): Record<string, string> {
  return {
    Authorization: `Bearer ${token}`,
  };
}

describe("Backend RBAC", () => {
  beforeEach(() => {
    process.env.QLX_SUPPORT_TOKEN = SUPPORT_TOKEN;
    process.env.QLX_OPERATIONS_TOKEN = OPERATIONS_TOKEN;
    process.env.QLX_SUPER_ADMIN_TOKEN = SUPER_ADMIN_TOKEN;

    adminControlService.reset();
    auditLogService.clear();
    statusService.setMaintenanceMode(false);
    statusService.updateLastIndexedLedger(100000);
    statusService.setMockCurrentLedger(100005);
  });

  it("allows support to access read-only troubleshooting endpoints", async () => {
    const res = await request(app)
      .get("/api/admin/status")
      .set(authHeader(SUPPORT_TOKEN));

    expect(res.status).toBe(200);
    expect(res.body.requested_by).toBe("support");

    const auditRes = await request(app)
      .get("/api/admin/audit-logs?limit=5")
      .set(authHeader(SUPPORT_TOKEN));

    expect(auditRes.status).toBe(200);
    expect(Array.isArray(auditRes.body.entries)).toBe(true);
  });

  it("denies support write access to operations endpoints", async () => {
    const res = await request(app)
      .post("/api/admin/maintenance")
      .set(authHeader(SUPPORT_TOKEN))
      .send({ enabled: true });

    expect(res.status).toBe(403);
    expect(res.body.error.code).toBe("INSUFFICIENT_ROLE");
    expect(statusService.getStatus()).resolves.toMatchObject({
      maintenance: false,
    });
  });

  it("allows operations admins to toggle maintenance and queue backfills", async () => {
    const maintenanceRes = await request(app)
      .post("/api/admin/maintenance")
      .set(authHeader(OPERATIONS_TOKEN))
      .send({ enabled: true });

    expect(maintenanceRes.status).toBe(200);
    expect(maintenanceRes.body.updated_by).toBe("operations_admin");

    const backfillRes = await request(app)
      .post("/api/admin/backfill")
      .set(authHeader(OPERATIONS_TOKEN))
      .send({ scope: "ledger:100000-100010" });

    expect(backfillRes.status).toBe(202);
    expect(backfillRes.body.job.requestedBy).toBe("operations_admin");
    expect(adminControlService.listBackfillJobs()).toHaveLength(1);
  });

  it("denies operations admins dangerous config writes", async () => {
    const res = await request(app)
      .post("/api/admin/config/dangerous")
      .set(authHeader(OPERATIONS_TOKEN))
      .send({
        allowEmergencyConfigChanges: true,
        maintenanceWindowMinutes: 60,
      });

    expect(res.status).toBe(403);
    expect(res.body.error.code).toBe("INSUFFICIENT_ROLE");
  });

  it("allows super-admins to perform dangerous config changes", async () => {
    const res = await request(app)
      .post("/api/admin/config/dangerous")
      .set(authHeader(SUPER_ADMIN_TOKEN))
      .send({
        allowEmergencyConfigChanges: true,
        maintenanceWindowMinutes: 45,
      });

    expect(res.status).toBe(200);
    expect(res.body.updated_by).toBe("super_admin");
    expect(res.body.config.allowEmergencyConfigChanges).toBe(true);
    expect(res.body.config.maintenanceWindowMinutes).toBe(45);
  });

  it("rejects missing bearer credentials on protected endpoints", async () => {
    const res = await request(app).get("/api/admin/status");

    expect(res.status).toBe(401);
    expect(res.body.error.code).toBe("AUTH_REQUIRED");
  });

  it("rejects malformed authorization headers as unauthenticated", async () => {
    const res = await request(app)
      .get("/api/admin/status")
      .set("Authorization", "Basic not-a-bearer-token");

    expect(res.status).toBe(401);
    expect(res.body.error.code).toBe("AUTH_REQUIRED");
  });

  it("rejects invalid admin tokens", async () => {
    const res = await request(app)
      .get("/api/admin/status")
      .set(authHeader("invalid-token"));

    expect(res.status).toBe(403);
    expect(res.body.error.code).toBe("FORBIDDEN");
  });

  it("fails closed when RBAC tokens are not configured", async () => {
    delete process.env.QLX_SUPPORT_TOKEN;
    delete process.env.QLX_OPERATIONS_TOKEN;
    delete process.env.QLX_SUPER_ADMIN_TOKEN;

    const res = await request(app)
      .get("/api/admin/status")
      .set(authHeader(SUPPORT_TOKEN));

    expect(res.status).toBe(503);
    expect(res.body.error.code).toBe("RBAC_NOT_CONFIGURED");
  });

  it("fails closed on duplicate admin token configuration", async () => {
    process.env.QLX_SUPPORT_TOKEN = "shared-token";
    process.env.QLX_OPERATIONS_TOKEN = "shared-token";

    const res = await request(app)
      .get("/api/admin/status")
      .set(authHeader("shared-token"));

    expect(res.status).toBe(500);
    expect(res.body.error.code).toBe("RBAC_MISCONFIGURED");
  });

  it("returns 400 for invalid admin payloads", async () => {
    const dangerousConfigRes = await request(app)
      .post("/api/admin/config/dangerous")
      .set(authHeader(SUPER_ADMIN_TOKEN))
      .send({
        allowEmergencyConfigChanges: "yes",
        maintenanceWindowMinutes: 0,
      });

    expect(dangerousConfigRes.status).toBe(400);

    const backfillRes = await request(app)
      .post("/api/admin/backfill")
      .set(authHeader(OPERATIONS_TOKEN))
      .send({ scope: "   " });

    expect(backfillRes.status).toBe(400);

    const auditRes = await request(app)
      .get("/api/admin/audit-logs?limit=0")
      .set(authHeader(SUPPORT_TOKEN));

    expect(auditRes.status).toBe(400);
  });

  it("records audit events for denied and successful admin actions", async () => {
    await request(app)
      .post("/api/admin/maintenance")
      .set(authHeader(SUPPORT_TOKEN))
      .send({ enabled: true });

    await request(app)
      .post("/api/admin/maintenance")
      .set(authHeader(OPERATIONS_TOKEN))
      .send({ enabled: true });

    const auditRes = await request(app)
      .get("/api/admin/audit-logs?limit=10")
      .set(authHeader(SUPPORT_TOKEN));

    expect(auditRes.status).toBe(200);
    expect(
      auditRes.body.entries.some(
        (entry: { action: string; outcome: string; reason?: string }) =>
          entry.action === "admin.maintenance.write" &&
          entry.outcome === "denied" &&
          entry.reason === "insufficient_role",
      ),
    ).toBe(true);
    expect(
      auditRes.body.entries.some(
        (entry: { action: string; outcome: string }) =>
          entry.action === "maintenance.mode.updated" &&
          entry.outcome === "performed",
      ),
    ).toBe(true);
  });

  it("falls back to an unknown client IP and exposes admin context after auth", () => {
    const middleware = requireAdminRoles(["support"], "admin.status.read");
    const req = {
      headers: {
        authorization: `Bearer ${SUPPORT_TOKEN}`,
      },
      method: "GET",
      path: "/api/admin/status",
      ip: undefined,
    } as any;
    const res = {
      status: jest.fn().mockReturnThis(),
      json: jest.fn(),
    } as any;
    const next = jest.fn();

    middleware(req, res, next);

    expect(next).toHaveBeenCalledTimes(1);
    expect(getAdminContext(req).role).toBe("support");
    expect(auditLogService.listEntries(1)[0].ip).toBe("unknown");
  });

  it("throws when admin context is requested before authorization", () => {
    expect(() => getAdminContext({} as any)).toThrow(
      "Admin context is not available on this request.",
    );
  });

  it("caps stored audit history and admin backfill queues", () => {
    for (let index = 0; index < 260; index += 1) {
      auditLogService.recordAuthorization({
        action: `audit_${index}`,
        outcome: "allowed",
        role: "support",
        method: "GET",
        path: "/api/admin/status",
        ip: "127.0.0.1",
      });
    }

    const auditEntries = auditLogService.listEntries(500);
    expect(auditEntries).toHaveLength(100);
    expect(auditEntries[0].action).toBe("audit_259");
    expect(auditEntries[auditEntries.length - 1].action).toBe("audit_160");

    for (let index = 0; index < 55; index += 1) {
      adminControlService.queueBackfill("operations_admin", `scope_${index}`);
    }

    const jobs = adminControlService.listBackfillJobs();
    expect(jobs).toHaveLength(50);
    expect(jobs[0].scope).toBe("scope_54");
    expect(jobs[jobs.length - 1].scope).toBe("scope_5");
  });
});

/**
 * feature-flag.test.ts
 *
 * Test coverage for the per-tenant feature flag system:
 *
 * Unit tests (service + middleware):
 *  ✔ flag-on tenant sees endpoint (next() called)
 *  ✔ flag-off tenant gets 404
 *  ✔ default (no row) treats flag as off → 404
 *  ✔ cache invalidates on toggle
 *  ✔ admin toggle is audited via auditLogService
 *  ✔ flag check adds <1 ms overhead (performance assertion)
 *
 * Integration tests (HTTP via supertest):
 *  ✔ PUT /api/v1/admin/feature-flags/:apiKeyId/:flag enables a flag
 *  ✔ PUT /api/v1/admin/feature-flags/:apiKeyId/:flag disables a flag
 *  ✔ GET /api/v1/admin/feature-flags lists all flags
 *  ✔ GET /api/v1/admin/feature-flags/:apiKeyId lists flags for one tenant
 *  ✔ DELETE /api/v1/admin/feature-flags/:apiKeyId/:flag removes flag row
 *  ✔ Unauthenticated admin requests are rejected with 401/403
 */

import path from "path";
import fs from "fs";
import crypto from "crypto";
import request from "supertest";
import { Request, Response, NextFunction } from "express";

// ── DB isolation setup ──────────────────────────────────────────────────────
import { getDatabase, closeDatabase } from "../lib/database";
import { db } from "../db/database";
import { apiKeyService } from "../services/api-key-service";
import { featureFlagService } from "../services/featureFlagService";
import { auditLogService } from "../services/auditLogService";
import { requireFlag } from "../middleware/feature-flag";
import app from "../app";

const TEST_DB_DIR = path.resolve(__dirname, "../../.data");
const TEST_DB_PATH = path.join(
  TEST_DB_DIR,
  `test-feature-flag-${crypto.randomUUID()}.db`
);

beforeAll(() => {
  process.env.DATABASE_PATH = TEST_DB_PATH;
  process.env.SKIP_API_KEY_AUTH = "true";
  process.env.TEST_ACTOR = "test-actor";

  fs.mkdirSync(TEST_DB_DIR, { recursive: true });
  closeDatabase();

  const conn = getDatabase();

  // Minimal schema required by api-key service
  conn.exec(`
    CREATE TABLE IF NOT EXISTS api_keys (
      id TEXT PRIMARY KEY,
      key_hash TEXT NOT NULL,
      signing_secret_hash TEXT,
      prev_signing_secret_hash TEXT,
      prefix TEXT NOT NULL,
      name TEXT NOT NULL,
      scopes TEXT NOT NULL,
      created_at TEXT NOT NULL,
      last_used_at TEXT,
      expires_at TEXT,
      prev_secret_expires_at TEXT,
      revoked INTEGER NOT NULL DEFAULT 0,
      created_by TEXT NOT NULL
    )
  `);
  conn.exec(
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_api_keys_prefix ON api_keys(prefix)"
  );
  conn.exec(`
    CREATE TABLE IF NOT EXISTS api_key_audit_log (
      id TEXT PRIMARY KEY,
      event_type TEXT NOT NULL CHECK(event_type IN ('created','used','rotated','revoked')),
      key_id TEXT NOT NULL,
      actor TEXT NOT NULL,
      timestamp TEXT NOT NULL,
      ip_address TEXT,
      endpoint TEXT,
      metadata TEXT,
      FOREIGN KEY (key_id) REFERENCES api_keys(id) ON DELETE CASCADE
    )
  `);

  // Feature flags schema
  conn.exec(`
    CREATE TABLE IF NOT EXISTS feature_flags (
      id         TEXT    NOT NULL PRIMARY KEY,
      api_key_id TEXT    NOT NULL,
      flag       TEXT    NOT NULL,
      enabled    INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0, 1)),
      created_at TEXT    NOT NULL,
      updated_at TEXT    NOT NULL,
      updated_by TEXT    NOT NULL,
      UNIQUE(api_key_id, flag)
    )
  `);
  conn.exec(
    "CREATE INDEX IF NOT EXISTS idx_feature_flags_api_key_id ON feature_flags(api_key_id)"
  );
  conn.exec(
    "CREATE INDEX IF NOT EXISTS idx_feature_flags_flag ON feature_flags(flag)"
  );
});

afterAll(() => {
  closeDatabase();
  try {
    if (fs.existsSync(TEST_DB_PATH)) fs.unlinkSync(TEST_DB_PATH);
    try { fs.unlinkSync(TEST_DB_PATH + "-wal"); } catch { /* ok */ }
    try { fs.unlinkSync(TEST_DB_PATH + "-shm"); } catch { /* ok */ }
  } catch { /* ok */ }
  delete process.env.SKIP_API_KEY_AUTH;
  delete process.env.TEST_ACTOR;
});

beforeEach(() => {
  // Clear feature flags and audit buffer before each test
  const conn = getDatabase();
  conn.prepare("DELETE FROM feature_flags").run();
  featureFlagService.clearCache();
  auditLogService.clear();
});

// ============================================================================
// Helpers
// ============================================================================

function buildReq(overrides: Partial<Request> = {}): Request {
  return {
    apiKey: undefined,
    headers: {},
    ip: "127.0.0.1",
    ...overrides,
  } as unknown as Request;
}

function buildRes(): {
  res: Response;
  statusCode: number | undefined;
  body: unknown;
} {
  const ctx: { statusCode: number | undefined; body: unknown } = {
    statusCode: undefined,
    body: undefined,
  };
  const res = {
    status(code: number) {
      ctx.statusCode = code;
      return res;
    },
    json(body: unknown) {
      ctx.body = body;
      return res;
    },
  } as unknown as Response;
  return { res, ...ctx };
}

// ============================================================================
// Unit: FeatureFlagService
// ============================================================================

describe("FeatureFlagService", () => {
  test("isEnabled returns false by default (no row = flag off)", () => {
    expect(featureFlagService.isEnabled("key-abc", "kyc_tiers")).toBe(false);
  });

  test("isEnabled returns true after setFlag enabled=true", () => {
    featureFlagService.setFlag({
      api_key_id: "key-abc",
      flag: "kyc_tiers",
      enabled: true,
      updated_by: "admin",
    });
    expect(featureFlagService.isEnabled("key-abc", "kyc_tiers")).toBe(true);
  });

  test("isEnabled returns false after setFlag enabled=false", () => {
    featureFlagService.setFlag({
      api_key_id: "key-abc",
      flag: "kyc_tiers",
      enabled: false,
      updated_by: "admin",
    });
    expect(featureFlagService.isEnabled("key-abc", "kyc_tiers")).toBe(false);
  });

  test("setFlag upserts: second call updates enabled", () => {
    featureFlagService.setFlag({
      api_key_id: "key-abc",
      flag: "dispute_composer",
      enabled: true,
      updated_by: "admin",
    });
    featureFlagService.setFlag({
      api_key_id: "key-abc",
      flag: "dispute_composer",
      enabled: false,
      updated_by: "admin2",
    });
    expect(featureFlagService.isEnabled("key-abc", "dispute_composer")).toBe(false);
  });

  test("flags are isolated per tenant", () => {
    featureFlagService.setFlag({
      api_key_id: "tenant-A",
      flag: "bid_ranking_v2",
      enabled: true,
      updated_by: "admin",
    });
    expect(featureFlagService.isEnabled("tenant-A", "bid_ranking_v2")).toBe(true);
    expect(featureFlagService.isEnabled("tenant-B", "bid_ranking_v2")).toBe(false);
  });

  test("cache invalidates on toggle: fresh read after setFlag", () => {
    // Prime the cache with false
    expect(featureFlagService.isEnabled("key-xyz", "some_flag")).toBe(false);

    // Toggle on
    featureFlagService.setFlag({
      api_key_id: "key-xyz",
      flag: "some_flag",
      enabled: true,
      updated_by: "admin",
    });

    // Must return the new value, not the stale cached false
    expect(featureFlagService.isEnabled("key-xyz", "some_flag")).toBe(true);
  });

  test("deleteFlag removes the row and returns true", () => {
    featureFlagService.setFlag({
      api_key_id: "key-del",
      flag: "beta_feature",
      enabled: true,
      updated_by: "admin",
    });
    expect(featureFlagService.deleteFlag("key-del", "beta_feature")).toBe(true);
    // After delete, default-deny applies
    expect(featureFlagService.isEnabled("key-del", "beta_feature")).toBe(false);
  });

  test("deleteFlag returns false when row does not exist", () => {
    expect(featureFlagService.deleteFlag("key-missing", "no_such_flag")).toBe(false);
  });

  test("listFlagsForKey returns all flags for a tenant", () => {
    featureFlagService.setFlag({ api_key_id: "key-list", flag: "flag_a", enabled: true, updated_by: "admin" });
    featureFlagService.setFlag({ api_key_id: "key-list", flag: "flag_b", enabled: false, updated_by: "admin" });
    featureFlagService.setFlag({ api_key_id: "other-key", flag: "flag_c", enabled: true, updated_by: "admin" });

    const flags = featureFlagService.listFlagsForKey("key-list");
    expect(flags).toHaveLength(2);
    expect(flags.map((f) => f.flag).sort()).toEqual(["flag_a", "flag_b"]);
  });

  test("listAllFlags returns flags across all tenants", () => {
    featureFlagService.setFlag({ api_key_id: "t1", flag: "f1", enabled: true, updated_by: "admin" });
    featureFlagService.setFlag({ api_key_id: "t2", flag: "f2", enabled: true, updated_by: "admin" });

    const all = featureFlagService.listAllFlags();
    expect(all.length).toBeGreaterThanOrEqual(2);
  });

  test("getFlag returns null when no row exists", () => {
    expect(featureFlagService.getFlag("nobody", "nothing")).toBeNull();
  });

  test("getFlag returns the record when it exists", () => {
    featureFlagService.setFlag({ api_key_id: "key-get", flag: "some_flag", enabled: true, updated_by: "admin" });
    const flag = featureFlagService.getFlag("key-get", "some_flag");
    expect(flag).not.toBeNull();
    expect(flag!.enabled).toBe(true);
    expect(flag!.updated_by).toBe("admin");
  });

  // ── Performance assertion ──────────────────────────────────────────────────
  test("flag check (cache hit) adds <1 ms overhead", () => {
    // Prime the cache
    featureFlagService.setFlag({
      api_key_id: "perf-key",
      flag: "perf_flag",
      enabled: true,
      updated_by: "admin",
    });
    // Warm cache
    featureFlagService.isEnabled("perf-key", "perf_flag");

    const ITERATIONS = 1_000;
    const start = process.hrtime.bigint();
    for (let i = 0; i < ITERATIONS; i++) {
      featureFlagService.isEnabled("perf-key", "perf_flag");
    }
    const elapsedNs = Number(process.hrtime.bigint() - start);
    const avgMs = elapsedNs / 1_000_000 / ITERATIONS;

    // Warm cache hit should be well under 1 ms per call
    expect(avgMs).toBeLessThan(1);
  });
});

// ============================================================================
// Unit: requireFlag middleware
// ============================================================================

describe("requireFlag middleware", () => {
  const next: jest.Mock = jest.fn();

  beforeEach(() => next.mockReset());

  test("calls next() when flag is enabled for the key", () => {
    featureFlagService.setFlag({
      api_key_id: "mw-key-1",
      flag: "kyc_tiers",
      enabled: true,
      updated_by: "admin",
    });

    const req = buildReq({ apiKey: { id: "mw-key-1" } as any });
    const { res } = buildRes();
    requireFlag("kyc_tiers")(req, res, next);

    expect(next).toHaveBeenCalledTimes(1);
  });

  test("responds 404 when flag is disabled for the key", () => {
    featureFlagService.setFlag({
      api_key_id: "mw-key-2",
      flag: "kyc_tiers",
      enabled: false,
      updated_by: "admin",
    });

    const req = buildReq({ apiKey: { id: "mw-key-2" } as any });
    const { res, statusCode, body } = buildRes();

    // Re-capture latest values through closure
    let capturedStatus: number | undefined;
    let capturedBody: unknown;
    const capturingRes = {
      status(code: number) {
        capturedStatus = code;
        return capturingRes;
      },
      json(b: unknown) {
        capturedBody = b;
        return capturingRes;
      },
    } as unknown as Response;

    requireFlag("kyc_tiers")(req, capturingRes, next);

    expect(capturedStatus).toBe(404);
    expect(next).not.toHaveBeenCalled();
    expect((capturedBody as any).error.code).toBe("NOT_FOUND");
  });

  test("responds 404 when no row exists (default-deny)", () => {
    const req = buildReq({ apiKey: { id: "brand-new-key" } as any });
    let capturedStatus: number | undefined;
    const capturingRes = {
      status(code: number) { capturedStatus = code; return capturingRes; },
      json() { return capturingRes; },
    } as unknown as Response;

    requireFlag("nonexistent_flag")(req, capturingRes, next);

    expect(capturedStatus).toBe(404);
    expect(next).not.toHaveBeenCalled();
  });

  test("responds 401 when req.apiKey is not set", () => {
    const req = buildReq({ apiKey: undefined });
    let capturedStatus: number | undefined;
    const capturingRes = {
      status(code: number) { capturedStatus = code; return capturingRes; },
      json() { return capturingRes; },
    } as unknown as Response;

    requireFlag("any_flag")(req, capturingRes, next);

    expect(capturedStatus).toBe(401);
    expect(next).not.toHaveBeenCalled();
  });
});

// ============================================================================
// Integration: Admin HTTP endpoints
// ============================================================================

describe("Admin feature-flag HTTP endpoints", () => {
  let adminKey: string; // operations_admin key (write:*)
  let supportKey: string; // support key (read:*)
  let tenantKeyId: string;

  beforeAll(async () => {
    // Create an operations_admin key for flag management
    const opsKey = await apiKeyService.createApiKey({
      name: "Ops Admin",
      scopes: ["write:*"],
      created_by: "test-setup",
    });
    adminKey = opsKey.plaintext_key;

    // Create a support key (read-only)
    const suppKey = await apiKeyService.createApiKey({
      name: "Support",
      scopes: ["read:*"],
      created_by: "test-setup",
    });
    supportKey = suppKey.plaintext_key;

    // Create a tenant API key whose flags we'll manage
    const tenant = await apiKeyService.createApiKey({
      name: "Tenant Key",
      scopes: ["read:*", "write:invoices"],
      created_by: "test-setup",
    });
    tenantKeyId = tenant.id;
  });

  beforeEach(() => {
    const conn = getDatabase();
    conn.prepare("DELETE FROM feature_flags").run();
    featureFlagService.clearCache();
    auditLogService.clear();
  });

  // ── PUT (enable) ───────────────────────────────────────────────────────────

  test("PUT /feature-flags/:apiKeyId/:flag enables a flag (201/200)", async () => {
    const res = await request(app)
      .put(`/api/v1/admin/feature-flags/${tenantKeyId}/kyc_tiers`)
      .set("Authorization", `Bearer ${adminKey}`)
      .send({ enabled: true });

    expect([200, 201]).toContain(res.status);
    expect(res.body.flag.enabled).toBe(true);
    expect(res.body.flag.flag).toBe("kyc_tiers");
    expect(res.body.flag.api_key_id).toBe(tenantKeyId);

    // Verify it's actually persisted
    expect(featureFlagService.isEnabled(tenantKeyId, "kyc_tiers")).toBe(true);
  });

  test("PUT /feature-flags/:apiKeyId/:flag disables a flag", async () => {
    // First enable
    await request(app)
      .put(`/api/v1/admin/feature-flags/${tenantKeyId}/kyc_tiers`)
      .set("Authorization", `Bearer ${adminKey}`)
      .send({ enabled: true });

    // Then disable
    const res = await request(app)
      .put(`/api/v1/admin/feature-flags/${tenantKeyId}/kyc_tiers`)
      .set("Authorization", `Bearer ${adminKey}`)
      .send({ enabled: false });

    expect([200, 201]).toContain(res.status);
    expect(res.body.flag.enabled).toBe(false);
    expect(featureFlagService.isEnabled(tenantKeyId, "kyc_tiers")).toBe(false);
  });

  test("PUT returns 400 when 'enabled' field is missing or not boolean", async () => {
    const res = await request(app)
      .put(`/api/v1/admin/feature-flags/${tenantKeyId}/kyc_tiers`)
      .set("Authorization", `Bearer ${adminKey}`)
      .send({ enabled: "yes" }); // string instead of boolean

    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("VALIDATION_ERROR");
  });

  // ── GET (list all) ─────────────────────────────────────────────────────────

  test("GET /feature-flags returns all flags", async () => {
    featureFlagService.setFlag({ api_key_id: tenantKeyId, flag: "f1", enabled: true, updated_by: "admin" });
    featureFlagService.setFlag({ api_key_id: "other-id", flag: "f2", enabled: false, updated_by: "admin" });

    const res = await request(app)
      .get("/api/v1/admin/feature-flags")
      .set("Authorization", `Bearer ${adminKey}`);

    expect(res.status).toBe(200);
    expect(Array.isArray(res.body.flags)).toBe(true);
    expect(res.body.flags.length).toBeGreaterThanOrEqual(2);
  });

  test("GET /feature-flags requires operations_admin (support key is denied)", async () => {
    const res = await request(app)
      .get("/api/v1/admin/feature-flags")
      .set("Authorization", `Bearer ${supportKey}`);

    expect(res.status).toBe(403);
  });

  // ── GET (per-key) ──────────────────────────────────────────────────────────

  test("GET /feature-flags/:apiKeyId returns flags for a specific tenant", async () => {
    featureFlagService.setFlag({ api_key_id: tenantKeyId, flag: "kyc_tiers", enabled: true, updated_by: "admin" });
    featureFlagService.setFlag({ api_key_id: tenantKeyId, flag: "bid_ranking_v2", enabled: false, updated_by: "admin" });

    const res = await request(app)
      .get(`/api/v1/admin/feature-flags/${tenantKeyId}`)
      .set("Authorization", `Bearer ${adminKey}`);

    expect(res.status).toBe(200);
    expect(res.body.api_key_id).toBe(tenantKeyId);
    expect(res.body.flags).toHaveLength(2);
  });

  test("GET /feature-flags/:apiKeyId is accessible to support role", async () => {
    const res = await request(app)
      .get(`/api/v1/admin/feature-flags/${tenantKeyId}`)
      .set("Authorization", `Bearer ${supportKey}`);

    expect(res.status).toBe(200);
  });

  // ── DELETE ─────────────────────────────────────────────────────────────────

  test("DELETE /feature-flags/:apiKeyId/:flag removes a flag", async () => {
    featureFlagService.setFlag({ api_key_id: tenantKeyId, flag: "kyc_tiers", enabled: true, updated_by: "admin" });

    const res = await request(app)
      .delete(`/api/v1/admin/feature-flags/${tenantKeyId}/kyc_tiers`)
      .set("Authorization", `Bearer ${adminKey}`);

    expect(res.status).toBe(204);
    expect(featureFlagService.isEnabled(tenantKeyId, "kyc_tiers")).toBe(false);
  });

  test("DELETE returns 404 when flag does not exist", async () => {
    const res = await request(app)
      .delete(`/api/v1/admin/feature-flags/${tenantKeyId}/no_such_flag`)
      .set("Authorization", `Bearer ${adminKey}`);

    expect(res.status).toBe(404);
  });

  // ── Unauthenticated / missing auth ─────────────────────────────────────────

  test("PUT without auth header returns 401 or 403", async () => {
    const res = await request(app)
      .put(`/api/v1/admin/feature-flags/${tenantKeyId}/kyc_tiers`)
      .send({ enabled: true });

    expect([401, 403]).toContain(res.status);
  });

  // ── Audit: toggle is logged ────────────────────────────────────────────────

  test("admin toggle is audited in auditLogService", async () => {
    auditLogService.clear();

    await request(app)
      .put(`/api/v1/admin/feature-flags/${tenantKeyId}/kyc_tiers`)
      .set("Authorization", `Bearer ${adminKey}`)
      .send({ enabled: true });

    const entries = auditLogService.listEntries(10);
    const toggleEntry = entries.find((e) => e.action === "FEATURE_FLAG_TOGGLE");
    expect(toggleEntry).toBeDefined();
    expect(toggleEntry!.outcome).toBe("performed");
    expect((toggleEntry!.metadata as any)?.flag).toBe("kyc_tiers");
    expect((toggleEntry!.metadata as any)?.enabled).toBe(true);
  });
});

// ============================================================================
// Cache invalidation: toggle invalidates stale cache entries
// ============================================================================

describe("Cache invalidation on toggle", () => {
  test("direct service toggle clears cache so next read is fresh", () => {
    // Warm up with enabled=true
    featureFlagService.setFlag({ api_key_id: "ci-key", flag: "ci_flag", enabled: true, updated_by: "admin" });
    expect(featureFlagService.isEnabled("ci-key", "ci_flag")).toBe(true); // populates cache

    // Disable via setFlag — must invalidate cache
    featureFlagService.setFlag({ api_key_id: "ci-key", flag: "ci_flag", enabled: false, updated_by: "admin" });
    expect(featureFlagService.isEnabled("ci-key", "ci_flag")).toBe(false); // stale cache must not return true
  });

  test("deleteFlag invalidates cache", () => {
    featureFlagService.setFlag({ api_key_id: "ci-key2", flag: "del_flag", enabled: true, updated_by: "admin" });
    featureFlagService.isEnabled("ci-key2", "del_flag"); // prime cache

    featureFlagService.deleteFlag("ci-key2", "del_flag");
    expect(featureFlagService.isEnabled("ci-key2", "del_flag")).toBe(false);
  });
});

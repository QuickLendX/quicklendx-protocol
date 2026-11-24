import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { randomUUID } from "node:crypto";
import request from "supertest";
import app from "../app";
import { apiKeyService } from "../services/api-key-service";
import { getDatabase, closeDatabase } from "../lib/database";
import v8 from "../migrations/v008_create_api_keys";
import v13 from "../migrations/v013_api_key_versions";
import v14 from "../migrations/v014_add_api_key_rotation";
import v15 from "../migrations/v015_add_signing_secret_hash";
import type { MigrationContext } from "../lib/migrations/types";

/**
 * Integration tests for the rotate / rotate-signing-secret endpoints, run
 * against the real mounted app, controllers, service and SQLite store
 * (migrations v008 + v013 + v014 + v015).
 */

const ROTATE = (id: string) => `/api/v1/api-keys/${id}/rotate`;
const ROTATE_SECRET = (id: string) => `/api/v1/api-keys/${id}/rotate-signing-secret`;

let opsAdminKey: string; // admin:keys   -> operations_admin
let superAdminKey: string; // admin:*, admin:keys -> super_admin

async function createTarget(overrides: { expires_at?: string } = {}) {
  return apiKeyService.createApiKey({
    name: "Target Key",
    scopes: ["read:invoices"],
    created_by: "target-user",
    expires_at: overrides.expires_at,
  });
}

beforeAll(async () => {
  process.env.DATABASE_PATH = ":memory:";
  closeDatabase();
  const ctx = {
    db: {
      exec: async (sql: string) => {
        getDatabase().exec(sql);
        return [];
      },
    },
  } as unknown as MigrationContext;
  for (const migration of [v8, v13, v14, v15]) await migration.up(ctx);

  opsAdminKey = (
    await apiKeyService.createApiKey({ name: "ops", scopes: ["admin:keys"], created_by: "ops-admin" })
  ).plaintext_key;
  superAdminKey = (
    await apiKeyService.createApiKey({ name: "root", scopes: ["admin:*", "admin:keys"], created_by: "super-admin" })
  ).plaintext_key;
});

afterAll(async () => {
  await new Promise((resolve) => setImmediate(resolve));
  closeDatabase();
});

describe("POST /api/v1/api-keys/:id/rotate (integration)", () => {
  it("rejects unauthenticated callers and keys without admin:keys", async () => {
    const target = await createTarget();
    await request(app).post(ROTATE(target.id)).send({}).expect(401);

    const reader = await createTarget();
    await request(app)
      .post(ROTATE(target.id))
      .auth(reader.plaintext_key, { type: "bearer" })
      .send({})
      .expect(403);
  });

  it("issues a replacement key once, revokes the old one, and leaks no hashes", async () => {
    const target = await createTarget();
    const res = await request(app)
      .post(ROTATE(target.id))
      .auth(opsAdminKey, { type: "bearer" })
      .send({})
      .expect(200);

    const rotated = res.body.data;
    expect(rotated.plaintext_key).toMatch(/^qlx_/);
    expect(rotated.plaintext_key).not.toBe(target.plaintext_key);
    expect(rotated.id).not.toBe(target.id);
    expect(JSON.stringify(res.body)).not.toMatch(/_hash/);

    expect(await apiKeyService.verifyApiKey(target.plaintext_key)).toBeNull();
    expect(await apiKeyService.verifyApiKey(rotated.plaintext_key)).not.toBeNull();

    // Reads never re-expose the plaintext key.
    const read = await request(app)
      .get(`/api/v1/api-keys/${rotated.id}`)
      .auth(opsAdminKey, { type: "bearer" })
      .expect(200);
    expect(JSON.stringify(read.body)).not.toMatch(/plaintext|_hash/);
  });

  it("returns 404 for an unknown key id", async () => {
    await request(app)
      .post(ROTATE(randomUUID()))
      .auth(opsAdminKey, { type: "bearer" })
      .send({})
      .expect(404);
  });

  it("rejects rotation of a revoked key", async () => {
    const target = await createTarget();
    await apiKeyService.revokeApiKey(target.id, "ops-admin");
    await request(app)
      .post(ROTATE(target.id))
      .auth(opsAdminKey, { type: "bearer" })
      .send({})
      .expect(403);
  });

  it("rejects rotation of an expired key", async () => {
    const target = await createTarget({ expires_at: new Date(Date.now() + 3_600_000).toISOString() });
    getDatabase()
      .prepare("UPDATE api_keys SET expires_at = ? WHERE id = ?")
      .run(new Date(Date.now() - 60_000).toISOString(), target.id);
    await request(app)
      .post(ROTATE(target.id))
      .auth(opsAdminKey, { type: "bearer" })
      .send({})
      .expect(403);
  });

  it("records a rotation audit event that never contains key hashes", async () => {
    const target = await createTarget();
    await request(app).post(ROTATE(target.id)).auth(opsAdminKey, { type: "bearer" }).send({}).expect(200);

    const logs = await request(app)
      .get(`/api/v1/api-keys/${target.id}/audit-logs`)
      .auth(opsAdminKey, { type: "bearer" })
      .expect(200);
    const events = logs.body.data as Array<Record<string, unknown>>;
    expect(events.some((e) => /rotat/i.test(String(e.event_type ?? e.eventType)))).toBe(true);
    expect(JSON.stringify(logs.body)).not.toMatch(/_hash/);
  });
});

describe("POST /api/v1/api-keys/:id/rotate-signing-secret (integration)", () => {
  it("requires the super_admin role (operations_admin is rejected)", async () => {
    const target = await createTarget();
    await request(app).post(ROTATE_SECRET(target.id)).send({}).expect(401);
    await request(app)
      .post(ROTATE_SECRET(target.id))
      .auth(opsAdminKey, { type: "bearer" })
      .send({})
      .expect(403);
  });

  it("returns the new signing secret once and preserves the previous secret for the grace window", async () => {
    const target = await createTarget();
    const res = await request(app)
      .post(ROTATE_SECRET(target.id))
      .auth(superAdminKey, { type: "bearer" })
      .send({ grace_window_hours: 2 })
      .expect(200);

    expect(typeof res.body.data.plaintext_signing_secret).toBe("string");
    expect(res.body.data.plaintext_signing_secret.length).toBeGreaterThan(0);
    expect(JSON.stringify(res.body)).not.toMatch(/_hash/);

    const row = getDatabase().prepare("SELECT * FROM api_keys WHERE id = ?").get(target.id) as Record<string, any>;
    expect(row.prev_secret_expires_at).toBeTruthy();
    const graceMs = Date.parse(row.prev_secret_expires_at) - Date.now();
    expect(graceMs).toBeGreaterThan(60 * 60 * 1000);
    expect(graceMs).toBeLessThanOrEqual(2 * 60 * 60 * 1000 + 5_000);
  });

  it("rejects a second rotation while the previous secret is still in its grace window", async () => {
    const target = await createTarget();
    await request(app).post(ROTATE_SECRET(target.id)).auth(superAdminKey, { type: "bearer" }).send({}).expect(200);
    await request(app).post(ROTATE_SECRET(target.id)).auth(superAdminKey, { type: "bearer" }).send({}).expect(409);
  });

  it("validates the grace window", async () => {
    const target = await createTarget();
    for (const grace_window_hours of [-1, 169]) {
      await request(app)
        .post(ROTATE_SECRET(target.id))
        .auth(superAdminKey, { type: "bearer" })
        .send({ grace_window_hours })
        .expect(400);
    }
  });

  it("returns 404 for unknown keys and 403 for revoked keys", async () => {
    await request(app)
      .post(ROTATE_SECRET(randomUUID()))
      .auth(superAdminKey, { type: "bearer" })
      .send({})
      .expect(404);

    const target = await createTarget();
    await apiKeyService.revokeApiKey(target.id, "super-admin");
    await request(app)
      .post(ROTATE_SECRET(target.id))
      .auth(superAdminKey, { type: "bearer" })
      .send({})
      .expect(403);
  });

  it("serializes concurrent rotations: one success, the rest conflict, never a 5xx", async () => {
    const target = await createTarget();
    const results = await Promise.all(
      Array.from({ length: 5 }, () =>
        request(app).post(ROTATE_SECRET(target.id)).auth(superAdminKey, { type: "bearer" }).send({})
      )
    );
    const statuses = results.map((r) => r.status);
    expect(statuses.filter((s) => s === 200)).toHaveLength(1);
    for (const s of statuses.filter((s) => s !== 200)) expect(s).toBe(409);
  });
});

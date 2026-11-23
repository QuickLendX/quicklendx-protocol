import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import request from "supertest";
import app from "../app";
import { apiKeyService } from "../services/api-key-service";
import { getDatabase, closeDatabase } from "../lib/database";
import v6 from "../migrations/v006_create_api_keys";
import v10 from "../migrations/v010_add_api_key_rotation";
import v11 from "../migrations/v011_add_signing_secret_hash";
import v13 from "../migrations/v013_api_key_versions";
import type { MigrationContext } from "../lib/migrations/types";

// Unrelated upstream reconciliation code has duplicate declarations. Keep the
// real app/v1 mounting and all API-key middleware, controllers and SQLite code.
vi.mock("../routes/v1/reconciliation", async () => ({ default: (await import("express")).Router() }));

let admin: string;
beforeAll(async () => {
  process.env.DATABASE_PATH = ":memory:";
  const ctx = { db: { exec: async (sql: string) => { getDatabase().exec(sql); return []; } } } as MigrationContext;
  for (const migration of [v6, v10, v11, v13]) await migration.up(ctx);
  admin = (await apiKeyService.createApiKey({ name: "admin", scopes: ["admin:keys"], created_by: "admin" })).plaintext_key;
});
afterAll(async () => { await new Promise(resolve => setImmediate(resolve)); closeDatabase(); });

describe("mounted API key management routes", () => {
  it("discovers scopes publicly and rejects unauthenticated writes", async () => {
    await request(app).get("/api/v1/api-keys/scopes").expect(200);
    await request(app).post("/api/v1/api-keys").send({ name: "ci", scopes: ["read:invoices"] }).expect(401);
  });
  it("creates with an admin key, derives ownership, and never exposes hashes on reads", async () => {
    const created = await request(app).post("/api/v1/api-keys").auth(admin, { type: "bearer" })
      .send({ name: "ci", scopes: ["read:invoices"], created_by: "spoofed" }).expect(201);
    const key = created.body.data;
    expect(key.plaintext_key).toMatch(/^qlx_/);
    expect(key.created_by).toBe("admin");
    expect(JSON.stringify(created.body)).not.toContain("_hash");
    const read = await request(app).get(`/api/v1/api-keys/${key.id}`).auth(admin, { type: "bearer" }).expect(200);
    expect(JSON.stringify(read.body)).not.toMatch(/plaintext|_hash/);
    const list = await request(app).get("/api/v1/api-keys").auth(admin, { type: "bearer" }).expect(200);
    expect(JSON.stringify(list.body)).not.toMatch(/plaintext|_hash/);
    await request(app).post("/api/v1/api-keys").auth(key.plaintext_key, { type: "bearer" })
      .send({ name: "no", scopes: ["admin:keys"] }).expect(403);
    const first = await request(app).post(`/api/v1/api-keys/${key.id}/revoke`).auth(admin, { type: "bearer" }).send({}).expect(200);
    const second = await request(app).post(`/api/v1/api-keys/${key.id}/revoke`).auth(admin, { type: "bearer" }).send({}).expect(200);
    expect(second.body.data.revokedAt).toBe(first.body.data.revokedAt);
    expect(await apiKeyService.verifyApiKey(key.plaintext_key)).toBeNull();
  });
});

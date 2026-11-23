import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { getDatabase, closeDatabase } from "../../lib/database";
import { db } from "../../db/database";
import { ApiKeyService } from "../api-key-service";
import { FrozenClock } from "../clock";
import { hashApiKey } from "../../models/api-key";
import { ApiKeyError } from "../../errors/api-key-error";
import v6 from "../../migrations/v006_create_api_keys";
import v10 from "../../migrations/v010_add_api_key_rotation";
import v11 from "../../migrations/v011_add_signing_secret_hash";
import v13 from "../../migrations/v013_api_key_versions";
import type { MigrationContext } from "../../lib/migrations/types";

const clock = new FrozenClock(Date.now());
const service = new ApiKeyService(db, clock);
const input = { name: "ci", scopes: ["read:invoices"], created_by: "admin" };
const flushAudit = () => new Promise<void>(resolve => setImmediate(resolve));

beforeAll(async () => {
  process.env.DATABASE_PATH = ":memory:";
  const ctx = { db: { exec: async (sql: string) => { getDatabase().exec(sql); return []; } } } as MigrationContext;
  for (const migration of [v6, v10, v11, v13]) await migration.up(ctx);
});
beforeEach(async () => { await flushAudit(); db.clear(); });
afterAll(async () => { await flushAudit(); closeDatabase(); });

describe("SQLite API keys", () => {
  it("returns credentials once, stores hashes, verifies and revokes", async () => {
    const key = await service.createApiKey(input);
    expect(key.plaintext_key).toMatch(/^qlx_[A-Za-z0-9_-]{32,}$/);
    const row = db.getApiKeyById(key.id)!;
    expect(row.key_hash).toBe(hashApiKey(key.plaintext_key));
    expect(row.signing_secret_hash).toBe(hashApiKey(key.plaintext_signing_secret));
    expect(JSON.stringify(row)).not.toContain(key.plaintext_key);
    expect(JSON.stringify(row)).not.toContain(key.plaintext_signing_secret);
    expect((await service.verifyApiKey(key.plaintext_key))?.id).toBe(key.id);
    expect(await service.verifyApiKey(key.plaintext_key + "x")).toBeNull();
    expect(await service.verifyApiKey("qlx_" + "x".repeat(43))).toBeNull();
    expect(await service.getApiKeyById(key.id)).not.toHaveProperty("plaintext_key");
    await service.revokeApiKey(key.id, "admin");
    expect(await service.verifyApiKey(key.plaintext_key)).toBeNull();
  });
  it("persists OCC and idempotent timestamps across service instances", async () => {
    const key = await service.createApiKey(input);
    await service.rotateSigningSecret(key.id, "admin", undefined, 1);
    await expect(service.revokeApiKey(key.id, "admin", 0)).rejects.toBeInstanceOf(ApiKeyError);
    await expect(service.revokeApiKey(key.id, "admin", key.version)).rejects.toMatchObject({ code: "CONFLICT" });
    const revoked = await service.revokeApiKey(key.id, "admin", key.version + 1);
    clock.advance(1000);
    const again = await new ApiKeyService(db, clock).revokeApiKey(key.id, "other");
    expect(again).toEqual(revoked);
  });
  it("rejects expired keys and invalid inputs", async () => {
    const key = await service.createApiKey({ ...input, expires_at: new Date(clock.now() + 1000).toISOString() });
    clock.advance(1000);
    expect(await service.verifyApiKey(key.plaintext_key)).toBeNull();
    await expect(service.createApiKey({ ...input, scopes: ["bogus"] })).rejects.toMatchObject({ code: "VALIDATION_ERROR" });
    await expect(service.createApiKey({ ...input, expires_at: "bad" })).rejects.toMatchObject({ code: "VALIDATION_ERROR" });
  });
  it("rotates credentials atomically and preserves grants", async () => {
    const key = await service.createApiKey(input);
    const rotated = await service.rotateApiKey(key.id, "admin");
    expect(await service.verifyApiKey(key.plaintext_key)).toBeNull();
    expect((await service.verifyApiKey(rotated.plaintext_key))?.scopes).toEqual(input.scopes);
    expect((await service.getApiKey(key.id))?.version).toBe(2);
  });
  it("rotates only signing secrets and enforces the grace window", async () => {
    const key = await service.createApiKey(input);
    const rotated = await service.rotateSigningSecret(key.id, "admin", undefined, 1);
    expect(rotated.prev_signing_secret_hash).toBe(hashApiKey(key.plaintext_signing_secret));
    expect(rotated.signing_secret_hash).toBe(hashApiKey(rotated.plaintext_signing_secret));
    expect(await service.verifyApiKey(key.plaintext_key)).not.toBeNull();
    await expect(service.rotateSigningSecret(key.id, "admin")).rejects.toMatchObject({ code: "CONFLICT" });
    clock.advance(3600000);
    await expect(service.rotateSigningSecret(key.id, "admin")).resolves.toHaveProperty("plaintext_signing_secret");
  });
  it("rolls back a replacement key if revoking the old key fails", async () => {
    const key = await service.createApiKey(input);
    const update = vi.spyOn(db, "updateApiKey").mockImplementationOnce(() => { throw new Error("write failed"); });
    await expect(service.rotateApiKey(key.id, "admin")).rejects.toThrow("write failed");
    update.mockRestore();
    expect((await service.listApiKeys()).map(k => k.id)).toEqual([key.id]);
    expect(await service.verifyApiKey(key.plaintext_key)).not.toBeNull();
  });
  it("allows only one concurrent mutation for the same version", async () => {
    const key = await service.createApiKey(input);
    const results = await Promise.allSettled([
      service.revokeApiKey(key.id, "admin", key.version),
      service.revokeApiKey(key.id, "other", key.version),
    ]);
    expect(results.map(r => r.status)).toEqual(["fulfilled", "rejected"]);
    expect((results[1] as PromiseRejectedResult).reason).toMatchObject({ code: "CONFLICT" });
    expect((await service.getApiKey(key.id))?.version).toBe(key.version + 1);
  });
  it("lists and filters keys, updates usage, and writes non-secret audits", async () => {
    const key = await service.createApiKey(input);
    await service.updateLastUsed(key.id, "/test");
    expect((await service.listApiKeys({ created_by: "admin", revoked: false })).map(k => k.id)).toEqual([key.id]);
    expect(await service.listApiKeys({ created_by: "other" })).toEqual([]);
    expect((await service.getApiKey(key.id))?.last_used_at).not.toBeNull();
    await flushAudit();
    expect(db.getAuditLogs({ key_id: key.id }).map(log => log.event_type)).toEqual(expect.arrayContaining(["created", "used"]));
    expect(JSON.stringify(db.getAuditLogs())).not.toContain(key.plaintext_key);
  });
});

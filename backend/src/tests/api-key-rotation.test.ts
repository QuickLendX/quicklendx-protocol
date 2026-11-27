import { apiKeyService } from "../services/api-key-service";
import { auditLogService } from "../services/audit-log";
import { db } from "../db/database";
import { hashApiKey } from "../models/api-key";

jest.mock("../config", () => ({
  config: { DATABASE_PATH: ":memory:" },
}));

jest.mock("../services/audit-log", () => ({
  auditLogService: {
    logCreated: jest.fn(),
    logUsed: jest.fn(),
    logRotated: jest.fn(),
    logRevoked: jest.fn(),
  },
}));

const { getDatabase, closeDatabase } = require("../lib/database") as typeof import(
  "../lib/database"
);

describe("API key signing secret rotation", () => {
  let adminId: string;

  beforeAll(() => {
    closeDatabase();
    const connection = getDatabase();

    connection.exec(`
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
        created_by TEXT NOT NULL,
        version INTEGER NOT NULL DEFAULT 1,
        revoked_at INTEGER,
        revoked_by TEXT
      )
    `);
    connection.exec(`
      CREATE UNIQUE INDEX IF NOT EXISTS idx_api_keys_prefix ON api_keys(prefix)
    `);
    connection.exec(`
      CREATE TABLE IF NOT EXISTS api_key_audit_log (
        id TEXT PRIMARY KEY,
        event_type TEXT NOT NULL,
        key_id TEXT NOT NULL,
        actor TEXT NOT NULL,
        timestamp TEXT NOT NULL,
        ip_address TEXT,
        endpoint TEXT,
        metadata TEXT
      )
    `);
  });

  afterAll(() => {
    closeDatabase();
  });

  beforeEach(() => {
    db.clear();
    adminId = "admin-user";
    jest.clearAllMocks();
  });

  it("rotates the signing secret and retains its hash during the grace window", async () => {
    const created = await apiKeyService.createApiKey({
      name: "Test Key",
      scopes: ["read:invoices"],
      created_by: adminId,
    });

    const rotated = await apiKeyService.rotateSigningSecret(
      created.id,
      adminId,
      "127.0.0.1",
      24
    );
    const stored = db.getApiKeyById(created.id)!;

    expect(rotated.id).toBe(created.id);
    expect(rotated.plaintext_signing_secret).not.toBe(
      created.plaintext_signing_secret
    );
    expect(rotated.signing_secret_hash).toBe(
      hashApiKey(rotated.plaintext_signing_secret)
    );
    expect(rotated.prev_signing_secret_hash).toBe(
      hashApiKey(created.plaintext_signing_secret)
    );
    expect(Date.parse(rotated.prev_secret_expires_at!)).toBeGreaterThan(
      Date.now()
    );
    expect(stored.signing_secret_hash).toBe(rotated.signing_secret_hash);
    expect(stored.prev_signing_secret_hash).toBe(
      hashApiKey(created.plaintext_signing_secret)
    );
    expect(
      await apiKeyService.verifyApiKey(created.plaintext_key)
    ).not.toBeNull();
    expect(auditLogService.logRotated).toHaveBeenCalledWith(
      created.id,
      created.id,
      adminId,
      "127.0.0.1"
    );
  });

  it("expires the previous signing secret immediately with a zero-hour grace window", async () => {
    const created = await apiKeyService.createApiKey({
      name: "Test Key",
      scopes: ["read:invoices"],
      created_by: adminId,
    });

    const rotated = await apiKeyService.rotateSigningSecret(
      created.id,
      adminId,
      "127.0.0.1",
      0
    );

    expect(rotated.prev_signing_secret_hash).toBe(
      hashApiKey(created.plaintext_signing_secret)
    );
    expect(Date.parse(rotated.prev_secret_expires_at!)).toBeLessThanOrEqual(
      Date.now()
    );
    expect(rotated.signing_secret_hash).toBe(
      hashApiKey(rotated.plaintext_signing_secret)
    );
  });

  it("a later rotation replaces the previous signing-secret grace slot", async () => {
    const created = await apiKeyService.createApiKey({
      name: "Test Key",
      scopes: ["read:invoices"],
      created_by: adminId,
    });
    const first = await apiKeyService.rotateSigningSecret(
      created.id,
      adminId,
      undefined,
      0
    );
    const second = await apiKeyService.rotateSigningSecret(
      created.id,
      adminId,
      undefined,
      24
    );

    expect(second.signing_secret_hash).toBe(
      hashApiKey(second.plaintext_signing_secret)
    );
    expect(second.prev_signing_secret_hash).toBe(
      hashApiKey(first.plaintext_signing_secret)
    );
    expect(second.prev_signing_secret_hash).not.toBe(
      hashApiKey(created.plaintext_signing_secret)
    );
  });

  it("cannot rotate a revoked key", async () => {
    const created = await apiKeyService.createApiKey({
      name: "Test Key",
      scopes: ["read:invoices"],
      created_by: adminId,
    });
    await apiKeyService.revokeApiKey(created.id, adminId);

    await expect(
      apiKeyService.rotateSigningSecret(created.id, adminId)
    ).rejects.toThrow("API key is revoked or expired.");
  });

  it("rejects rotation for a non-existent key", async () => {
    await expect(
      apiKeyService.rotateSigningSecret("missing-key-id", adminId)
    ).rejects.toMatchObject({ code: "NOT_FOUND" });
  });

  it("records the actor who requested the rotation", async () => {
    const created = await apiKeyService.createApiKey({
      name: "Test Key",
      scopes: ["read:invoices"],
      created_by: adminId,
    });

    await apiKeyService.rotateSigningSecret(created.id, "other-user");

    expect(auditLogService.logRotated).toHaveBeenCalledWith(
      created.id,
      created.id,
      "other-user",
      undefined
    );
  });

  it("rejects rotation with an invalid grace window", async () => {
    const created = await apiKeyService.createApiKey({
      name: "Test Key",
      scopes: ["read:invoices"],
      created_by: adminId,
    });

    await expect(
      apiKeyService.rotateSigningSecret(
        created.id,
        adminId,
        undefined,
        Number.NaN
      )
    ).rejects.toThrow("Grace hours must be between 0 and 168.");
  });

  it("allows only one concurrent rotation while a grace secret is active", async () => {
    const created = await apiKeyService.createApiKey({
      name: "Test Key",
      scopes: ["read:invoices"],
      created_by: adminId,
    });

    const results = await Promise.allSettled([
      apiKeyService.rotateSigningSecret(created.id, adminId),
      apiKeyService.rotateSigningSecret(created.id, adminId),
    ]);

    expect(results.map((result) => result.status).sort()).toEqual([
      "fulfilled",
      "rejected",
    ]);
    const rejected = results.find(
      (result): result is PromiseRejectedResult => result.status === "rejected"
    );
    expect(rejected?.reason).toMatchObject({ code: "CONFLICT" });
  });

  it("rolls back a failed database write without changing the signing secret", async () => {
    const created = await apiKeyService.createApiKey({
      name: "Test Key",
      scopes: ["read:invoices"],
      created_by: adminId,
    });
    const update = jest
      .spyOn(db, "updateApiKey")
      .mockImplementationOnce(() => {
        throw new Error("simulated database failure");
      });

    try {
      await expect(
        apiKeyService.rotateSigningSecret(created.id, adminId)
      ).rejects.toThrow("simulated database failure");
    } finally {
      update.mockRestore();
    }

    const stored = db.getApiKeyById(created.id)!;
    expect(stored.signing_secret_hash).toBe(
      hashApiKey(created.plaintext_signing_secret)
    );
    expect(stored.prev_signing_secret_hash).toBeNull();
  });

  it("allows a rotation retry after a transient database failure", async () => {
    const created = await apiKeyService.createApiKey({
      name: "Test Key",
      scopes: ["read:invoices"],
      created_by: adminId,
    });
    const update = jest
      .spyOn(db, "updateApiKey")
      .mockImplementationOnce(() => {
        throw new Error("simulated transient failure");
      });

    try {
      await expect(
        apiKeyService.rotateSigningSecret(created.id, adminId)
      ).rejects.toThrow("simulated transient failure");
    } finally {
      update.mockRestore();
    }

    const retried = await apiKeyService.rotateSigningSecret(
      created.id,
      adminId,
      undefined,
      24
    );
    const stored = db.getApiKeyById(created.id)!;
    expect(stored.prev_signing_secret_hash).toBe(
      hashApiKey(created.plaintext_signing_secret)
    );
    expect(stored.signing_secret_hash).toBe(
      hashApiKey(retried.plaintext_signing_secret)
    );
  });

  it("does not include plaintext signing secrets in audit logs", async () => {
    const created = await apiKeyService.createApiKey({
      name: "Test Key",
      scopes: ["read:invoices"],
      created_by: adminId,
    });

    const rotated = await apiKeyService.rotateSigningSecret(
      created.id,
      adminId,
      "127.0.0.1"
    );
    const auditCalls = (auditLogService.logRotated as jest.Mock).mock.calls;
    const serialized = JSON.stringify(auditCalls);

    expect(serialized).not.toContain(rotated.plaintext_signing_secret);
    expect(serialized).not.toContain(created.plaintext_signing_secret);
  });
});

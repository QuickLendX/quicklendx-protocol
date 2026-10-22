import crypto from "node:crypto";
import { db, type DbApiKey } from "../db/database";
import { type ApiKey, type ApiKeyService as ServiceContract, type ApiKeyCreateInput,
  type ApiKeyWithPlaintext, type RevokedApiKey, generateApiKey, hashApiKey, timingSafeCompare } from "../models/api-key";
import { validateScopes } from "../config/scopes";
import { auditLogService } from "./audit-log";
import { ApiKeyError } from "../errors/api-key-error";
import { SystemClock, type Clock } from "./clock";

type Store = Pick<typeof db, "createApiKey" | "getApiKeyById" | "getApiKeyByPrefix" | "listApiKeys" | "updateApiKey" | "transaction">;
type Audit = Pick<typeof auditLogService, "logCreated" | "logUsed" | "logRotated" | "logRevoked">;

/** Synchronous SQLite transactions keep read/version/write atomic. */
export class ApiKeyService implements ServiceContract {
  constructor(protected readonly store: Store = db, protected readonly clock: Clock = new SystemClock(),
    private readonly audit: Audit = auditLogService) {}

  protected decode(row: DbApiKey): ApiKey {
    const scopes: string[] = JSON.parse(row.scopes);
    const revoked = row.revoked !== 0;
    const expired = row.expires_at !== null && Date.parse(row.expires_at) <= this.clock.now();
    return {
      id: row.id, name: row.name, prefix: row.prefix, scopes,
      created_by: row.created_by, created_at: row.created_at,
      last_used_at: row.last_used_at, expires_at: row.expires_at, revoked,
      signing_secret_hash: row.signing_secret_hash, prev_signing_secret_hash: row.prev_signing_secret_hash,
      prev_secret_expires_at: row.prev_secret_expires_at,
      status: revoked ? "revoked" : expired ? "expired" : "active", version: row.version ?? 1,
      revokedAt: row.revoked_at ?? null, revokedBy: row.revoked_by ?? null,
      // Compatibility projections for the existing revoke controller.
      ownerId: row.created_by, permissions: [...scopes], createdAt: Date.parse(row.created_at),
    };
  }

  private requireRow(id: string): DbApiKey {
    const row = this.store.getApiKeyById(id);
    if (!row) throw new ApiKeyError("NOT_FOUND", "API key not found.", { apiKeyId: id });
    return row;
  }

  private requireActive(row: DbApiKey): void {
    if (this.decode(row).status !== "active") throw new ApiKeyError("FORBIDDEN", "API key is revoked or expired.");
  }

  private newKey(input: ApiKeyCreateInput): { row: DbApiKey; plaintext: ApiKeyWithPlaintext } {
    if (!input.name?.trim() || !input.created_by?.trim() || !Array.isArray(input.scopes) ||
      !validateScopes(input.scopes).valid || (input.expires_at != null &&
      (!Number.isFinite(Date.parse(input.expires_at)) || Date.parse(input.expires_at) <= this.clock.now()))) {
      throw new ApiKeyError("VALIDATION_ERROR", "Invalid API key input.");
    }
    const credential = generateApiKey();
    const secret = crypto.randomBytes(32).toString("base64url");
    const row: DbApiKey = {
      id: crypto.randomUUID(), name: input.name, prefix: credential.prefix, key_hash: credential.hash,
      scopes: JSON.stringify(input.scopes), created_by: input.created_by,
      created_at: new Date(this.clock.now()).toISOString(), last_used_at: null,
      expires_at: input.expires_at ?? null, revoked: 0, signing_secret_hash: hashApiKey(secret),
      prev_signing_secret_hash: null, prev_secret_expires_at: null, version: 1, revoked_at: null, revoked_by: null,
    };
    return { row, plaintext: { ...this.decode(row), plaintext_key: credential.key, plaintext_signing_secret: secret } };
  }

  async createApiKey(input: ApiKeyCreateInput, ip?: string): Promise<ApiKeyWithPlaintext> {
    const { row, plaintext } = this.newKey(input);
    this.store.createApiKey(row);
    await this.audit.logCreated(row.id, input.created_by, ip);
    return plaintext;
  }

  async verifyApiKey(plaintext: string): Promise<ApiKey | null> {
    if (typeof plaintext !== "string" || !/^qlx_[A-Za-z0-9_-]{32,}$/.test(plaintext)) return null;
    const row = this.store.getApiKeyByPrefix(plaintext.slice(0, 15));
    if (!row || !timingSafeCompare(hashApiKey(plaintext), row.key_hash)) return null;
    const key = this.decode(row);
    return key.status === "active" ? key : null;
  }

  async getApiKeyById(id: string): Promise<ApiKey | null> {
    const row = this.store.getApiKeyById(id);
    return row ? this.decode(row) : null;
  }
  getApiKey(id: string): Promise<ApiKey | null> { return this.getApiKeyById(id); }
  async listApiKeys(filters: { created_by?: string; revoked?: boolean } = {}): Promise<ApiKey[]> {
    return this.store.listApiKeys(filters).map(row => this.decode(row));
  }
  async updateLastUsed(id: string, endpoint: string, ip?: string): Promise<void> {
    const row = this.store.getApiKeyById(id);
    if (!row) return;
    this.store.updateApiKey(id, { last_used_at: new Date(this.clock.now()).toISOString() });
    await this.audit.logUsed(id, row.created_by, endpoint, ip);
  }
  async revokeApiKey(id: string, actor: string, expectedVersion?: number): Promise<RevokedApiKey> {
    let changed = false;
    const result = this.store.transaction(() => {
      const row = this.requireRow(id);
      if (expectedVersion !== undefined && expectedVersion !== (row.version ?? 1)) {
        throw new ApiKeyError("CONFLICT", "API key version changed; re-read and retry.", { apiKeyId: id, actorId: actor });
      }
      if (row.revoked) return this.decode(row) as RevokedApiKey;
      const updates = { revoked: 1, version: (row.version ?? 1) + 1, revoked_at: this.clock.now(), revoked_by: actor };
      this.store.updateApiKey(id, updates);
      changed = true;
      return this.decode({ ...row, ...updates }) as RevokedApiKey;
    });
    if (changed) await this.audit.logRevoked(id, actor);
    return result;
  }
  async rotateApiKey(id: string, actor: string, ip?: string): Promise<ApiKeyWithPlaintext> {
    const result = this.store.transaction(() => {
      const old = this.requireRow(id);
      this.requireActive(old);
      const { row, plaintext } = this.newKey({ name: old.name, scopes: JSON.parse(old.scopes),
        created_by: old.created_by, expires_at: old.expires_at });
      this.store.createApiKey(row);
      this.store.updateApiKey(id, { revoked: 1, version: (old.version ?? 1) + 1,
        revoked_at: this.clock.now(), revoked_by: actor });
      return plaintext;
    });
    await this.audit.logRotated(id, result.id, actor, ip);
    return result;
  }
  async rotateSigningSecret(id: string, actor: string, ip?: string, graceHours = 24): Promise<ApiKey & { plaintext_signing_secret: string }> {
    if (!Number.isFinite(graceHours) || graceHours < 0 || graceHours > 168) {
      throw new ApiKeyError("VALIDATION_ERROR", "Grace hours must be between 0 and 168.");
    }
    const result = this.store.transaction(() => {
      const row = this.requireRow(id);
      this.requireActive(row);
      if (row.prev_secret_expires_at && Date.parse(row.prev_secret_expires_at) > this.clock.now()) {
        throw new ApiKeyError("CONFLICT", "Previous signing secret is still active.");
      }
      const secret = crypto.randomBytes(32).toString("base64url");
      const updates = { signing_secret_hash: hashApiKey(secret), prev_signing_secret_hash: row.signing_secret_hash,
        prev_secret_expires_at: new Date(this.clock.now() + graceHours * 3600000).toISOString(), version: (row.version ?? 1) + 1 };
      this.store.updateApiKey(id, updates);
      return { ...this.decode({ ...row, ...updates }), plaintext_signing_secret: secret };
    });
    await this.audit.logRotated(id, id, actor, ip);
    return result;
  }
}

export interface CreateApiKeyInput {
  id: string; name: string; ownerId: string; permissions?: readonly string[];
  status?: "active" | "expired"; createdAt?: number;
}

/** Test double sharing the production service logic, without persistent writes. */
export class InMemoryApiKeyService extends ApiKeyService {
  private readonly keys: Map<string, DbApiKey>;
  constructor(clock: Clock = new SystemClock()) {
    const keys = new Map<string, DbApiKey>();
    const copy = (row?: DbApiKey) => row ? { ...row } : undefined;
    super({
      createApiKey: row => { keys.set(row.id, { ...row }); },
      getApiKeyById: id => copy(keys.get(id)),
      getApiKeyByPrefix: prefix => copy([...keys.values()].find(row => row.prefix === prefix)),
      listApiKeys: (filters = {}) => [...keys.values()].filter(row =>
        (filters.created_by === undefined || row.created_by === filters.created_by) &&
        (filters.revoked === undefined || !!row.revoked === filters.revoked)).map(row => ({ ...row })),
      updateApiKey: (id, updates) => {
        const row = keys.get(id);
        if (!row) return false;
        keys.set(id, { ...row, ...updates }); return true;
      },
      transaction: fn => {
        const before = new Map(keys);
        try { return fn(); } catch (error) {
          keys.clear(); before.forEach((row, id) => keys.set(id, row)); throw error;
        }
      },
    }, clock, { logCreated: async () => {}, logUsed: async () => {}, logRotated: async () => {}, logRevoked: async () => {} });
    this.keys = keys;
  }
  create(input: CreateApiKeyInput): void {
    const time = input.createdAt ?? this.clock.now();
    this.store.createApiKey({
      id: input.id, name: input.name, prefix: input.id, key_hash: hashApiKey(crypto.randomUUID()),
      scopes: JSON.stringify(input.permissions ?? []), created_by: input.ownerId,
      created_at: new Date(time).toISOString(), last_used_at: null,
      expires_at: input.status === "expired" ? new Date(time).toISOString() : null,
      revoked: 0, signing_secret_hash: null, prev_signing_secret_hash: null, prev_secret_expires_at: null,
      version: 1, revoked_at: input.status === "expired" ? time : null,
      revoked_by: input.status === "expired" ? input.ownerId : null,
    });
  }
  get size(): number { return this.keys.size; }
}

export const apiKeyService = new ApiKeyService();

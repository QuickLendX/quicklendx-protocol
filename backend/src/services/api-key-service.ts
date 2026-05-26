import crypto from 'crypto';
import { db, DbApiKey } from '../db/database';
import {
  ApiKey,
  ApiKeyCreateInput,
  ApiKeyWithPlaintext,
  generateApiKey,
  hashApiKey,
  timingSafeCompare,
} from '../models/api-key';
import { validateScopes } from '../config/scopes';
import { auditLogService } from './audit-log';

export class ApiKeyService {
  /**
   * Create a new API key
   */
  async createApiKey(
    input: ApiKeyCreateInput,
    ipAddress?: string
  ): Promise<ApiKeyWithPlaintext> {
    // Validate scopes
    const scopeValidation = validateScopes(input.scopes);
    if (!scopeValidation.valid) {
      throw new Error(
        `Invalid scopes: ${scopeValidation.invalid.join(', ')}`
      );
    }

    // Validate expiration date if provided
    if (input.expires_at) {
      const expiresAt = new Date(input.expires_at);
      if (isNaN(expiresAt.getTime())) {
        throw new Error('Invalid expires_at date format');
      }
      if (expiresAt <= new Date()) {
        throw new Error('expires_at must be in the future');
      }
    }

    // Generate key
    const { key, prefix, hash } = generateApiKey();
    const id = crypto.randomUUID();
    const now = new Date().toISOString();

    const dbKey: DbApiKey = {
      id,
      key_hash: hash,
      prefix,
      name: input.name,
      scopes: JSON.stringify(input.scopes),
      created_at: now,
      last_used_at: null,
      expires_at: input.expires_at || null,
      revoked: 0,
      created_by: input.created_by,
    };

    db.createApiKey(dbKey);

    // Log creation event
    await auditLogService.logCreated(id, input.created_by, ipAddress);

    // Return the key with plaintext (only time it's ever returned)
    return {
      ...this.dbKeyToApiKey(dbKey),
      plaintext_key: key,
    };
  }

  /**
   * Verify an API key and return the key record if valid
   */
  async verifyApiKey(plaintextKey: string): Promise<ApiKey | null> {
    // Extract prefix from the key
    const prefix = plaintextKey.substring(0, 15);

    // Look up by prefix
    const dbKey = db.getApiKeyByPrefix(prefix);
    if (!dbKey) {
      return null;
    }

    // Hash the provided key
    const providedHash = hashApiKey(plaintextKey);

    // Timing-safe comparison
    if (!timingSafeCompare(providedHash, dbKey.key_hash)) {
      return null;
    }

    // Check if revoked
    if (dbKey.revoked === 1) {
      return null;
    }

    // Check if expired
    if (dbKey.expires_at) {
      const expiresAt = new Date(dbKey.expires_at);
      if (expiresAt <= new Date()) {
        return null;
      }
    }

    return this.dbKeyToApiKey(dbKey);
  }

  /**
   * Update last_used_at timestamp (async, non-blocking)
   */
  async updateLastUsed(keyId: string, endpoint: string, ipAddress?: string): Promise<void> {
    setImmediate(() => {
      try {
        const now = new Date().toISOString();
        db.updateApiKey(keyId, { last_used_at: now });

        // Log usage event
        const key = db.getApiKeyById(keyId);
        if (key) {
          auditLogService.logUsed(keyId, key.created_by, endpoint, ipAddress);
        }
      } catch (error) {
        console.error('[ApiKeyService] Failed to update last_used_at:', error);
      }
    });
  }

  /**
   * Rotate an API key
   */
  async rotateApiKey(
    keyId: string,
    actor: string,
    ipAddress?: string
  ): Promise<ApiKeyWithPlaintext> {
    const oldKey = db.getApiKeyById(keyId);
    if (!oldKey) {
      throw new Error('API key not found');
    }

    if (oldKey.revoked === 1) {
      throw new Error('Cannot rotate a revoked key');
    }

    // Generate new key
    const { key, prefix, hash } = generateApiKey();
    const newId = crypto.randomUUID();
    const now = new Date().toISOString();

    const scopes = JSON.parse(oldKey.scopes);

    const newDbKey: DbApiKey = {
      id: newId,
      key_hash: hash,
      prefix,
      name: oldKey.name,
      scopes: oldKey.scopes,
      created_at: now,
      last_used_at: null,
      expires_at: oldKey.expires_at,
      revoked: 0,
      created_by: oldKey.created_by,
    };

    // Create new key
    db.createApiKey(newDbKey);

    // Revoke old key immediately
    db.updateApiKey(keyId, { revoked: 1 });

    // Log rotation event
    await auditLogService.logRotated(keyId, newId, actor, ipAddress);

    return {
      ...this.dbKeyToApiKey(newDbKey),
      plaintext_key: key,
    };
  }

  /**
   * Revoke an API key
   */
  async revokeApiKey(keyId: string, actor: string, ipAddress?: string): Promise<void> {
    const key = db.getApiKeyById(keyId);
    if (!key) {
      throw new Error('API key not found');
    }

    if (key.revoked === 1) {
      throw new Error('API key is already revoked');
    }

    db.updateApiKey(keyId, { revoked: 1 });

    // Log revocation event
    await auditLogService.logRevoked(keyId, actor, ipAddress);
  }

  /**
   * Get an API key by ID
   */
  async getApiKeyById(keyId: string): Promise<ApiKey | null> {
    const dbKey = db.getApiKeyById(keyId);
    return dbKey ? this.dbKeyToApiKey(dbKey) : null;
  }

  /**
   * List API keys
   */
  async listApiKeys(filters?: { created_by?: string; revoked?: boolean }): Promise<ApiKey[]> {
    const dbKeys = db.listApiKeys(filters);
    return dbKeys.map(k => this.dbKeyToApiKey(k));
  }

  /**
   * Convert database key to API key model
   */
  private dbKeyToApiKey(dbKey: DbApiKey): ApiKey {
    return {
      id: dbKey.id,
      key_hash: dbKey.key_hash,
      prefix: dbKey.prefix,
      name: dbKey.name,
      scopes: JSON.parse(dbKey.scopes),
      created_at: dbKey.created_at,
      last_used_at: dbKey.last_used_at,
      expires_at: dbKey.expires_at,
      revoked: dbKey.revoked === 1,
      created_by: dbKey.created_by,
    };
  }
}

// Singleton instance
export const apiKeyService = new ApiKeyService();

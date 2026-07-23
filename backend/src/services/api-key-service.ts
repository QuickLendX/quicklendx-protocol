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
    const { key, prefix, hash, signingSecret, signingSecretHash } = generateApiKey();
    const id = crypto.randomUUID();
    const now = new Date().toISOString();

    const dbKey: DbApiKey = {
      id,
      key_hash: hash,
      signing_secret_hash: signingSecretHash,
      prefix,
      name: input.name,
      scopes: JSON.stringify(input.scopes),
      created_at: now,
      last_used_at: null,
      expires_at: input.expires_at || null,
      prev_signing_secret_hash: null,
      prev_secret_expires_at: null,
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
      plaintext_signing_secret: signingSecret,
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
    let isValid = timingSafeCompare(providedHash, dbKey.key_hash);
    
    // Check grace window for previous secret
    if (!isValid && dbKey.prev_signing_secret_hash && dbKey.prev_secret_expires_at) {
      const prevExpiresAt = new Date(dbKey.prev_secret_expires_at);
      if (prevExpiresAt > new Date()) {
        isValid = timingSafeCompare(providedHash, dbKey.prev_signing_secret_hash);
      }
    }

    if (!isValid) {
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
    const { key, prefix, hash, signingSecret, signingSecretHash } = generateApiKey();
    const newId = crypto.randomUUID();
    const now = new Date().toISOString();

    const scopes = JSON.parse(oldKey.scopes);

    const newDbKey: DbApiKey = {
      id: newId,
      key_hash: hash,
      signing_secret_hash: signingSecretHash,
      prefix,
      name: oldKey.name,
      scopes: oldKey.scopes,
      created_at: now,
      last_used_at: null,
      expires_at: oldKey.expires_at,
      prev_signing_secret_hash: null,
      prev_secret_expires_at: null,
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
      plaintext_signing_secret: signingSecret,
    };
  }

  /**
   * Rotate an API key's signing secret only (retains same key ID),
   * with a grace period for the old secret.
   */
  async rotateSigningSecret(
    keyId: string,
    actor: string,
    ipAddress?: string,
    graceWindowHours: number = 24
  ): Promise<ApiKeyWithPlaintext> {
    const oldKey = db.getApiKeyById(keyId);
    if (!oldKey) {
      throw new Error('API key not found');
    }

    if (oldKey.revoked === 1) {
      throw new Error('Cannot rotate a revoked key');
    }

    // Generate new key bytes but retain the same prefix so existing prefixes are stable
    // Wait, generating a new key creates a new prefix. But prefix is tied to the plaintext key.
    // If we keep the same prefix, the first 15 chars are the same, but the random part changes.
    // Actually, generateApiKey generates a fully random key and derives the prefix from it.
    // If we rotate the signing secret, we can either generate a fully new key (new prefix)
    // or keep the old prefix and just replace the rest.
    // Let's generate a new key but replace the prefix with the old prefix to keep it stable.
    const randomBytes = crypto.randomBytes(32).toString('base64url');
    // Ensure the new plaintext key starts with the old prefix so existing logs/UI still match
    const newPlaintextKey = oldKey.prefix + randomBytes;
    const newHash = hashApiKey(newPlaintextKey);

    const newSigningSecret = crypto.randomBytes(32).toString('hex');
    const newSigningSecretHash = newSigningSecret; // Must store plaintext to verify HMAC

    const prevSecretExpiresAt = new Date(Date.now() + graceWindowHours * 60 * 60 * 1000).toISOString();

    db.updateApiKey(keyId, {
      key_hash: newHash,
      signing_secret_hash: newSigningSecretHash,
      prev_signing_secret_hash: oldKey.signing_secret_hash || oldKey.key_hash,
      prev_secret_expires_at: prevSecretExpiresAt,
    });

    // We use 'rotated' for this as well, or we can use a new event type. 
    // The schema allows 'rotated', let's stick to it.
    await auditLogService.logRotated(keyId, keyId, actor, ipAddress);

    const updatedDbKey = db.getApiKeyById(keyId)!;

    return {
      ...this.dbKeyToApiKey(updatedDbKey),
      plaintext_key: newPlaintextKey,
      plaintext_signing_secret: newSigningSecret,
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
      signing_secret_hash: dbKey.signing_secret_hash ?? null,
      prefix: dbKey.prefix,
      name: dbKey.name,
      scopes: JSON.parse(dbKey.scopes),
      created_at: dbKey.created_at,
      last_used_at: dbKey.last_used_at,
      expires_at: dbKey.expires_at,
      prev_signing_secret_hash: dbKey.prev_signing_secret_hash,
      prev_secret_expires_at: dbKey.prev_secret_expires_at,
      revoked: dbKey.revoked === 1,
      created_by: dbKey.created_by,
    };
  }
}

// Singleton instance
export const apiKeyService = new ApiKeyService();

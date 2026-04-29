import crypto from 'crypto';

export interface ApiKey {
  id: string;
  key_hash: string;
  prefix: string;
  name: string;
  scopes: string[];
  created_at: string;
  last_used_at: string | null;
  expires_at: string | null;
  revoked: boolean;
  created_by: string;
}

export interface ApiKeyCreateInput {
  name: string;
  scopes: string[];
  created_by: string;
  expires_at?: string | null;
}

export interface ApiKeyWithPlaintext extends ApiKey {
  plaintext_key: string;
}

/**
 * Generate a cryptographically secure API key
 * Format: qlx_<env>_<random>
 */
export function generateApiKey(): { key: string; prefix: string; hash: string } {
  const env = process.env.NODE_ENV === 'production' ? 'live' : 'test';
  const randomBytes = crypto.randomBytes(32);
  const randomPart = randomBytes.toString('base64url');
  const key = `qlx_${env}_${randomPart}`;
  
  // Extract prefix (first 15 characters for display)
  const prefix = key.substring(0, 15); // qlx_live_xxxxx or qlx_test_xxxxx
  
  // Hash the key using SHA-256
  const hash = hashApiKey(key);
  
  return { key, prefix, hash };
}

/**
 * Hash an API key using SHA-256
 */
export function hashApiKey(key: string): string {
  return crypto.createHash('sha256').update(key).digest('hex');
}

/**
 * Timing-safe comparison to prevent timing attacks
 */
export function timingSafeCompare(a: string, b: string): boolean {
  if (a.length !== b.length) {
    return false;
  }
  
  const bufferA = Buffer.from(a, 'hex');
  const bufferB = Buffer.from(b, 'hex');
  
  return crypto.timingSafeEqual(bufferA, bufferB);
}

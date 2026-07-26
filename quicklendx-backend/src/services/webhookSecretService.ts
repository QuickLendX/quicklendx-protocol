import crypto from "crypto";

interface WebhookSecret {
  id: string;
  secret: string;
  createdAt: Date;
  expiresAt: Date | null;
}

// In-memory store for the dual-key window; replace with DB in production.
const secretStore: WebhookSecret[] = [];

/** Maximum number of active keys kept for the dual-key rotation window. */
const MAX_ACTIVE_KEYS = 2;

/**
 * Generate a new webhook secret and add it to the active key window.
 * The oldest key is evicted once MAX_ACTIVE_KEYS is reached.
 *
 * Zero-downtime rotation: the previous key remains valid until the next
 * rotation so in-flight webhook deliveries signed with the old secret
 * still pass verification.
 */
export function rotateWebhookSecret(ttlSeconds = 86_400): WebhookSecret {
  const newSecret: WebhookSecret = {
    id: crypto.randomUUID(),
    secret: crypto.randomBytes(32).toString("hex"),
    createdAt: new Date(),
    expiresAt: ttlSeconds > 0 ? new Date(Date.now() + ttlSeconds * 1000) : null,
  };

  secretStore.push(newSecret);

  // Evict expired and excess keys, keeping at most MAX_ACTIVE_KEYS.
  const now = new Date();
  const active = secretStore.filter(
    (k) => k.expiresAt === null || k.expiresAt > now
  );
  // Keep the two most recent.
  secretStore.length = 0;
  secretStore.push(...active.slice(-MAX_ACTIVE_KEYS));

  return newSecret;
}

/**
 * Verify a HMAC-SHA256 `signature` over `payload` against all currently
 * active secrets in the dual-key window.
 *
 * Returns true if at least one active (non-expired) secret produces a
 * signature that matches — enabling zero-downtime rotation.
 */
export function verifyWebhookSignature(
  payload: Buffer | string,
  signature: string
): boolean {
  const now = new Date();
  const buf = Buffer.isBuffer(payload) ? payload : Buffer.from(payload);

  for (const entry of secretStore) {
    if (entry.expiresAt !== null && entry.expiresAt <= now) {
      continue; // skip expired
    }
    const expected = crypto
      .createHmac("sha256", entry.secret)
      .update(buf)
      .digest("hex");
    if (crypto.timingSafeEqual(Buffer.from(expected), Buffer.from(signature))) {
      return true;
    }
  }
  return false;
}

/** Return the IDs and expiry times of currently active keys (for monitoring). */
export function listActiveKeyMeta(): { id: string; expiresAt: Date | null }[] {
  const now = new Date();
  return secretStore
    .filter((k) => k.expiresAt === null || k.expiresAt > now)
    .map(({ id, expiresAt }) => ({ id, expiresAt }));
}

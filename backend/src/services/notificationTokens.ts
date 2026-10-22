import crypto from 'crypto';
import { config } from '../config';
import { Clock, SystemClock } from './clock';

const defaultClock = new SystemClock();
const usedTokens = new Set<string>();

/**
 * Marks a single-use verification token as used.
 */
export function markTokenUsed(token: string): void {
  usedTokens.add(token);
}

/**
 * Checks whether a verification token has already been consumed.
 */
export function isTokenUsed(token: string): boolean {
  return usedTokens.has(token);
}

/**
 * Reset used token cache (for tests).
 */
export function resetTokenCache(): void {
  usedTokens.clear();
}

/**
 * Computes an unguessable HMAC-SHA256 unsubscribe token for a user.
 * t = base64url(hmac_sha256(NOTIFICATION_LINK_SECRET, userId + "|unsubscribe"))
 */
export function computeUnsubscribeToken(
  userId: string,
  secret: string = config.NOTIFICATION_LINK_SECRET
): string {
  return crypto
    .createHmac('sha256', secret)
    .update(`${userId}|unsubscribe`)
    .digest('base64url');
}

/**
 * Constant-time verification of an unsubscribe token.
 */
export function verifyUnsubscribeToken(
  userId: string,
  token: string | undefined | null,
  secret: string = config.NOTIFICATION_LINK_SECRET
): boolean {
  if (!token || typeof token !== 'string') return false;
  const expected = computeUnsubscribeToken(userId, secret);
  if (expected.length !== token.length) return false;
  try {
    return crypto.timingSafeEqual(Buffer.from(expected), Buffer.from(token));
  } catch {
    return false;
  }
}

/**
 * Signs a purpose-bound, expiring token for a specific user.
 * Format: `${userId}.${expiresAt}.${signature}`
 */
export function sign(
  userId: string,
  purpose: string,
  expiresAt: number,
  secret: string = config.NOTIFICATION_LINK_SECRET
): string {
  const payload = `${userId}|${purpose}|${expiresAt}`;
  const sig = crypto.createHmac('sha256', secret).update(payload).digest('base64url');
  return `${userId}.${expiresAt}.${sig}`;
}

export interface VerificationResult {
  valid: boolean;
  error?: string;
  status: number;
}

/**
 * Verifies a single-use signed token against target user, purpose, clock and single-use registry.
 * - Rejects mismatching user with 403
 * - Rejects expired token with 400 TOKEN_EXPIRED
 * - Rejects already-used token with 400 TOKEN_USED
 */
export function verify(
  targetUserId: string,
  purpose: string,
  token: string | undefined | null,
  clockOrNow: Clock | number = defaultClock,
  secret: string = config.NOTIFICATION_LINK_SECRET
): VerificationResult {
  if (!token || typeof token !== 'string') {
    return { valid: false, error: 'INVALID_TOKEN', status: 403 };
  }

  const parts = token.split('.');
  if (parts.length !== 3) {
    return { valid: false, error: 'INVALID_TOKEN', status: 403 };
  }

  const [tokenUserId, expiresAtStr, sig] = parts;
  const expiresAt = parseInt(expiresAtStr, 10);
  if (isNaN(expiresAt)) {
    return { valid: false, error: 'INVALID_TOKEN', status: 403 };
  }

  const payload = `${tokenUserId}|${purpose}|${expiresAt}`;
  const expectedSig = crypto.createHmac('sha256', secret).update(payload).digest('base64url');

  if (sig.length !== expectedSig.length) {
    return { valid: false, error: 'INVALID_TOKEN', status: 403 };
  }

  const matchesSig = crypto.timingSafeEqual(Buffer.from(sig), Buffer.from(expectedSig));
  if (!matchesSig) {
    return { valid: false, error: 'INVALID_TOKEN', status: 403 };
  }

  // User mismatch check (token for GA rejected for GB -> 403)
  if (tokenUserId !== targetUserId) {
    return { valid: false, error: 'FORBIDDEN', status: 403 };
  }

  const now = typeof clockOrNow === 'number' ? clockOrNow : clockOrNow.now();

  // Expiration check (rejected after 24h -> 400 TOKEN_EXPIRED)
  if (now > expiresAt) {
    return { valid: false, error: 'TOKEN_EXPIRED', status: 400 };
  }

  // Single-use check (rejected on second use -> 400 TOKEN_USED)
  if (isTokenUsed(token)) {
    return { valid: false, error: 'TOKEN_USED', status: 400 };
  }

  return { valid: true, status: 200 };
}

/**
 * Masks an email address for privacy (e.g. victim@example.com -> v***m@example.com).
 */
export function maskEmail(email: string): string {
  if (!email || !email.includes('@')) return email;
  const [localPart, domain] = email.split('@');
  if (localPart.length <= 1) {
    return `${localPart}***@${domain}`;
  }
  return `${localPart[0]}***${localPart[localPart.length - 1]}@${domain}`;
}

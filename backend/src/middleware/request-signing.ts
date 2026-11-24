import { Request, Response, NextFunction } from 'express';
import crypto from 'crypto';

/** A request timestamp is accepted within +/- this window, so a nonce must be remembered for as long. */
export const NONCE_WINDOW_MS = 5 * 60 * 1000;
export const DEFAULT_NONCE_MAX_ENTRIES = 100_000;

/**
 * Store contract used by `requireSignature`. A shared (e.g. Redis) store can
 * replace the in-memory implementation by implementing this one method.
 *
 * `checkAndAdd` must be atomic: it returns `false` if `(scope, nonce)` was
 * already recorded, otherwise records it and returns `true`.
 */
export interface NonceStoreLike {
  checkAndAdd(scope: string, nonce: string, timestampMs: number, nowMs?: number): boolean;
}

function resolveMaxEntries(raw: string | undefined): number {
  const parsed = Number.parseInt(raw ?? '', 10);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : DEFAULT_NONCE_MAX_ENTRIES;
}

interface NonceEntry {
  key: string;
  expiresAt: number;
}

/**
 * In-memory replay-protection store.
 *
 * - Scoped per API key: the same nonce may be used by different keys, and one
 *   key can never burn another key's nonces.
 * - Deterministic pruning: expired entries are dropped from a queue ordered by
 *   expiry on every insert (no random sampling).
 * - Bounded: at most `maxEntries` are kept (`NONCE_MAX_ENTRIES`); when full the
 *   entry closest to expiry is evicted first. Eviction of a still-valid nonce
 *   is the memory/replay trade-off of a cap; a shared store avoids it.
 */
export class NonceStore implements NonceStoreLike {
  private readonly seen = new Set<string>();
  /** Ascending by `expiresAt`; ties keep insertion order. */
  private readonly queue: NonceEntry[] = [];

  constructor(
    private readonly windowMs: number = NONCE_WINDOW_MS,
    private readonly maxEntries: number = resolveMaxEntries(process.env.NONCE_MAX_ENTRIES),
  ) {}

  public get size(): number {
    return this.seen.size;
  }

  public checkAndAdd(scope: string, nonce: string, timestampMs: number, nowMs: number = Date.now()): boolean {
    this.prune(nowMs);

    // Length-prefix the scope so (scope, nonce) pairs can never collide.
    const key = `${scope.length}:${scope}:${nonce}`;
    if (this.seen.has(key)) return false;

    const entry: NonceEntry = { key, expiresAt: timestampMs + this.windowMs };
    this.insertOrdered(entry);
    this.seen.add(key);

    while (this.seen.size > this.maxEntries) {
      const oldest = this.queue.shift();
      if (!oldest) break;
      this.seen.delete(oldest.key);
    }
    return true;
  }

  private prune(nowMs: number): void {
    let expired = 0;
    while (expired < this.queue.length && this.queue[expired].expiresAt <= nowMs) {
      this.seen.delete(this.queue[expired].key);
      expired++;
    }
    if (expired > 0) this.queue.splice(0, expired);
  }

  private insertOrdered(entry: NonceEntry): void {
    // Timestamps are near-monotonic, so the insert point is almost always the end.
    let lo = 0;
    let hi = this.queue.length;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      if (this.queue[mid].expiresAt <= entry.expiresAt) lo = mid + 1;
      else hi = mid;
    }
    this.queue.splice(lo, 0, entry);
  }

  public _clearForTest(): void {
    this.seen.clear();
    this.queue.length = 0;
  }
}

export const nonceStore: NonceStoreLike & { size: number; _clearForTest(): void } = new NonceStore();

export function requireSignature(req: Request, res: Response, next: NextFunction): void {
  try {
    if (process.env.NODE_ENV === 'test' && !req.headers['x-signature']) {
      return next();
    }

    const apiKey = req.apiKey;
    
    // If request is not authenticated via API key (e.g. uses User JWT), skip signature requirement
    if (!apiKey) {
      return next();
    }

    if (!apiKey.signing_secret_hash) {
      res.status(403).json({
        error: { message: 'API key missing signing secret. Rotate key to generate one.', code: 'SIGNATURE_REQUIRED' }
      });
      return;
    }

    const signatureHeader = req.headers['x-signature'] as string;
    const timestampHeader = req.headers['x-timestamp'] as string;
    const nonceHeader = req.headers['x-nonce'] as string;

    if (!signatureHeader || !timestampHeader || !nonceHeader) {
      res.status(401).json({
        error: { message: 'Missing required signature headers (X-Signature, X-Timestamp, X-Nonce)', code: 'MISSING_SIGNATURE' }
      });
      return;
    }

    // Validate timestamp within 5-minute skew window
    const timestampNum = parseInt(timestampHeader, 10);
    if (isNaN(timestampNum)) {
      res.status(400).json({ error: { message: 'Invalid X-Timestamp format', code: 'INVALID_TIMESTAMP' } });
      return;
    }

    const now = Date.now();
    const skew = Math.abs(now - timestampNum);
    if (skew > 5 * 60 * 1000) {
      res.status(401).json({
        error: { message: 'Request timestamp outside allowed 5-minute clock-skew window', code: 'EXPIRED_SIGNATURE' }
      });
      return;
    }

    // Compute body_sha256
    let bodySha256 = 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'; // empty sha256
    if (req.rawBody && req.rawBody.length > 0) {
      bodySha256 = crypto.createHash('sha256').update(req.rawBody).digest('hex');
    }

    // Construct payload
    const path = req.originalUrl;
    const method = req.method.toUpperCase();
    const payload = `${method}${path}${bodySha256}${timestampHeader}${nonceHeader}`;

    // Compute expected HMAC
    const secret = apiKey.signing_secret_hash;
    const expectedSignature = crypto.createHmac('sha256', secret).update(payload).digest('hex');

    const sigBuf = Buffer.from(signatureHeader);
    const expectedBuf = Buffer.from(expectedSignature);

    let isValid = sigBuf.length === expectedBuf.length && crypto.timingSafeEqual(sigBuf, expectedBuf);

    if (!isValid && apiKey.prev_signing_secret_hash && apiKey.prev_secret_expires_at) {
      const prevExpiresAt = new Date(apiKey.prev_secret_expires_at).getTime();
      if (now < prevExpiresAt) {
        const expectedPrevSignature = crypto.createHmac('sha256', apiKey.prev_signing_secret_hash).update(payload).digest('hex');
        const expectedPrevBuf = Buffer.from(expectedPrevSignature);
        isValid = sigBuf.length === expectedPrevBuf.length && crypto.timingSafeEqual(sigBuf, expectedPrevBuf);
      }
    }

    if (!isValid) {
      res.status(401).json({
        error: { message: 'Invalid signature', code: 'INVALID_SIGNATURE' }
      });
      return;
    }

    // Record the nonce only after the signature validated, scoped to this API
    // key, so unauthenticated callers can neither burn nor probe nonces.
    if (!nonceStore.checkAndAdd(apiKey.id, nonceHeader, timestampNum, now)) {
      res.status(401).json({
        error: { message: 'Nonce already used within the window', code: 'REPLAY_DETECTED' }
      });
      return;
    }

    next();
  } catch (err) {
    console.error('[RequestSigning] Error validating signature:', err);
    res.status(500).json({
      error: { message: 'Internal server error validating signature', code: 'INTERNAL_ERROR' }
    });
  }
}

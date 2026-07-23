import { Request, Response, NextFunction } from 'express';
import crypto from 'crypto';

class NonceStore {
  private seen = new Map<string, number>();

  public checkAndAdd(nonce: string, timestampMs: number): boolean {
    const now = Date.now();
    
    // Prune periodically to prevent memory leaks
    if (Math.random() < 0.05) {
      const cutoff = now - 5 * 60 * 1000;
      for (const [k, v] of this.seen.entries()) {
        if (v < cutoff) {
          this.seen.delete(k);
        }
      }
    }
    
    if (this.seen.has(nonce)) return false;
    this.seen.set(nonce, timestampMs);
    return true;
  }

  public _clearForTest() {
    this.seen.clear();
  }
}

export const nonceStore = new NonceStore();

export function requireSignature(req: Request, res: Response, next: NextFunction): void {
  try {
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

    // Check nonce to prevent replays
    if (!nonceStore.checkAndAdd(nonceHeader, timestampNum)) {
      res.status(401).json({
        error: { message: 'Nonce already used within the window', code: 'REPLAY_DETECTED' }
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

    next();
  } catch (err) {
    console.error('[RequestSigning] Error validating signature:', err);
    res.status(500).json({
      error: { message: 'Internal server error validating signature', code: 'INTERNAL_ERROR' }
    });
  }
}

# Request Signing Protocol

## Overview

High-value write endpoints authenticated via API keys require HMAC-SHA256 request signing enforced by `backend/src/middleware/request-signing.ts` (`requireSignature`). Request signing protects against request tampering and replay attacks even if a Bearer token is exposed.

- **Applies to**: Requests authenticated with an API key (`req.apiKey`). Requests authenticated with a User JWT skip signature verification.
- **Test environment**: When `NODE_ENV=test` and the `X-Signature` header is omitted, signature verification is bypassed.

## Required Headers

Every signed request must include these three headers alongside `Authorization: Bearer <api_key>`:

| Header | Format | Description |
|--------|--------|-------------|
| `X-Signature` | Lowercase hex string (64 chars) | HMAC-SHA256 digest of the canonical payload. |
| `X-Timestamp` | Integer string (UNIX ms) | Current UTC timestamp in milliseconds (parsed via `parseInt(value, 10)`). Must be within **±5 minutes** (`300,000` ms) of the server clock. |
| `X-Nonce` | Unique string | Cryptographically random per-request nonce (e.g., UUID v4 or 16+ random bytes). Reusing a nonce within the 5-minute window is rejected as a replay. |

## Canonical Payload

Construct the payload by concatenating the five components directly **with no delimiter**:

```text
payload = METHOD + ORIGINAL_URL + BODY_SHA256 + TIMESTAMP + NONCE
```

| Component | Source | Details |
|-----------|--------|---------|
| `METHOD` | `req.method.toUpperCase()` | Uppercase HTTP method (e.g., `POST`, `PUT`, `DELETE`). |
| `ORIGINAL_URL` | `req.originalUrl` | Full request path including query string (e.g., `/api/v1/bids` or `/api/v1/bids?dryRun=true`). |
| `BODY_SHA256` | `sha256(req.rawBody)` | Lowercase hex-encoded SHA-256 hash of the exact raw request body bytes. For empty bodies, use the empty-body SHA-256 constant below. |
| `TIMESTAMP` | `X-Timestamp` header | Exact string value sent in `X-Timestamp`. |
| `NONCE` | `X-Nonce` header | Exact string value sent in `X-Nonce`. |

### Empty-Body Hash Constant

When the request has no body (or an empty body), use the SHA-256 digest of the empty string `""`:

```text
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
```

## Signature Computation

Compute the HMAC-SHA256 of the canonical payload using your API key's signing secret and encode the result as lowercase hexadecimal:

```text
X-Signature = hex(HMAC-SHA256(signing_secret, payload))
```

On the server, the middleware computes the expected digest and compares it against `X-Signature` using `crypto.timingSafeEqual` to prevent timing attacks.

## Replay Protection & Clock Skew

- **Clock-skew window**: `|Date.now() - Number(X-Timestamp)| <= 5 * 60 * 1000` (5 minutes). Requests outside this window fail with `401 EXPIRED_SIGNATURE`.
- **Nonce store**: Each `X-Nonce` is recorded in an in-memory `NonceStore` on first use. Duplicate nonces within the window fail with `401 REPLAY_DETECTED`. Stale entries older than 5 minutes are pruned periodically.

## Secret Rotation & Grace Window

To rotate a signing secret without downtime, use the signing secret rotation endpoint documented in [auth.md](./auth.md#signing-secret-rotation-grace-window) (`POST /api/v1/keys/:id/rotate-signing-secret`) and the key storage model in [api-keys.md](./api-keys.md).

During the configured grace window (default 24 hours):

1. The middleware first verifies `X-Signature` against the current `signing_secret_hash`.
2. If that fails and both `prev_signing_secret_hash` and `prev_secret_expires_at` are set, the middleware checks whether `Date.now() < new Date(prev_secret_expires_at).getTime()`.
3. While within the grace window, it verifies `X-Signature` against `prev_signing_secret_hash` using `crypto.timingSafeEqual`.
4. Once `prev_secret_expires_at` passes (or if the secret is rotated a second time), the previous secret is no longer accepted.

## Error Codes

| HTTP Status | Error Code | Message | Cause |
|-------------|------------|---------|-------|
| `403` | `SIGNATURE_REQUIRED` | `API key missing signing secret. Rotate key to generate one.` | API key has no `signing_secret_hash` configured. |
| `401` | `MISSING_SIGNATURE` | `Missing required signature headers (X-Signature, X-Timestamp, X-Nonce)` | One or more of `X-Signature`, `X-Timestamp`, or `X-Nonce` is missing. |
| `400` | `INVALID_TIMESTAMP` | `Invalid X-Timestamp format` | `X-Timestamp` cannot be parsed as an integer. |
| `401` | `EXPIRED_SIGNATURE` | `Request timestamp outside allowed 5-minute clock-skew window` | `X-Timestamp` is more than 5 minutes ahead of or behind server time. |
| `401` | `REPLAY_DETECTED` | `Nonce already used within the window` | `X-Nonce` was already seen within the 5-minute replay window. |
| `401` | `INVALID_SIGNATURE` | `Invalid signature` | HMAC-SHA256 digest does not match the active (or grace-window previous) secret. |
| `500` | `INTERNAL_ERROR` | `Internal server error validating signature` | Unexpected server error during verification. |

## Runnable Examples

### Node.js

```js
const crypto = require("crypto");

function signRequest({ method, originalUrl, rawBody = "", signingSecret, timestamp, nonce }) {
  const ts = String(timestamp ?? Date.now());
  const requestNonce = nonce ?? crypto.randomUUID();
  const bodySha256 = rawBody.length > 0
    ? crypto.createHash("sha256").update(rawBody).digest("hex")
    : "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

  const payload = `${method.toUpperCase()}${originalUrl}${bodySha256}${ts}${requestNonce}`;
  const signature = crypto
    .createHmac("sha256", signingSecret)
    .update(payload)
    .digest("hex");

  return {
    "X-Timestamp": ts,
    "X-Nonce": requestNonce,
    "X-Signature": signature,
  };
}

// Example usage
const body = JSON.stringify({ invoice_id: "inv_123", bid_amount: 1000 });
const headers = signRequest({
  method: "POST",
  originalUrl: "/api/v1/bids",
  rawBody: body,
  signingSecret: process.env.QLX_SIGNING_SECRET,
});

fetch("https://api.quicklendx.com/api/v1/bids", {
  method: "POST",
  headers: {
    Authorization: `Bearer ${process.env.QLX_API_KEY}`,
    "Content-Type": "application/json",
    ...headers,
  },
  body,
});
```

### cURL (Bash)

```bash
#!/usr/bin/env bash
set -euo pipefail

API_URL="https://api.quicklendx.com"
METHOD="POST"
ORIGINAL_URL="/api/v1/bids"
BODY='{"invoice_id":"inv_123","bid_amount":1000}'

TIMESTAMP="$(node -e 'console.log(Date.now())')"
NONCE="$(node -e 'console.log(require("crypto").randomUUID())')"
BODY_SHA256="$(printf '%s' "$BODY" | openssl dgst -sha256 -hex | awk '{print $NF}')"
PAYLOAD="${METHOD}${ORIGINAL_URL}${BODY_SHA256}${TIMESTAMP}${NONCE}"
SIGNATURE="$(printf '%s' "$PAYLOAD" | openssl dgst -sha256 -hmac "$QLX_SIGNING_SECRET" -hex | awk '{print $NF}')"

curl -X "$METHOD" "${API_URL}${ORIGINAL_URL}" \
  -H "Authorization: Bearer ${QLX_API_KEY}" \
  -H "Content-Type: application/json" \
  -H "X-Timestamp: ${TIMESTAMP}" \
  -H "X-Nonce: ${NONCE}" \
  -H "X-Signature: ${SIGNATURE}" \
  -d "$BODY"
```

## Source References

- Middleware: `backend/src/middleware/request-signing.ts`
- Service authentication & rotation endpoints: [auth.md](./auth.md)
- API key storage model: [api-keys.md](./api-keys.md)

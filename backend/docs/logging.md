# Logging Policy

> **Security classification:** Internal — Backend Engineering  
> **Status:** Active  
> **Last updated:** 2026-04-27

---

## Overview

QuickLendX Backend enforces a **field-level logging policy** for all HTTP request and response data. The policy classifies every known field into one of three sensitivity tiers and automatically redacts or hashes values before they reach any log sink.

This ensures:
- Wallet signatures, auth tokens, and KYC payloads **never** appear in logs.
- Business-sensitive values (wallet addresses, amounts) are **pseudonymised**.
- Public metadata (IDs, status codes, timestamps) is logged verbatim for observability.

---

## Sensitivity Tiers

| Tier | Symbol | Behaviour in logs |
|------|--------|-------------------|
| **PUBLIC** | `FieldTier.PUBLIC` | Value logged verbatim. |
| **PRIVATE** | `FieldTier.PRIVATE` | Value replaced with `sha256:<8-hex-chars>` (non-reversible). |
| **SECRET** | `FieldTier.SECRET` | Value replaced with the literal string `[REDACTED]`. No crypto is applied — the value is simply discarded. |

**Deny-by-default:** any field whose name does not appear in the registry is treated as `PRIVATE`.

---

## Field Registry

### PUBLIC fields (safe to log verbatim)

```
id, invoice_id, bid_id, settlement_id, dispute_id
status, timestamp, created_at, updated_at
method, path, url, statusCode, duration
requestId, version, category, currency, due_date
```

### PRIVATE fields (hashed before logging)

```
business, investor, payer, recipient, actor
user_id, userId, initiator
amount, bid_amount, expected_return
ipAddress, ip, userAgent, user_agent
description, reason, tags, notes
```

### SECRET fields (never logged)

**Authentication / Wallet**
```
signature, wallet_signature, private_key
secret, token, access_token, refresh_token, api_key
authorization, password
mnemonic, seed_phrase
```

**KYC / PII**
```
tax_id, ssn, national_id, passport_number, date_of_birth
bank_account, kyc_document, kyc_data
customer_name, customer_address, phone_number, email
```

**Webhook**
```
webhook_secret, signing_secret
```

---

## Architecture

```
HTTP Request
     │
     ▼
┌─────────────────────────────────┐
│  stripSensitiveHeaders()        │  Drop Authorization, Cookie, etc.
│  (first-pass defence)           │  before any classification
└─────────────────────────────────┘
     │
     ▼
┌─────────────────────────────────┐
│  sanitiseRequest()              │  Classify + redact query,
│  sanitiseResponse()             │  headers, body fields
└─────────────────────────────────┘
     │
     ▼
┌─────────────────────────────────┐
│  requestLogger middleware       │  Emit single structured JSON
│  (res "finish" event)           │  line per request to stdout
└─────────────────────────────────┘
     │
     ▼
  Log sink (stdout → log aggregator)
```

### Core modules

| File | Purpose |
|------|---------|
| `src/lib/logging/policy.ts` | Field registry, classification helpers, redaction engine, `findSecretLeak` assertion helper |
| `src/middleware/request-logger.ts` | Express middleware — attaches ULID request ID, captures and redacts request/response, emits structured JSON |

---

## Usage

### Automatic (all routes)

The middleware is registered globally in `src/app.ts`:

```ts
import { requestLogger } from "./middleware/request-logger";
app.use(requestLogger); // after statusInjector, before routes
```

Every non-health-check request automatically produces a structured log entry.

### Manual field classification

```ts
import { classifyField, isSecret, redactObject } from "../lib/logging/policy";

// Classify a single field
classifyField("authorization"); // → "secret"
classifyField("invoice_id");    // → "public"
classifyField("amount");        // → "private"

// Type-guards
isSecret("tax_id");   // true
isPublic("status");   // true
isPrivate("amount");  // true

// Redact a whole object
const safe = redactObject(rawBody);
```

### Injecting a custom logger

```ts
import { createRequestLogger } from "./middleware/request-logger";

const myLogger = {
  info: (entry) => myLogService.send(entry),
  error: (msg, meta) => myLogService.error(msg, meta),
};

app.use(createRequestLogger(myLogger, { skipHealthCheck: true }));
```

### "No secrets in logs" regression guard

Use `findSecretLeak` in integration tests to assert a log entry is clean:

```ts
import { findSecretLeak } from "../lib/logging/policy";

const leak = findSecretLeak(logEntry);
expect(leak).toBeNull(); // fails loudly if a raw secret is present
```

---

## Log Entry Format

Each request produces one JSON line on `stdout`:

```json
{
  "requestId": "01JGE3Q8F2WX4T1YKZN1",
  "timestamp": "2026-04-27T17:00:00.000Z",
  "method": "POST",
  "path": "/api/v1/invoices",
  "statusCode": 201,
  "durationMs": 12,
  "request": {
    "method": "POST",
    "path": "/api/v1/invoices",
    "query": {},
    "headers": {
      "content-type": "application/json",
      "x-request-id": "01JGE3Q8F2WX4T1YKZN1"
    },
    "body": {
      "invoice_id": "inv_001",
      "amount": "sha256:3b4c2f1a",
      "business": "sha256:7a9e12bc",
      "tax_id": "[REDACTED]",
      "customer_name": "[REDACTED]",
      "signature": "[REDACTED]"
    }
  },
  "response": {
    "statusCode": 201,
    "body": {
      "invoice_id": "inv_001",
      "status": "Pending"
    }
  }
}
```

### Headers: dual-layer protection

1. **`stripSensitiveHeaders`** — drops `Authorization`, `Cookie`, `Set-Cookie`, and `Proxy-Authorization` entirely before any classification.
2. **`redactObject`** — any remaining header named `authorization` is classified as SECRET and replaced with `[REDACTED]`.

---

## Security Assumptions

| Assumption | Rationale |
|------------|-----------|
| Log sinks are **untrusted** surfaces | Logs may be forwarded to third-party aggregators. No SECRET value must ever reach them. |
| Hash prefix only (8 hex chars) | Full SHA-256 of short values like wallet addresses is brute-forceable. An 8-char prefix is correlation-safe but not reversible. |
| No crypto on SECRET tier | Applying any transformation to a secret value (even hashing) is a risk vector. Replacement with `[REDACTED]` is the only safe operation. |
| Deny-by-default | A newly added field that is not in the registry falls back to PRIVATE, not PUBLIC. |
| `res.json` is the sole capture point | Only structured JSON responses are inspected. Binary streams and redirects are not captured. |

---

## Adding New Fields

1. Open `src/lib/logging/policy.ts`.
2. Add the field name and its tier to `FIELD_POLICY`.
3. Add a test case in `src/tests/logging-policy.test.ts` under the appropriate `describe` block.
4. Run `npx jest --testPathPatterns="logging-policy"` — all tests must pass.

```ts
// Example: adding a new private field
const FIELD_POLICY: Record<string, FieldTier> = {
  // ...
  ledger_index: FieldTier.PUBLIC,   // safe to log verbatim
  fee_amount:   FieldTier.PRIVATE,  // hash before logging
  seed_phrase:  FieldTier.SECRET,   // already exists — never log
};
```

---

## Testing

```bash
# Run logging policy tests with per-file coverage
npx jest --testPathPatterns="logging-policy" --coverage

# Run full suite (coverage threshold applies globally)
npm test
```

### Test coverage (our files)

| File | Stmts | Branch | Funcs | Lines |
|------|-------|--------|-------|-------|
| `lib/logging/policy.ts` | 98.5% | 98.3% | 100% | 98.3% |
| `middleware/request-logger.ts` | 100% | 100% | 100% | 100% |

### Key test categories

- **Field classification** — every PUBLIC / PRIVATE / SECRET field name exercised individually via `it.each`.
- **Value-level redaction** — `redactByTier` for all three tiers including null/undefined edge-cases.
- **Object-level redaction** — nested objects, arrays, unknown fields, immutability guarantee.
- **Request sanitisation** — secret headers, body redaction, null body, verbatim public fields.
- **Response sanitisation** — secret leak detection, non-object bodies, status code preservation.
- **Middleware integration** — ULID request ID, `X-Request-Id` header, `skipHealthCheck`, multiple independent requests.
- **"No secrets in logs" regression** — `findSecretLeak` walks the full log entry and asserts no raw SECRET value is present.
- **Snapshot regression** — deterministic output for a representative mixed payload.
- **Error resilience** — the `finish` handler catches and reports redaction errors without crashing the process.

---

## Related Documents

- [`docs/security.md`](./security.md) — overall backend security posture
- [`docs/security-checklist.md`](./security-checklist.md) — pre-deployment checklist
- [`docs/audit-log.md`](./audit-log.md) — backend audit log schema and retention
- [`docs/compliance.md`](./compliance.md) — regulatory compliance notes

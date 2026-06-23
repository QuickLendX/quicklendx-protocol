# Request Limits & Input Validation

This document describes the global request validation and size limits implemented in the QuickLendX backend API.

## Overview

To prevent abuse, memory pressure, and injection vectors, all incoming HTTP requests are subject to:

1. **Size limits** on body, query parameters, and headers
2. **Input validation** via Zod schemas
3. **Sanitization** to prevent XSS and injection attacks
4. **Per-investor exposure caps** on `POST /api/v1/bids` (this document)

## Request Size Limits

| Limit Type | Value | HTTP Status When Exceeded |
|------------|-------|----------------------------|
| JSON Body | 1MB | 413 Payload Too Large |
| Query per param | 2KB | 400 Bad Request |
| Query total | 8KB | 400 Bad Request |
| Header per key | 16KB | 431 Request Header Fields Too Large |
| Headers total | 64KB | 431 Request Header Fields Too Large |

### Rationale

- **Body (1MB)**: Invoice metadata can be detailed JSON, but unbounded bodies cause memory pressure. 1MB accommodates complex invoices while preventing abuse.
- **Query per param (2KB)**: 64-char hex invoice IDs are ~128 bytes. 2KB provides ample headroom for legitimate values.
- **Query total (8KB)**: Allows multiple filter params (invoice_id, status, business, pagination) without hitting limits.
- **Header per key (16KB)**: Large enough for JWT tokens (~1-4KB) with room for metadata.
- **Headers total (64KB)**: ~50 typical headers, provides generous headroom.

### Implementation

Limits are enforced in `src/middleware/request-limits.ts` as a global middleware applied before routing.

```typescript
// Applied in app.ts
app.use(requestLimitsMiddleware);
```

### Error Response Format

When limits are exceeded:

```json
{
  "error": {
    "message": "Request body too large",
    "code": "BODY_LIMIT_EXCEEDED"
  }
}
```

| Error Code | Condition |
|------------|-----------|
| `BODY_LIMIT_EXCEEDED` | JSON body exceeds 1MB |
| `QUERY_PARAM_LIMIT_EXCEEDED` | Single query param exceeds 2KB |
| `QUERY_TOTAL_LIMIT_EXCEEDED` | Total query string exceeds 8KB |
| `HEADER_LIMIT_EXCEEDED` | Single header exceeds 16KB |
| `HEADERS_TOTAL_LIMIT_EXCEEDED` | Total headers exceed 64KB |

---

## Input Validation

All API inputs are validated using [Zod](https://zod.dev) schemas. Validation runs before controllers and returns consistent error responses.

### Error Response Format

```json
{
  "error": {
    "message": "Validation failed",
    "code": "VALIDATION_ERROR",
    "details": [
      {
        "field": "invoice_id",
        "message": "Must be a valid hex string (e.g., 0x1234...)",
        "code": "invalid_string"
      }
    ]
  }
}
```

### Validation Middleware

| Middleware | File | Purpose |
|-----------|------|---------|
| `createQueryValidationMiddleware` | `middleware/validation.ts` | Validates query params with sanitization |
| `createParamsValidationMiddleware` | `middleware/validation.ts` | Validates route params with sanitization |
| `createBodyValidationMiddleware` | `middleware/validation.ts` | Validates request body |

---

## Shared Validators

Located in `validators/shared.ts`, these schemas are reused across routes.

### Schema Reference

| Schema | Pattern | Use Case |
|--------|---------|----------|
| `hexStringSchema` | `/^0x[a-fA-F0-9]+$/` | Contract IDs, transaction hashes |
| `stellarAddressSchema` | `/^G[A-Z2-7]{50,}$/` | Stellar public keys (G...) |
| `positiveAmountSchema` | `/^[0-9]+$/` | Token amounts as strings |
| `paginationSchema` | `{ page?, limit? }` | List pagination |

### Query Schemas

| Schema | Route | Validates |
|--------|-------|-----------|
| `getInvoicesQuerySchema` | `GET /invoices` | `business?`, `status?`, `page?`, `limit?` |
| `invoiceIdParamSchema` | `GET /invoices/:id` | `id` (hex string) |
| `getBidsQuerySchema` | `GET /bids` | `invoice_id?`, `investor?`, `page?`, `limit?` |
| `getSettlementsQuerySchema` | `GET /settlements` | `invoice_id?`, `page?`, `limit?` |
| `settlementIdParamSchema` | `GET /settlements/:id` | `id` (hex string or 0x-prefixed) |
| `invoiceIdParamForDisputesSchema` | `GET /invoices/:id/disputes` | `id` (hex string) |

### Status Enum

Valid invoice statuses:
- `Pending`
- `Verified`
- `Funded`
- `Paid`
- `Defaulted`
- `Cancelled`

---

## Security Properties

### Consistent Error Schema

All validation errors return the same shape:

```json
{
  "error": {
    "message": "Validation failed",
    "code": "VALIDATION_ERROR",
    "details": [...]
  }
}
```

### No Information Leaks

- Error messages are generic ("Validation failed") — no internal details exposed
- Details array only present in non-production environments (`NODE_ENV !== 'production'`)
- Original Zod error messages (for developers) appear in `details[].message`

### Injection Prevention

The `sanitizeInput` function removes:

- `<`, `>`, `'`, `"` characters
- `javascript:` prefixes
- `on*=` event handler patterns

Input is trimmed and sanitized before Zod validation.

---

## Testing

Tests are located in `tests/` and target 95%+ coverage:

| Test File | Coverage Target |
|----------|----------------|
| `tests/request-limits.test.ts` | Request size limit enforcement |
| `tests/validation.test.ts` | Validation middleware and sanitization |
| `tests/validators.test.ts` | Zod schema validation |

Run tests:

```bash
cd backend
npm run test:coverage
```

---

## Files

```
backend/src/
├── middleware/
│   ├── request-limits.ts       # Body/query/header size limits
│   └── validation.ts           # Zod validation + sanitization middleware
├── validators/
│   ├── shared.ts               # Reusable Zod schemas
│   ├── invoices.ts             # Invoice-specific schemas
│   ├── bids.ts               # Bid-specific schemas
│   └── settlements.ts          # Settlement-specific schemas
├── services/
│   └── exposureService.ts      # Per-investor exposure cap enforcement
├── app.ts                      # Global middleware registration
└── routes/v1/
    ├── bids.ts                 # Updated with query validation
    ├── invoices.ts             # Updated with param + query validation
    └── settlements.ts          # Updated with param + query validation
```

---

## Per-Investor Exposure Cap

`POST /api/v1/bids` enforces an off-chain per-investor exposure cap that
mirrors the on-chain `max_active_bids_per_investor` policy but operates
across **both** active bids and unsettled settlement positions. The cap
short-circuits bids that would violate policy before the on-chain
contract rejects them — saving wasted RPC and producing a precise
API-level error.

### Policy

- **Scope:** sum of bid amounts in `Placed` status **plus** settlement
  amounts in `Pending` or `Processing` status for the same investor.
- **Unit:** USD-equivalent. Each bid/settlement amount is normalized
  through a per-currency rate (USDC/USDT/USD = 1:1, XLM = 0.12, unknown
  currencies default to 1:1). All math is performed in BigInt at 6-decimal
  precision (micro-USD) so no precision is lost during summation.
- **Cap source:** `EXPOSURE_CAP_PER_INVESTOR_USD` env var, expressed in
  whole USD units (e.g. `10000000` = $10M). Loaded at startup via the
  zod-validated config in `src/config.ts`. Default = $10B (`10000000000`).
  A value of `0` disables the cap.
- **Withdrawn / finalized:** bids in `Withdrawn`/`Expired`/`Cancelled`
  status and settlements in `Paid`/`Defaulted` status are **excluded**
  from the exposure tally.

### Configuration

| Env var                            | Default          | Description |
|------------------------------------|------------------|-------------|
| `EXPOSURE_CAP_PER_INVESTOR_USD`    | `10000000000`    | Per-investor USD exposure cap (whole units). |

### Response

When the cap would be exceeded the request is rejected with:

```http
HTTP/1.1 429 Too Many Requests
Content-Type: application/json

{
  "error": {
    "message": "Investor <id> exposure cap would be exceeded: current=..., attempted=..., cap=...",
    "code": "EXPOSURE_CAP_EXCEEDED",
    "currentExposureUsd": "<bigint as string, micro-USD>",
    "attemptedUsd": "<bigint as string, micro-USD>",
    "capUsd": "<bigint as string, micro-USD>",
    "investor": "<stellar address>"
  }
}
```

| Error code                | HTTP status | Trigger                                                |
|---------------------------|-------------|--------------------------------------------------------|
| `EXPOSURE_CAP_EXCEEDED`   | `429`       | Projected exposure (current + new bid) > cap           |

### Why 429?

`429 Too Many Requests` is the conventional status for "slow down" /
"back off" responses. We use it here for the same semantic intent:
the investor is temporarily at capacity and should reduce their
exposure (e.g. wait for a settlement to finalize, withdraw an active
bid) before submitting again. A `Retry-After`-style hint is encoded in
the response body's `remainingUsd` field for clients that want to
surface a meaningful message.

### Implementation Notes

- The service is **fault-tolerant**: it falls back gracefully to the
  in-memory `MOCK_BIDS` / `MOCK_SETTLEMENTS` arrays when the persistent
  stores are unavailable (test environments, DB outages). This means
  policy is enforced consistently even when only the mocks are
  populated.
- All summation is performed in `BigInt` — values above
  `Number.MAX_SAFE_INTEGER` (~9 × 10¹⁵) are handled correctly. Tests
  in `tests/exposure.test.ts` cover this property explicitly.
- The exposure tally is **eventually consistent**. Two bids accepted
  by the off-chain gate in the same instant may collectively exceed
  the cap until both are persisted. The on-chain contract remains the
  final source of truth; this layer is a soft, cheap pre-filter.

### Files

| File                                  | Purpose                                                  |
|---------------------------------------|----------------------------------------------------------|
| `src/services/exposureService.ts`     | Exposure computation + cap enforcement                   |
| `src/controllers/v1/bids.ts`          | `POST /bids` gate returning 429 EXPOSURE_CAP_EXCEEDED    |
| `src/validators/bids.ts`              | Optional `currency` tag in body; delegates cap to service|
| `src/config.ts`                       | `EXPOSURE_CAP_PER_INVESTOR_USD` zod-validated env var    |
| `openapi.yaml`                        | Documents 429 response on POST /bids                     |
| `src/tests/exposure.test.ts`          | Unit + integration coverage (95%+ target)                |


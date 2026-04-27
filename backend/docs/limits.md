# Request Limits & Input Validation

This document describes the global request validation and size limits implemented in the QuickLendX backend API.

## Overview

To prevent abuse, memory pressure, and injection vectors, all incoming HTTP requests are subject to:

1. **Size limits** on body, query parameters, and headers
2. **Input validation** via Zod schemas
3. **Sanitization** to prevent XSS and injection attacks

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
├── app.ts                      # Global middleware registration
└── routes/v1/
    ├── bids.ts                 # Updated with query validation
    ├── invoices.ts             # Updated with param + query validation
    └── settlements.ts          # Updated with param + query validation
```

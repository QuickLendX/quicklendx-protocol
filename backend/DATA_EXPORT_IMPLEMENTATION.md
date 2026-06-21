# Data Export Endpoints — Implementation Summary

## Branch
`feature/backend-data-exports`

## Commit
`feat: add signed user-scoped data export endpoints with audit logging`

---

## API Endpoints

| Method | Endpoint | Auth | Description |
|--------|----------|------|-------------|
| `POST` | `/api/v1/exports/generate` | `Bearer <userId>` | Returns signed download link |
| `GET`  | `/api/v1/exports/download/:token` | Signed token | Streams export file |

### Generate

```
POST /api/v1/exports/generate?format=json
Authorization: Bearer <user_id>
Content-Type: application/json

200 OK
{
  "success": true,
  "download_url": "/api/v1/exports/download/<TOKEN>",
  "expires_in": "1 hour"
}
```

Supported formats: `json` (default), `csv`. Invalid format → `400 INVALID_FORMAT`.

### Download

```
GET /api/v1/exports/download/<TOKEN>

200 OK  — Content-Type: application/json  or  text/csv
         Content-Disposition: attachment; filename="quicklendx-export-<userId>-<date>.<ext>"
401     — INVALID_TOKEN (tampered, expired, or missing)
```

---

## Security

| Control | Implementation |
|---------|----------------|
| AuthN | `requireUserAuth` — Bearer required on `/generate` |
| No IDOR | Token payload locks `userId`; `getUserData()` hard-filters every collection |
| Signed links | HMAC-SHA256(`{userId, format, expiresAt}`, EXPORT_SECRET) encoded as base64 |
| Expiry | 1-hour TTL validated on every `/download` call |
| Audit | `data_export_requested` + `data_export_downloaded` in `auditLogService` |

---

## New Files

- `src/services/exportService.ts` — signing, validation, data retrieval, formatting
- `src/controllers/v1/exports.ts` — `requestExport` / `downloadExport` handlers
- `src/routes/v1/exports.ts` — router wiring
- `src/middleware/userAuth.ts` — `requireUserAuth` + `getUser()` helpers
- `src/types/auth.ts` — `UserContext` / `RequestWithUser` types
- `tests/exports.test.ts` — 30 tests

---

## Environment Variables

```env
EXPORT_SECRET=your-min-32-char-secret   # required in production
```

---

## Test Output

```
Tests: 30 passed, 30 total
Coverage on new files:
  Statements: 94.1%
  Branches:   83.3%
  Functions:  93.75%
  Lines:      93.87%
  userAuth.ts: 100% across all metrics
```

### Coverage categories
- AuthZ: absent/malformed/empty-Bearer/valid
- Format: json, csv, invalid → 400
- IDOR: other-user token returns empty; own token returns only own records
- Content: JSON structure, CSV section headers, row presence/absence
- Token: tampered, expired, valid
- Audit log: both events recorded
- Middleware units: direct injection for empty-token and getUser-throws paths

---

## Data Scoping

| Collection | Filter |
|------------|--------|
| Invoices | `invoice.business === userId` |
| Bids | `bid.investor === userId` |
| Settlements | `settlement.payer === userId \|\| settlement.recipient === userId` |

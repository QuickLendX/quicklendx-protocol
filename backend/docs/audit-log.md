# Audit Log — Append-Only Log for Privileged Backend Operations

## Overview

The audit log records an immutable, append-only history of privileged operations performed via the backend API. Every entry captures the actor identity, timestamped action, redacted parameters, client metadata, and a human-readable effect summary.

## Design Principles

- **Append-only**: Logs are written via `fs.appendFileSync` only. No read-modify-write operations exist. The service has no `deleteEntry` or `updateEntry` method.
- **Actor-identified**: Every privileged request must present an `X-API-Key` header. Keys map to stable actor identifiers (e.g., `deploy-bot`, `oncall-operator`, `security-admin`).
- **Secrets redacted at write time**: Sensitive fields are replaced with `[REDACTED]` in the stored `redactedParams` field. The raw `params` field is also stored for post-incident reconstruction if needed, but should be treated as potentially redaction-capable in the future.
- **Daily rotation**: One `.jsonl` file per day (`audit-YYYY-MM-DD.jsonl`). Files are never modified after creation.
- **Idempotent-enough ULIDs**: Entry IDs use ULID (time-sortable, lexicographic) for approximate ordering without coordination.

## Audit Entry Format

Each line in a `.jsonl` file is a valid JSON object:

```json
{
  "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "timestamp": "2026-04-25T10:30:00.000Z",
  "actor": "deploy-bot",
  "operation": "WEBHOOK_SECRET_ROTATE",
  "params": {
    "keyId": "wh-live-key-001",
    "secret": "sk_live_..."
  },
  "redactedParams": {
    "keyId": "wh-live-key-001",
    "secret": "[REDACTED]"
  },
  "ip": "10.0.0.42",
  "userAgent": "curl/8.4.0",
  "effect": "Webhook secret rotated for keyId: wh-live-key-001",
  "success": true
}
```

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | ULID, globally unique |
| `timestamp` | ISO 8601 datetime | Server-side write time |
| `actor` | string | Actor identifier from API key map |
| `operation` | enum | One of the audited operation types |
| `params` | object | Raw request body (may contain secrets) |
| `redactedParams` | object | Same as params, sensitive fields replaced |
| `ip` | string | Client IP (first IP from X-Forwarded-For) |
| `userAgent` | string | Client User-Agent header |
| `effect` | string | Human-readable summary of the resulting change |
| `success` | boolean | Whether the operation completed successfully |
| `errorMessage` | string? | Error message if `success` is false |

## Operations Tracked

| Operation | Triggered By |
|-----------|-------------|
| `MAINTENANCE_MODE` | `POST /api/v1/admin/maintenance` |
| `WEBHOOK_SECRET_ROTATE` | `POST /api/v1/admin/webhook/rotate` |
| `CONFIG_CHANGE` | `POST /api/v1/admin/config` |
| `BACKFILL_START` | `POST /api/v1/admin/backfill` |
| `BACKFILL_PROGRESS` | Progress events during backfill |
| `BACKFILL_COMPLETE` | Backfill job completion |
| `BACKFILL_ABORT` | `POST /api/v1/admin/backfill/abort` |
| `ADMIN_API_KEY_ADD` | `POST /api/v1/admin/keys` |
| `ADMIN_API_KEY_REVOKE` | `DELETE /api/v1/admin/keys` |

## API Endpoints

### GET /api/v1/admin/audit

Query audit entries with filters.

**Auth**: Requires valid `X-API-Key` header.

**Query parameters**:

| Param | Type | Default | Description |
|--------|------|---------|-------------|
| `actor` | string | — | Filter by actor |
| `operation` | string | — | Filter by operation enum |
| `from` | ISO 8601 | — | Start of time range (inclusive) |
| `to` | ISO 8601 | — | End of time range (inclusive) |
| `limit` | integer | 100 | Max entries returned (1–10000) |
| `offset` | integer | 0 | Pagination offset |

**Response**:
```json
{
  "entries": [...],
  "total": 1234,
  "limit": 100,
  "offset": 0,
  "hasMore": true
}
```

### GET /api/v1/admin/audit/operations

List all valid operation types.

**Auth**: Requires valid `X-API-Key` header.

**Response**: `{"operations": ["MAINTENANCE_MODE", "WEBHOOK_SECRET_ROTATE", ...]}`

### GET /api/v1/admin/audit/export

Stream all entries in a date range as NDJSON (newline-delimited JSON).

**Auth**: Requires valid `X-API-Key` header.

**Query parameters**: `from`, `to` (same semantics as `/audit`).

**Response**: Content-Type `application/x-ndjson`, streamed.

## Storage

- **Directory**: `audit_logs/` (configurable via `AUDIT_DIR` env var)
- **File naming**: `audit-YYYY-MM-DD.jsonl`
- **Encoding**: UTF-8
- **Max entry size**: 10 KB per line (enforced at write time)
- **Rotation**: Automatic by date; no automatic retention policy (handled externally)

## Configuration

| Environment Variable | Default | Description |
|------------------|---------|-------------|
| `AUDIT_DIR` | `audit_logs` | Directory for `.jsonl` files |
| `ADMIN_API_KEYS` | *(unset)* | Comma-separated `key:actor` pairs |
| `SKIP_API_KEY_AUTH` | *(unset)* | Set `true` to bypass auth (dev/test only) |
| `TEST_ACTOR` | *(unset)* | Actor name when `SKIP_API_KEY_AUTH=true` |

### Example ADMIN_API_KEYS

```bash
ADMIN_API_KEYS="k8s-deploy:deploy-bot,oncall-key:oncall-operator,security-key:security-admin"
```

## Security Considerations

1. **No secrets in `redactedParams`**: Fields matching `SENSITIVE_FIELDS` (`secret`, `token`, `apiKey`, `password`, `privateKey`, `accessToken`, `refreshToken`, etc.) are replaced with `[REDACTED]`. The check is case-insensitive.

2. **Secrets in `params`**: The raw `params` field is stored for forensic reconstruction, not for programmatic consumption. Treat it as potentially containing secrets.

3. **Tamper resistance**: The append-only design means any tampering is detectable by comparing entry IDs and timestamps. File-level integrity can be strengthened by pairing with tools like [osquery](https://osquery.io) or audit daemon monitoring.

4. **API key storage**: `ADMIN_API_KEYS` is an env var, not a file. In production, inject it via your orchestration secret store (Kubernetes Secrets, Vault, etc.). Never commit real keys.

5. **Auth bypass**: `SKIP_API_KEY_AUTH=true` must never be set in production. It is stripped from CI and test environments.

6. **Read access**: The `/audit` endpoint is itself a privileged endpoint. Only operators with a valid `X-API-Key` should be able to query it.

## Adding a New Audited Operation

1. Add the operation name to `AuditOperationSchema` in `src/types/audit.ts`.
2. Register the route in `AUDIT_ROUTES` in `src/middleware/auditMiddleware.ts` with the operation type and an `describeEffect` function.
3. Add tests in `tests/audit.test.ts`.
4. Document the new operation in this file.

## Testing

```bash
cd backend
npm test
```

To run only the audit tests:

```bash
npm test -- tests/audit.test.ts
```

To run with coverage:

```bash
npm run test:coverage
```

## Extending

### Switching to a database backend

Replace `AuditService.append()` and `AuditService.queryWithSchema()` with database calls. The interface (`append`, `query`, `getEntriesForTest`, `clearAll`) remains the same.

### Adding per-entry cryptographic signatures

After each `append()`, compute an HMAC-SHA256 of the entry line using a per-day secret and append it as a second line (or a companion `.sig` file). Verify on read.

### Structured log shipping

Point a log shipper (Filebeat, Fluentd, Vector) at `audit_logs/`. The `.jsonl` format is line-delimited and compatible with most log ingestion pipelines.
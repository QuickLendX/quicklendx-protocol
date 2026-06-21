# Admin Monitoring Endpoints

Operational visibility endpoints for privileged operators. All routes require a valid `X-API-Key` header (same authentication as the audit log system).

**Base paths:**
- Primary: `/api/v1/admin/monitoring`
- Legacy compat: `/api/admin/monitoring`

## Authentication

Every endpoint requires the `X-API-Key` header. Requests without a valid key return `401 Unauthorized` with `code: "UNAUTHORIZED"`.

In development/test environments, set `SKIP_API_KEY_AUTH=true` to bypass authentication.

## Endpoints

### GET /health

Overall system health. Aggregates sub-system health into a single status value.

**Response:**
```json
{
  "status": "ok",
  "statusService": "ok",
  "webhookQueue": "ok",
  "invariants": "ok",
  "timestamp": "2026-04-25T10:00:00.000Z"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `status` | enum | Overall: `ok`, `degraded`, `maintenance`, `unavailable` |
| `statusService` | enum | Status service health: `ok`, `degraded`, `unavailable` |
| `webhookQueue` | enum | Webhook queue health: `ok`, `degraded`, `unavailable` |
| `invariants` | enum | Invariant check health: `ok`, `degraded`, `unavailable` |
| `timestamp` | ISO 8601 | Server time of response |

**Status mapping:**
- `maintenance` — returned when maintenance mode is enabled (highest priority)
- `unavailable` — any sub-system throws an exception
- `degraded` — any sub-system returns degraded
- `ok` — all sub-systems healthy

---

### GET /cursor

Blockchain indexing cursor position and ingest lag.

**Response:**
```json
{
  "lastIndexedLedger": 100000,
  "currentLedger": 100005,
  "ingestLag": 5,
  "timestamp": "2026-04-25T10:00:00.000Z"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `lastIndexedLedger` | integer | Last ledger height fully indexed by the ingest process |
| `currentLedger` | integer | Current blockchain ledger height (mock or real RPC) |
| `ingestLag` | integer | `currentLedger - lastIndexedLedger` |

---

### GET /invariants

Foreign-key reference integrity checks across all domain entities. Each counter returns a count and up to 5 sample IDs for investigation.

**Response:**
```json
{
  "orphanBids": { "count": 0, "sampleIds": [] },
  "orphanSettlements": { "count": 0, "sampleIds": [] },
  "orphanDisputes": { "count": 0, "sampleIds": [] },
  "mismatchSettlements": { "count": 0, "sampleIds": [] },
  "timestamp": "2026-04-25T10:00:00.000Z"
}
```

| Counter | Checks |
|---------|---------|
| `orphanBids` | `Bid.invoice_id` not in `Invoice.id` |
| `orphanSettlements` | `Settlement.invoice_id` not in `Invoice.id` |
| `orphanDisputes` | `Dispute.invoice_id` not in `Invoice.id` |
| `mismatchSettlements` | `Settlement.invoice_id` not in `Bid.invoice_id` (settlement must follow a bid) |

**Sample IDs** are limited to 5 per counter to prevent large responses. They are IDs of the offending entities, not full record payloads.

---

### GET /webhook

Webhook queue statistics. Returns counts only — never includes event payloads or type contents.

**Response:**
```json
{
  "depth": 0,
  "successCount": 0,
  "failureCount": 0,
  "overflowCount": 0,
  "oldestTimestamp": null
}
```

| Field | Type | Description |
|-------|------|-------------|
| `depth` | integer | Current number of events in the queue |
| `successCount` | integer | Total events successfully delivered |
| `failureCount` | integer | Total events marked as failed |
| `overflowCount` | integer | Number of times the oldest entry was evicted due to ring buffer overflow |
| `oldestTimestamp` | ISO 8601 \| null | Timestamp of the oldest queued event, or null if queue is empty |

---

### POST /webhook

Enqueue a webhook event.

**Request body:**
```json
{
  "type": "invoice.funded",
  "payload": { "invoiceId": "0x...", "amount": "1000000" }
}
```

| Field | Required | Type | Description |
|-------|----------|------|-------------|
| `type` | Yes | string | Event type identifier (e.g. `invoice.funded`) |
| `payload` | No | unknown | Arbitrary event data (stored but never returned in queue stats) |

**Response:** `201 Created`
```json
{
  "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "enqueuedAt": "2026-04-25T10:00:00.000Z"
}
```

**Errors:**
- `400 INVALID_WEBHOOK_PAYLOAD` — missing or invalid `type` field

---

### POST /webhook/:id/success

Mark a pending webhook event as successfully delivered.

**Response:**
```json
{
  "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "outcome": "success"
}
```

If the event is not found (already resolved or never existed):
```json
{
  "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "outcome": "not_found_or_already_resolved"
}
```

---

### POST /webhook/:id/fail

Mark a pending webhook event as failed.

**Response:**
```json
{
  "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "outcome": "failed"
}
```

Idempotent — returns `not_found_or_already_resolved` for unknown or already-resolved IDs.

## Webhook Queue — Ring Buffer Semantics

The webhook queue is a fixed-capacity circular buffer (default 1000 entries, configurable via `WEBHOOK_QUEUE_MAX`).

- **Overflow**: When the buffer is full and a new event is enqueued, the oldest entry is evicted and `overflowCount` increments.
- **Order**: Eviction is FIFO — the oldest event by `enqueuedAt` timestamp is evicted first.
- **No automatic retry**: Failed events remain in the queue. Operators should call `POST /webhook/:id/fail` and implement retry logic externally.

## Security Notes

1. **Admin-only** — all 7 endpoints require `X-API-Key`. There is no public equivalent.
2. **No secrets in responses** — the `/webhook` GET endpoint never returns event payloads, types, or any payload content. Only counts and timestamps are exposed.
3. **Invariant responses are minimal** — only counts and up to 5 sample IDs per counter. Full record payloads (including `metadata`, `line_items`, etc.) are never returned.
4. **Cursor endpoint** — exposes only `lastIndexedLedger`, `currentLedger`, and `ingestLag`. Internal state (`mockCurrentLedger`, `isMaintenanceMode`, etc.) is not exposed.
5. **Health endpoint** — uses a safe status enum only. No stack traces, error details, or raw exception messages are included in responses.

## Configuration

| Environment Variable | Default | Description |
|--------------------|---------|-------------|
| `WEBHOOK_QUEUE_MAX` | `1000` | Maximum number of events in the webhook ring buffer |
| `SKIP_API_KEY_AUTH` | *(unset)* | Set `true` to bypass auth in dev/test environments |

## Adding New Invariant Checks

1. Add the check to `src/services/invariantService.ts` following the `scanOrphans` pattern.
2. Register the new counter in the `InvariantReportSchema`.
3. Add tests for the new counter.
4. Update this document.

## Testing

```bash
cd backend
npm test -- tests/monitoring.test.ts
```

To run with coverage:

```bash
npm run test:coverage
```

## Dependencies

- `src/services/statusService.ts` — provides `getLastIndexedLedger()`, `getCurrentLedger()`, `isMaintenanceEnabled()`
- `src/services/invariantService.ts` — stateless FK checks over mock data arrays
- `src/services/webhookQueueService.ts` — ring buffer with overflow tracking
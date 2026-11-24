# Webhook Retry & Backoff Policy

## Overview
Failed webhook deliveries are persisted in the `webhook_deliveries` table and
retried with an exponential backoff schedule and jitter before being promoted
to the dead-letter queue (DLQ).

## Schema

### `webhook_deliveries`

| Column | Type | Description |
|--------|------|-------------|
| `id` | `TEXT PK` | ULID, unique delivery identifier |
| `event_type` | `TEXT NOT NULL` | Event name (e.g. `invoice.paid`) |
| `payload` | `TEXT NOT NULL` | JSON-serialised event payload |
| `subscriber_id` | `TEXT` | Target subscriber, if known |
| `status` | `TEXT` | One of: `pending`, `processing`, `success`, `failed`, `dead_letter` |
| `enqueued_at` | `TEXT` | ISO-8601 timestamp of enqueue |
| `attempt_count` | `INTEGER` | Number of delivery attempts so far |
| `max_attempts` | `INTEGER` | Maximum attempts before DLQ (default 5) |
| `next_retry_at` | `TEXT` | ISO-8601 timestamp for next scheduled retry |
| `last_error` | `TEXT` | Error message from the last failed attempt |
| `last_attempt_at` | `TEXT` | ISO-8601 timestamp of the last delivery attempt |
| `created_at` | `TEXT` | ISO-8601 timestamp of row creation |
| `updated_at` | `TEXT` | ISO-8601 timestamp of last update |

### Indexes

- `idx_webhook_deliveries_status_next_retry` on `(status, next_retry_at)` — fast
  polling of pending and due deliveries.
- `idx_webhook_deliveries_created_at` on `(created_at)` — efficient cleanup of
  expired rows.

## Lifecycle

```
enqueue
  │
  ▼
pending ──► processing ──► success
  │              │
  │              ▼ (failure with remaining attempts)
  │           failed
  │              │
  │              ▼ (next_retry_at timer expires)
  │           pending (re-queued automatically)
  │
  └──► processing ──► dead_letter (max attempts exhausted)
                          │
                          ▼ (manual intervention)
                       pending (retryDeadLetter)
```

1. **Enqueue**: a delivery is inserted with `status = 'pending'` and
   `attempt_count = 0`.
2. **Processing**: picked up by the worker, moved to `processing`.
3. **Success**: removed from the active queue; kept for auditability.
4. **Failure**: `attempt_count` incremented, `next_retry_at` set per the retry
   schedule, status set to `failed`.
5. **Re-queue**: `getPending()` returns rows where `status = 'pending'` AND
   (`next_retry_at IS NULL` OR `next_retry_at <= now`).
6. **Dead letter**: when `attempt_count >= max_attempts`, status becomes
   `dead_letter`. These rows are NOT returned by `getPending()`.
7. **Manual retry**: `retryDeadLetter()` moves a `dead_letter` row back to
   `pending` with `next_retry_at = now`.

## Retry Schedule

The retry delays follow an exponential schedule with jitter:

| Attempt count | Base delay | Jitter range |
|--------------:|-----------:|-------------:|
| 1 | 1 minute | 0–30 s |
| 2 | 5 minutes | 0–2.5 min |
| 3 | 30 minutes | 0–15 min |
| 4 | 2 hours | 0–1 h |
| 5 | 12 hours | 0–6 h |

Jitter is computed as `Math.round(Math.random() * baseDelay * 0.5)`. The total
is `baseDelay + jitter` milliseconds.

After 5 failed attempts the delivery is promoted to `dead_letter` and will NOT
be retried automatically.

## Retry Conditions

- **Retried:** TIMEOUT, TRANSPORT_ERROR, 5xx responses, 429 Too Many Requests
- **Not retried:** 4xx responses (except 429), URL_INVALID, non-retryable errors

## Dead-Letter Queue (DLQ)

An event is dead-lettered when:

- Max attempts are exhausted
- A permanent 4xx response is received
- A non-retryable error occurs

Exhausted deliveries are persisted to the `webhook_dead_letters` table (as well as tracked in `webhook_deliveries` with `status = 'dead_letter'`).

### `webhook_dead_letters` Schema

| Column | Type | Description |
|--------|------|-------------|
| `id` | `TEXT PK` | ULID, unique dead-letter identifier |
| `delivery_id` | `TEXT` | Associated delivery identifier, if applicable |
| `event_type` | `TEXT NOT NULL` | Event name (e.g. `invoice.paid`) |
| `payload` | `TEXT NOT NULL` | JSON-serialised event payload |
| `target_url` | `TEXT` | Destination webhook URL |
| `subscriber_id` | `TEXT` | Target subscriber, if known |
| `attempt_count` | `INTEGER` | Number of delivery attempts before DLQ promotion |
| `max_attempts` | `INTEGER` | Configured attempt budget (default 5) |
| `last_error` | `TEXT` | Final error message or HTTP status failure |
| `status` | `TEXT` | `dead_letter` or `replayed` |
| `replayed_at` | `TEXT` | ISO-8601 timestamp of replay |
| `created_at` | `TEXT` | ISO-8601 timestamp of DLQ entry creation |
| `updated_at` | `TEXT` | ISO-8601 timestamp of last update |

### Admin DLQ Management & Replay Endpoints

Operators can inspect and re-drive failed deliveries via administrative endpoints:

- **`GET /admin/webhooks/dead-letters`** (paginated):
  Lists dead letters. Requires `support`, `operations_admin`, or `super_admin` role (`read:*` scope).
  Query parameters: `limit` (default 20, max 100), `offset` (default 0).

- **`POST /admin/webhooks/dead-letters/:id/replay`**:
  Re-enqueues the delivery with a fresh attempt count (`attempt_count = 0`, status `pending`).
  Requires `operations_admin` or `super_admin` role (`write:*` scope).
  Replay is guarded so each dead letter can be re-enqueued only once; subsequent attempts return HTTP `409 Conflict`.
  Every replay action is recorded with actor metadata in the audit log.

## Retention & Cleanup

The `cleanup(olderThanDays)` method deletes `success` and `dead_letter` rows
older than the given threshold (default 90 days). `pending`, `processing`, and
`failed` rows are never cleaned up this way.

The `vacuum()` method runs `PRAGMA incremental_vacuum` to reclaim space after
large cleanups.

## Security

SSRF protections in `urlValidation.ts` and `egressPolicy.ts` are enforced on
every retry attempt.

### DNS rebinding protection

Webhook deliveries pin the resolved IP into the TCP connection to prevent DNS
rebinding attacks (where a domain first resolves to a public IP, passing the
blocked-address check, and later resolves to a private/internal IP):

- **IP pinning**: before every HTTPS request the target hostname is resolved
  via DNS and the first public address is pinned.  The `https.Agent` uses a
  custom `lookup` that always returns this pinned IP — it never re-resolves.
- **Re-validation before connect**: the agent re-validates the pinned IP via
  `isBlockedDestinationIP` immediately before every socket connect.  If the
  IP has become blocked (e.g. the cached mapping expired and the attacker now
  points to a private address), the request is aborted with an `EGRESS_BLOCKED`
  error.
- **Redirects blocked**: 3xx responses are rejected with `REDIRECT_NOT_ALLOWED`.
  Redirects are not followed, closing the window for open-redirect chains that
  could bypass the initial IP check.
- **TLS SNI preserved**: `servername` is set to the original hostname so that
  TLS certificate validation and SNI use the intended domain, not the pinned IP.

These controls are implemented in `delivery.ts` (`resolveHostnameToPinnedIp`,
`createPinnedAgent`).

## Testing

```bash
cd backend
npm test -- webhook-delivery-repo
npm test -- webhookMiddleware
```

## Migration

The schema is created by migration `v006_webhook_deliveries.ts`. Run:

```bash
cd backend
npm run migrate
```

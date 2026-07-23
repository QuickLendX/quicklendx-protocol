# Notification Delivery Semantics

## Overview

QuickLendX sends email notifications for four on-chain events:

| `NotificationType`   | Trigger                                  |
|----------------------|------------------------------------------|
| `invoice_funded`     | An invoice is settled by an investor     |
| `payment_received`   | A payment is recorded against an invoice |
| `dispute_opened`     | A dispute is created for an invoice      |
| `dispute_resolved`   | A dispute is resolved by an admin        |

Notifications are processed by `NotificationService.processNotification()`, which is called from `EventProcessor` after each relevant on-chain event is indexed.

---

## Idempotency

### Key

The idempotency key is the composite `(event_id, user_id)` stored as a `UNIQUE` constraint in the `notifications` table.

```
UNIQUE(event_id, user_id)
```

`event_id` is derived from the on-chain event identifier (set by `EventProcessor` as `${eventId}_business`, `${eventId}_dispute`, etc.). `user_id` is the Stellar address of the recipient.

### Guarantee

- **At-most-once delivery per (event_id, user_id) pair.** Once a row reaches `status = 'sent'`, all subsequent calls with the same key are no-ops.
- **Restart-safe.** The `notifications` table is durable SQLite. A process restart followed by event replay will not re-send already-delivered notifications.
- **Concurrent-safe.** `INSERT OR IGNORE` ensures that two concurrent calls for the same key produce exactly one row.

### Status lifecycle

```
(no row) ──INSERT OR IGNORE──► pending ──SMTP ok──► sent
                                         └──SMTP err──► failed ──retry──► sent
                                                                  └──retry──► failed (...)
```

| Status    | Meaning                                                  |
|-----------|----------------------------------------------------------|
| `pending` | Row inserted; send attempt in progress                   |
| `sent`    | Email delivered successfully (or user opted out)         |
| `failed`  | SMTP error; row is retryable on next event replay        |

Opt-out paths (no preferences row, `email_enabled = false`, `email_address = NULL`, or type disabled) are recorded as `sent` to prevent retry spam.

---

## In-Memory Deduplication Cache

An LRU cache with TTL sits in front of the database to avoid a SQLite query on every hot-path notification.

| Env var                          | Default | Description                           |
|----------------------------------|---------|---------------------------------------|
| `MAX_NOTIFICATION_DEDUP_ENTRIES` | `10000` | Max cache entries before LRU eviction |
| `NOTIFICATION_DEDUP_TTL_MS`     | `86400000` (24h) | Time-to-live for each entry |

### Behaviour

- **LRU eviction**: When the cache exceeds `MAX_NOTIFICATION_DEDUP_ENTRIES`, the least-recently-used entry is evicted.
- **TTL expiry**: Entries older than `NOTIFICATION_DEDUP_TTL_MS` are removed lazily on `has()`.
- **Eviction metric**: `notificationService.dedupCacheEvictions` exposes the total number of max-size evictions (planned for consumption by the `/metrics` endpoint in #1054).
- **Not persisted**: The cache is in-memory only; durability is provided by the `notifications` table. A process restart resets the cache, which will be repopulated on the first DB-backed check.

### Integration

```typescript
// In NotificationService.processNotification():
// 1. Check in-memory cache (fastest path)
// 2. Check SQLite if cache miss
// 3. On confirmation of "sent" → populate cache
// 4. On send failure → do NOT cache (retryable)
```

---

## User Notification Preferences

Preferences are stored in the `user_notification_preferences` table (created by migration `v008`).

| Column                    | Default | Meaning                                      |
|---------------------------|---------|----------------------------------------------|
| `user_id`                 | PK      | Stellar address                              |
| `email_enabled`           | `1`     | Master switch for all email notifications    |
| `email_address`           | NULL    | Recipient address; NULL disables delivery    |
| `notify_invoice_funded`   | `1`     | Per-type opt-in                              |
| `notify_payment_received` | `1`     | Per-type opt-in                              |
| `notify_dispute_opened`   | `1`     | Per-type opt-in                              |
| `notify_dispute_resolved` | `1`     | Per-type opt-in                              |

If no row exists for a user, the notification is silently skipped and recorded as `sent`.

### API

```typescript
// Upsert preferences
notificationService.updateUserPreferences(userId, {
  email_enabled: true,
  email_address: 'user@example.com',
  notifications: {
    invoice_funded: true,
    payment_received: false,
    dispute_opened: true,
    dispute_resolved: true,
  },
});

// Read preferences (returns null if not found)
const prefs = notificationService.getUserPreferencesPublic(userId);
```

---

## Database Schema

### `notifications`

```sql
CREATE TABLE notifications (
  id                TEXT PRIMARY KEY,
  event_id          TEXT NOT NULL,
  user_id           TEXT NOT NULL,
  notification_type TEXT NOT NULL,
  status            TEXT NOT NULL CHECK(status IN ('pending','sent','failed')),
  smtp_error        TEXT,           -- truncated to 500 chars; never contains PII
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  UNIQUE(event_id, user_id)         -- idempotency key
);
```

### `user_notification_preferences`

```sql
CREATE TABLE user_notification_preferences (
  user_id                   TEXT PRIMARY KEY,
  email_enabled             INTEGER NOT NULL DEFAULT 1,
  email_address             TEXT,
  notify_invoice_funded     INTEGER NOT NULL DEFAULT 1,
  notify_payment_received   INTEGER NOT NULL DEFAULT 1,
  notify_dispute_opened     INTEGER NOT NULL DEFAULT 1,
  notify_dispute_resolved   INTEGER NOT NULL DEFAULT 1,
  updated_at                TEXT NOT NULL
);
```

Both tables are created by migration `v008_create_notifications` (forward-only).

---

## Security

- SMTP errors stored in `smtp_error` are truncated to 500 characters and must not contain email body content or recipient PII.
- Email addresses are never logged at `info` level. Log lines reference only `event_id` and `user_id` (Stellar address).
- Raw SMTP credentials (`SMTP_USER`, `SMTP_PASS`) are read from environment variables and never stored in the database.

---

## Retry Behaviour

A structured retry budget with circuit-breaker behavior is implemented around outbound notification delivery.
- Transient SMTP failures are retried automatically with an exponential backoff and jittered window.
- A `failed` row is marked as such only after the in-process retry budget is exhausted (permanent drop).
- On a permanent drop, the failure is persisted to the audit log and surfaced as a `HIGH` severity alert via `alertRouter`.
- After a permanent drop, the row is stored as `failed` and is retryable the next time the same event is replayed (e.g., via the backfill/replay service).

To manually trigger a retry of a permanent drop, replay the originating on-chain event through `EventProcessor.processEvent()`.

---

## Migration

Migration `v008_create_notifications` must be applied before the service starts:

```
backend/src/migrations/v008_create_notifications.ts
```

Rollback (emergency only):

```sql
DROP TABLE notifications;
DROP TABLE user_notification_preferences;
```

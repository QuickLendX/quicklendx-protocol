# QuickLendX — Complete Event Schema Reference

This document is the canonical reference for every event emitted by the
QuickLendX protocol.  Indexers should treat field names, topic strings, and
ordering as stable unless a breaking-change notice appears in the changelog.

---

## General envelope

All events share a common outer envelope when delivered via the ingestion
API (`POST /api/v1/events`):

```json
{
  "id":             "string (unique per event)",
  "ledger":         0,
  "txHash":         "string",
  "type":           "<EventType>",
  "payload":        { ... },
  "timestamp":      0,
  "complianceHold": false,
  "indexedAt":      "ISO 8601"
}
```

The `type` field determines which payload schema applies.

---

## Protocol events

### InvoiceSettled

Emitted when an invoice is fully settled.

```json
{
  "type": "InvoiceSettled",
  "payload": {
    "invoice_id": "string",
    "business":   "string",
    "investor":   "string",
    "amount":     "string"
  }
}
```

---

### PaymentRecorded

Emitted when a partial or full payment is recorded against an invoice.

```json
{
  "type": "PaymentRecorded",
  "payload": {
    "invoice_id": "string",
    "payer":      "string",
    "amount":     "string"
  }
}
```

---

### DisputeCreated

Emitted when a dispute is opened for an invoice.

```json
{
  "type": "DisputeCreated",
  "payload": {
    "invoice_id": "string",
    "initiator":  "string"
  }
}
```

---

### DisputeResolved

Emitted when an active dispute is resolved.

```json
{
  "type": "DisputeResolved",
  "payload": {
    "invoice_id":  "string",
    "resolved_by": "string"
  }
}
```

---

## Operational / safety events

### PauseBlocked

Emitted on **every** call that `require_unpaused` rejects because the
protocol is currently paused.  One event is emitted per blocked invocation —
there is no deduplication.

**Topic constant**: `events::TOPIC_PAUSE_BLOCKED = "PauseBlocked"`  
**Source module**: `src/pause.rs` → `PauseState::require_unpaused`

```json
{
  "type": "PauseBlocked",
  "payload": {
    "entrypoint": "string",
    "caller":     0,
    "ledger_ts":  0
  }
}
```

#### Payload fields

| Field | Type | Description |
|---|---|---|
| `entrypoint` | `string` | Stable symbol of the blocked entrypoint (see table below) |
| `caller` | `u64` | Numeric account ID whose call was rejected |
| `ledger_ts` | `u64` | Ledger timestamp (seconds since epoch) at the point of rejection |

#### `entrypoint` values

| Value | Protected action |
|---|---|
| `"invoice_upload"` | Business uploads a new invoice |
| `"bid_placement"` | Investor places a bid on an invoice |
| `"settlement_initiation"` | Business initiates invoice settlement |
| `"escrow_release"` | Business triggers escrow release |
| `"investment_action"` | Investor takes a general investment action |

#### Indexer notes

- Subscribe on the exact topic string `"PauseBlocked"`.
- `entrypoint` values are ASCII lowercase with underscores — safe to use as
  metric labels or map keys without sanitisation.
- Field order is stable; future additions will be appended.
- An absence of `PauseBlocked` events in a window means the protocol was
  either live or received no guarded-entrypoint traffic.
- Use this event to drive dashboards that track rejected-traffic volume per
  entrypoint during an incident.

---

## Validation rules (all event types)

- All fields are required unless marked optional.
- Unknown `type` values are rejected with HTTP 400.
- Each event must carry a stable unique `id`; replaying a known `id` is a
  no-op (`status: "duplicate"`).
- Oversized batches (> 100 events) are rejected before processing.
- Raw payload values are never echoed in error responses.

## Response shape

| Outcome | HTTP status | `status` field |
|---|---|---|
| Accepted and processed | 200 | `"processed"` |
| Known duplicate | 200 | `"duplicate"` |
| Validation failure | 400 | `"rejected"` |

Mixed batches return a per-event `results` array.  If any item is rejected
the HTTP status is 400; valid non-duplicate items in the same batch may still
be processed.

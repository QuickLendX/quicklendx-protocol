# Dispute Timeline Endpoint

## Overview

`get_dispute_timeline` normalizes the dispute lifecycle events for a given
invoice into a chronologically ordered, redacted sequence suitable for UI
consumption.  It reflects on-chain truth without editorial rewriting and
supports offset/limit pagination.

## Endpoint

```
get_dispute_timeline(invoice_id: BytesN<32>, offset: u32, limit: u32)
  -> Result<DisputeTimeline, QuickLendXError>
```

Defined in `src/dispute_timeline.rs`.

---

## Request Parameters

| Parameter    | Type         | Required | Description |
|--------------|--------------|----------|-------------|
| `invoice_id` | `BytesN<32>` | Yes      | The invoice whose dispute timeline is requested. |
| `offset`     | `u32`        | Yes      | Zero-based starting position (0 = first event). |
| `limit`      | `u32`        | Yes      | Maximum entries to return. Capped at 50 (`TIMELINE_MAX_PAGE_SIZE`). |

---

## Response — `DisputeTimeline`

```rust
pub struct DisputeTimeline {
    pub entries:        Vec<DisputeTimelineEntry>,
    pub total:          u32,
    pub has_more:       bool,
    pub current_status: DisputeStatus,
}
```

| Field            | Type                          | Description |
|------------------|-------------------------------|-------------|
| `entries`        | `Vec<DisputeTimelineEntry>`   | Ordered slice of events for this page. |
| `total`          | `u32`                         | Total event count across all pages. |
| `has_more`       | `bool`                        | `true` when additional pages exist. |
| `current_status` | `DisputeStatus`               | On-chain dispute status at query time. |

### `DisputeTimelineEntry`

```rust
pub struct DisputeTimelineEntry {
    pub sequence:  u32,
    pub event:     String,
    pub timestamp: u64,
    pub actor:     Address,
    pub summary:   String,
}
```

| Field       | Type      | Description |
|-------------|-----------|-------------|
| `sequence`  | `u32`     | 0-based position within the full timeline. |
| `event`     | `String`  | `"Opened"`, `"UnderReview"`, or `"Resolved"`. |
| `timestamp` | `u64`     | Ledger timestamp when this event occurred. |
| `actor`     | `Address` | Address of the triggering actor (see Redaction). |
| `summary`   | `String`  | Short human-readable description (see Redaction). |

---

## Lifecycle Events

The timeline contains at most three entries, one per lifecycle stage:

| Sequence | Event         | Condition                        |
|----------|---------------|----------------------------------|
| 0        | `Opened`      | Always present when dispute exists |
| 1        | `UnderReview` | Present when status ≥ UnderReview |
| 2        | `Resolved`    | Present only when status = Resolved |

Entries are always returned in ascending sequence order.  Timestamps are
non-decreasing across entries.

---

## Redaction Rules

The endpoint applies the following redaction rules to prevent PII leakage
and protect privileged information:

| Field                          | Rule |
|--------------------------------|------|
| Evidence                       | **Never included.** Use `get_dispute_details` for authorized access. |
| `UnderReview` → `actor`        | Replaced with the zero address (`GAAA...WHF`) to hide admin identity. |
| `Resolved` → `summary`         | Contains resolution text only when `current_status == Resolved`. |
| `Opened` → `summary`           | Contains the dispute **reason** (not evidence). |
| Invoice metadata (customer, tax ID) | Not included in any entry. |

---

## Pagination

- `offset = 0, limit = 10` returns all entries for most disputes (max 3).
- `offset` beyond `total` returns an empty `entries` list with `has_more = false`.
- `limit = 0` returns an empty `entries` list.
- `limit` values above 50 are silently capped to 50.
- `total` always reflects the full event count regardless of `offset`/`limit`.

---

## Errors

| Error                | Code | Condition |
|----------------------|------|-----------|
| `InvoiceNotFound`    | 1000 | No invoice exists for the given `invoice_id`. |
| `DisputeNotFound`    | 1900 | Invoice exists but has no active dispute (`dispute_status == None`). |

---

## Security Notes

- No authorization is required to call this endpoint; it is a read-only
  query.  Sensitive fields are redacted at the data layer, not by access
  control, so the response is safe to return to any caller.
- Evidence is structurally excluded from the response type — it is not
  present in `DisputeTimelineEntry` at all.
- The zero-address sentinel for the `UnderReview` actor is
  `GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF` (all-zero
  Stellar public key).

---

## Example

### Request

```
get_dispute_timeline(invoice_id=<id>, offset=0, limit=10)
```

### Response (Resolved dispute)

```json
{
  "entries": [
    {
      "sequence": 0,
      "event": "Opened",
      "timestamp": 1700000000,
      "actor": "GBUSINESS...",
      "summary": "Payment not received after due date"
    },
    {
      "sequence": 1,
      "event": "UnderReview",
      "timestamp": 1700000100,
      "actor": "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
      "summary": ""
    },
    {
      "sequence": 2,
      "event": "Resolved",
      "timestamp": 1700000200,
      "actor": "GADMIN...",
      "summary": "Verified payment delay. Instructed business to release funds."
    }
  ],
  "total": 3,
  "has_more": false,
  "current_status": "Resolved"
}
```

### Response (Disputed, page 1 of 1)

```json
{
  "entries": [
    {
      "sequence": 0,
      "event": "Opened",
      "timestamp": 1700000000,
      "actor": "GBUSINESS...",
      "summary": "Invoice amount does not match contract"
    }
  ],
  "total": 1,
  "has_more": false,
  "current_status": "Disputed"
}
```

---

## Related Endpoints

| Endpoint                        | Description |
|---------------------------------|-------------|
| `create_dispute`                | Opens a dispute on an invoice. |
| `put_dispute_under_review`      | Admin advances dispute to UnderReview. |
| `resolve_dispute`               | Admin finalizes dispute with resolution text. |
| `get_dispute_details`           | Returns full dispute data including evidence (authorized). |
| `get_invoices_with_disputes`    | Lists all invoice IDs that have a dispute. |
| `get_invoices_by_dispute_status`| Filters invoice IDs by dispute status. |

---

## Implementation

- **Source**: `quicklendx-contracts/src/dispute_timeline.rs`
- **Tests**: `quicklendx-contracts/src/test_dispute_timeline.rs`
- **Types**: `DisputeTimeline`, `DisputeTimelineEntry` (both `#[contracttype]`)
- **Pagination cap**: `TIMELINE_MAX_PAGE_SIZE = 50`

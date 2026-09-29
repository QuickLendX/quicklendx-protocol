# QuickLendX Event Ingestion API

This document describes the accepted event schemas for the POST `/api/v1/events` endpoint. Send either a single event object or an array of event objects. Unknown or malformed events are rejected with a structured 400 response and raw payload values are not echoed in validation errors.

> **Note for Operators:** For recommendations on which events to alert on and reasonable volume thresholds, see the [Monitoring Guide](MONITORING.md).

## Supported Event Types

### InvoiceSettled

```
{
  "id": "string (unique)",
  "ledger": number,
  "txHash": "string",
  "type": "InvoiceSettled",
  "payload": {
    "invoice_id":      "string",
    "amount":          "string",
    "ledger":          number,
    "business":        "string",
    "investor":        "string",
    "investor_return": "string",
    "platform_fee":    "string",
    "timestamp":       number
  },
  "timestamp": number,
  "complianceHold": boolean,
  "indexedAt": "ISO 8601 string"
}
```

### PaymentRecorded

```
{
  "id": "string (unique)",
  "ledger": number,
  "txHash": "string",
  "type": "PaymentRecorded",
  "payload": {
    "invoice_id": "string",
    "payer": "string",
    "amount": "string",
    ...
  },
  "timestamp": number,
  "complianceHold": boolean,
  "indexedAt": "ISO 8601 string"
}
```

### DisputeCreated

```
{
  "id": "string (unique)",
  "ledger": number,
  "txHash": "string",
  "type": "DisputeCreated",
  "payload": {
    "invoice_id": "string",
    "initiator": "string",
    ...
  },
  "timestamp": number,
  "complianceHold": boolean,
  "indexedAt": "ISO 8601 string"
}
```

### DisputeResolved

```
{
  "id": "string (unique)",
  "ledger": number,
  "txHash": "string",
  "type": "DisputeResolved",
  "payload": {
    "invoice_id": "string",
    "resolved_by": "string",
    ...
  },
  "timestamp": number,
  "complianceHold": boolean,
  "indexedAt": "ISO 8601 string"
}
```

## Validation Rules

- All fields are required unless marked optional.
- Unknown event types are rejected.
- Each event must have a stable unique `id`; replaying a previously processed `id` is a no-op.
- Processed event IDs are persisted in the raw event log backing store.
- Oversized batches (>100 events) are rejected before event processing.
- No raw event payloads are echoed in error responses.

## Response Shape

Successful events return `status: "processed"`. Duplicate events return `status: "duplicate"` and do not trigger notifications again. Invalid items return `status: "rejected"` with sanitized validation errors.

Mixed batches return a per-event `results` array. If any item is rejected, the HTTP status is 400 while valid non-duplicate items in the same batch may still be processed.

## Example Batch

```
[
  { ...InvoiceSettled },
  { ...PaymentRecorded },
  { ...DisputeCreated },
  { ...DisputeResolved }
]
```

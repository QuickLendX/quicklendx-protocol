# QuickLendX Event Ingestion API

This document describes the outer envelope for the `POST /api/v1/events`
endpoint. Send either a single event object or an array of event objects.
Unknown or malformed events are rejected with a structured `400` response and
raw payload values are not echoed in validation errors.

> **Note for Operators:** For recommendations on which events to alert on and
> reasonable volume thresholds, see the [Monitoring Guide](MONITORING.md).

## Event Envelope

```json
{
  "id": "string (unique)",
  "ledger": 0,
  "txHash": "string",
  "type": "<EventType>",
  "payload": {},
  "timestamp": 0,
  "complianceHold": false,
  "indexedAt": "ISO 8601 string"
}
```

## Authoritative Event Payloads

Per-event topics, wire formats, and payload field definitions are no longer
duplicated here. Use the generated machine-readable schema instead:

- **Schema:** [`docs/events.schema.json`](./events.schema.json)
- **Contract source:** [`quicklendx-contracts/src/events_schema.rs`](../quicklendx-contracts/src/events_schema.rs)
- **Regenerate:** `scripts/gen-events-schema.sh`

# QuickLendX — Complete Event Schema Reference

The authoritative, machine-readable reference for every event emitted by the
QuickLendX protocol is generated directly from the smart contract schema table:

- **Generated JSON schema:** [`docs/events.schema.json`](./events.schema.json)
- **Source table:** [`quicklendx-contracts/src/events_schema.rs`](../quicklendx-contracts/src/events_schema.rs)
- **Generator script:** `scripts/gen-events-schema.sh`

Indexers should treat field names, topic strings, and ordering in
`docs/events.schema.json` as stable unless a breaking-change notice appears in
the changelog.

---

## General Envelope

All events share a common outer envelope when delivered via the ingestion API
(`POST /api/v1/events`):

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

The `type` field determines which payload schema from
[`docs/events.schema.json`](./events.schema.json) applies.

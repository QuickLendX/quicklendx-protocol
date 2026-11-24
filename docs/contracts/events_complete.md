# QuickLendX Event System — Complete Reference

## Overview

The QuickLendX protocol emits structured events for all critical operations to
enable off-chain indexing, monitoring, and frontend updates.

### Event Architecture

- **Authoritative Schema**: [`docs/events.schema.json`](../events.schema.json)
  (generated from [`quicklendx-contracts/src/events_schema.rs`](../../quicklendx-contracts/src/events_schema.rs)
  via `scripts/gen-events-schema.sh`)
- **Timestamps**: Events include ledger-generated timestamps for chronological indexing
- **Authorization**: State-mutating events reflect actions gated by `require_auth()`
- **Immutability**: Events are blockchain-recorded and immutable
- **Schema Parity**: `cargo test --lib test_events_schema` verifies every
  emitter in `quicklendx-contracts/src/events.rs` against `EVENT_SCHEMA`

---

## Full Event Catalog

To prevent documentation drift, individual event topics and payload field lists
are no longer duplicated in this file. Refer to
[`docs/events.schema.json`](../events.schema.json) for the complete,
up-to-date catalog of all events, topics, wire formats (`contractevent`,
`tuple`, `unit`), topic fields, and payload fields.

# Backend Compatibility — Contract and Event Schema Versioning

This document describes how the QuickLendX indexer labels every record it
writes with two version numbers, and how those labels should be used when
upgrading contracts, rolling back, or maintaining parallel indexer pipelines.

---

## Concepts

### Contract Version

Corresponds to `PROTOCOL_VERSION: u32` in
`quicklendx-contracts/src/init.rs`. It is written to on-chain instance
storage at `"proto_ver"` during `initialize()` and can be read via
`get_version()` on the contract ABI.

**When to bump:**
| Change type | Bump? |
|---|---|
| Bug fix with no storage or event changes | No |
| New optional field appended to an event payload | No |
| New field added to on-chain storage struct | Recommended |
| Existing field removed or renamed | **Mandatory** |
| Storage key namespace changed | **Mandatory** |

### Event Schema Version

Tracks the shape of event payloads as described in
`docs/contracts/events.md`. Each event topic string maps to the schema
version that produced it (see `EVENT_TOPIC_SCHEMA_VERSIONS` in
`backend/src/services/versioningService.ts`).

**Stability rules (from `docs/contracts/events.md`):**
- Topic strings are **frozen once deployed** — no renames, no removals.
- Payload fields are **append-only** — existing field positions are frozen;
  new fields always go at the end.
- Adding a new field at the end of a payload does **not** require a schema
  version bump.
- Changing the type or position of an existing field **mandates** a new topic
  string and a schema version bump.

---

## Version Labels on Indexed Records

Every record stored by the indexer carries three fields:

| Field | Type | Description |
|---|---|---|
| `contract_version` | `number` | Contract version active when the event was emitted |
| `event_schema_version` | `number` | Event schema version of the originating event topic |
| `indexed_at` | `string` (ISO 8601) | UTC timestamp when the indexer wrote the record |

These fields are set exclusively by `labelRecord()` in
`backend/src/services/versioningService.ts` at ingest time. They are **never
accepted from user input** — query parameters or request bodies cannot
influence version labels.

### Example record (JSON)

```json
{
  "id": "0xabc...",
  "amount": "1000000000",
  "status": "Verified",
  "contract_version": 1,
  "event_schema_version": 1,
  "indexed_at": "2026-04-24T10:00:00.000Z"
}
```

---

## Safe Upgrade Procedure

### Step 1 — Deploy the new contract

1. Increment `PROTOCOL_VERSION` in `quicklendx-contracts/src/init.rs`.
2. If event payloads changed, add the new topic string to
   `EVENT_TOPIC_SCHEMA_VERSIONS` with the new schema version number.
3. Deploy the upgraded contract. **Do not stop the existing indexer yet.**

### Step 2 — Run parallel indexers

During a rollout, two contract versions may be active simultaneously
(old deployment vs. new deployment). The indexer correctly labels records
from each:

```
Old contract  → contract_version: 1, event_schema_version: 1
New contract  → contract_version: 2, event_schema_version: 1   (if only storage changed)
New contract  → contract_version: 2, event_schema_version: 2   (if event payloads changed)
```

Consumers can filter by `contract_version` to handle each cohort
independently.

### Step 3 — Migrate historical records (if required)

If breaking storage changes require a backfill, re-index historical events
using the correct `contractVersion` and `eventSchemaVersion` arguments to
`labelRecord()`.

### Step 4 — Drain the old indexer

Once all records from the old contract version have been indexed and
confirmed, decommission the old indexer pipeline.

---

## Compatibility Matrix

| API consumer | Recommended approach |
|---|---|
| Reads only current records | Filter `contract_version === CURRENT_CONTRACT_VERSION` |
| Must handle legacy records | Branch on `contract_version` and/or `event_schema_version` |
| Migration tooling | Group by `contract_version`, process each cohort independently |
| Audit / compliance | Store and expose all three version fields without modification |

---

## Security Assumptions

1. **Versions are derived from trusted sources only.** The indexer reads the
   contract version from the on-chain `"proto_ver"` key and the event schema
   version from its own `EVENT_TOPIC_SCHEMA_VERSIONS` map. No external caller
   can inject a version label.

2. **`labelRecord()` validates its inputs.** Passing a version number less
   than 1, a non-integer, or `NaN` will throw a `RangeError` immediately,
   preventing corrupt labels from reaching storage.

3. **`indexed_at` reflects indexer wall-clock time.** It is set at the moment
   the indexer writes the record and is not derived from the event timestamp,
   preventing clients from inferring ingest latency from user-supplied data.

---

## Adding a New Event Topic

1. Add the topic string and its initial schema version to
   `EVENT_TOPIC_SCHEMA_VERSIONS` in `versioningService.ts`.
2. Add the topic to `docs/contracts/events.md` with its full payload
   definition.
3. Add a test case in `backend/tests/versioning.test.ts` under the
   `resolveEventSchemaVersion` suite.

---

## References

- Contract version constant: `quicklendx-contracts/src/init.rs` — `PROTOCOL_VERSION`
- Event stability policy: `docs/contracts/events.md`
- On-chain storage schema: `docs/contracts/storage-schema.md`
- Versioning service: `backend/src/services/versioningService.ts`
- Type definitions: `backend/src/types/contract.ts` — `VersionedRecord`
- Tests: `backend/tests/versioning.test.ts`

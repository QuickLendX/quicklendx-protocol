# Event Schema Stability — `events.rs`

The authoritative catalog of every Soroban event emitted by QuickLendX —
including exact topic strings, wire formats, topic fields, and payload fields —
is maintained in [`docs/events.schema.json`](../events.schema.json) (generated
from [`quicklendx-contracts/src/events_schema.rs`](../../quicklendx-contracts/src/events_schema.rs)
via `scripts/gen-events-schema.sh`).

Off-chain indexers and analytics tools **must** consume
[`docs/events.schema.json`](../events.schema.json) rather than hand-copied
tables.

---

## Stability Policy

> **Topics are frozen.** Once deployed on a network, no existing topic string
> will be renamed or removed. New events may be added in future contract
> upgrades. Payload fields follow **append-only ordering**: existing field
> positions are frozen; new fields are appended at the end.

---

## Security Notes

| Property | Guarantee |
|---|---|
| Timestamps | Always from `env.ledger().timestamp()` — tamper-proof Soroban clock |
| No PII | Only addresses, identifiers, and amounts are emitted |
| Auth context | All state-mutating events are gated by `require_auth()` upstream |
| Read-only purity | Getter functions emit zero events |

---

## Verification

```bash
cd quicklendx-contracts && cargo test --lib test_events_schema
scripts/gen-events-schema.sh && git diff --exit-code docs/events.schema.json
```

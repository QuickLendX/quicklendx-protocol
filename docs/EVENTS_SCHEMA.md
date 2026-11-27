# QuickLendX — Contract Events Schema

> **Audience:** Off-chain integrators (indexers, dashboards, webhook consumers)
> who need to subscribe to and decode events emitted by the QuickLendX Soroban
> contract.

---

## Authoritative Schema

The complete, machine-readable list of every event topic, wire format, topic
field, and payload field is maintained in
[`docs/events.schema.json`](./events.schema.json).

- **Source of truth:** [`quicklendx-contracts/src/events_schema.rs`](../quicklendx-contracts/src/events_schema.rs)
- **Generator:** `scripts/gen-events-schema.sh`
- **Verification:** `cd quicklendx-contracts && cargo test --lib test_events_schema`

Every public emitter in `quicklendx-contracts/src/events.rs` is verified against
`EVENT_SCHEMA` in CI, so `docs/events.schema.json` is guaranteed not to drift
from on-chain emissions.

---

## Overview

The QuickLendX contract emits structured events via `#[contractevent]` structs
(map-encoded payloads with named fields) and legacy `env.events().publish`
calls (positional tuple or unit payloads). Each event has:

| Field    | Type       | Description                                                                   |
|----------|------------|-------------------------------------------------------------------------------|
| `topics` | `Vec<Val>` | `[Symbol(topic_name), ...]` — used for filtering                              |
| `data`   | `Val`      | Event payload (`Map<Symbol, Val>` for `contractevent`, `Vec<Val>` for `tuple`) |

Events are available on-chain via `getEvents` RPC and off-chain via
[Horizon](https://developers.stellar.org/api) transaction result meta.

---

## How to Subscribe (JavaScript/TypeScript)

```typescript
import { SorobanRpc } from "@stellar/stellar-sdk";
import eventsSchema from "./events.schema.json";

const server = new SorobanRpc.Server("https://soroban-testnet.stellar.org");

const events = await server.getEvents({
  startLedger: 1000000,
  filters: [
    {
      type: "contract",
      contractIds: ["<CONTRACT_ID>"],
    },
  ],
});
```

Consult [`docs/events.schema.json`](./events.schema.json) for the exact `topic`,
`format`, `topic_fields`, and `fields` of each of the protocol's events.

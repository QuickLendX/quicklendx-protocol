# On-Chain Logs: Tx-Level vs Contract-Level Events

> **Audience**: downstream integrators building indexers, analytics pipelines,
> or notification systems on top of QuickLendX.

Stellar exposes two distinct event surfaces for a Soroban transaction. This
document explains what each surface contains, how to subscribe to each, and
which QuickLendX contract events land where.

---

## Two event surfaces

### 1. Transaction-level (tx-meta)

Stellar's `TransactionMeta` XDR includes every `ContractEvent` emitted during
a transaction's execution, regardless of which contract emitted it. You get
these by:

- Polling `GET /transactions/{hash}` on Horizon and parsing the `result_meta_xdr`.
- Streaming `transactions` via the Horizon SSE endpoint.
- Using the Soroban RPC method `getTransaction` which returns
  `resultMetaXdr`.

**Characteristics**

- Contains _all_ events from _all_ contracts in the call stack (including
  cross-contract calls).
- Ordered by emission order within the transaction.
- Includes both `contract` events (emitted by `env.events().publish(...)`)
  and `system` events (e.g. token transfers emitted by the Stellar asset
  contract).
- Not filterable server-side; your indexer must filter locally by contract
  address and/or topic.

### 2. Contract-level (Soroban event API)

The Soroban RPC exposes `getEvents` which lets you subscribe to events by
contract address and optional topic filter:

```
POST /soroban/rpc
{
  "method": "getEvents",
  "params": {
    "startLedger": 1234567,
    "filters": [
      {
        "type": "contract",
        "contractIds": ["C...your_contract_address..."],
        "topics": [["*", "AAAADwAAAAtpbnZfaW5fdXA="]]
      }
    ]
  }
}
```

**Characteristics**

- Filtered server-side to one contract; much cheaper to process at high volume.
- Topics are base64-encoded XDR `ScVal` values. Use the SDK's `Symbol::new`
  to derive them (see example below).
- Events are paginated via `cursor`; store the cursor to resume after a
  restart.
- The node retains events for a configurable number of ledgers
  (`LEDGER_RETENTION_WINDOW`, typically 17,280 ledgers ≈ 24 h on Testnet).

---

## Which surface to use

| Situation | Use |
|---|---|
| You only care about one contract | Soroban `getEvents` — server-side filtering reduces load |
| You need to correlate multiple contracts (e.g. SAC token transfers + QuickLendX escrow) in the same tx | Tx-meta — all events are present together |
| You need guaranteed ordering across event types in a single call | Tx-meta — emission order is preserved |
| You are building a real-time dashboard for a specific topic | Soroban `getEvents` with a topic filter |
| You are backfilling historical data | Both; `getEvents` has a retention window, tx-meta is permanent on an archival node |

---

## QuickLendX contract events

All events are emitted via one of two patterns:

**Pattern A — `#[contractevent]` struct** (most protocol events):

```rust
// src/events.rs
#[contractevent]
pub struct BidPlaced {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub bid_amount: i128,
    pub expected_return: i128,
    pub timestamp: u64,
    pub expiration_timestamp: u64,
}
```

The `#[contractevent]` macro calls `env.events().publish(...)` internally,
encoding the struct as a `Map<Symbol, Val>` keyed by field name. The first
topic is derived from the struct name in snake_case.

**Pattern B — raw `env.events().publish(...)`** (admin/operational events):

```rust
// src/events.rs
env.events().publish(
    (symbol_short!("adm_trf"),),
    (old_admin.clone(), new_admin.clone(), env.ledger().timestamp()),
);
```

These emit a tuple as data rather than a named map. The short symbol is the
first (and only) topic.

### Full topic table

All topic strings exported from `src/events.rs` as `TOPIC_*` constants:

| `TOPIC_*` constant | Topic string | Emitted when |
|---|---|---|
| `TOPIC_INVOICE_UPLOADED` | `"invoice_uploaded"` | Business uploads a new invoice |
| `TOPIC_INVOICE_VERIFIED` | `"invoice_verified"` | Admin verifies an invoice |
| `TOPIC_INVOICE_CANCELLED` | `"invoice_cancelled"` | Business cancels an invoice |
| `TOPIC_INVOICE_SETTLED` | `"invoice_settled"` | Invoice fully settled |
| `TOPIC_INVOICE_SETTLED_FINAL` | `"invoice_settled_final"` | All funds disbursed after settlement |
| `TOPIC_INVOICE_DEFAULTED` | `"invoice_defaulted"` | Invoice marked defaulted |
| `TOPIC_INVOICE_EXPIRED` | `"invoice_expired"` | Invoice expires past due date |
| `TOPIC_INVOICE_FUNDED` | `"invoice_funded"` | Investor funds an invoice |
| `TOPIC_PARTIAL_PAYMENT` | `"partial_payment"` | Partial payment applied |
| `TOPIC_PAYMENT_RECORDED` | `"payment_recorded"` | Payment durably stored |
| `TOPIC_BID_PLACED` | `"bid_placed"` | Investor places a bid |
| `TOPIC_BID_ACCEPTED` | `"bid_accepted"` | Business accepts a bid |
| `TOPIC_BID_WITHDRAWN` | `"bid_withdrawn"` | Investor withdraws their bid |
| `TOPIC_BID_EXPIRED` | `"bid_expired"` | Bid TTL expires |
| `TOPIC_ESCROW_CREATED` | `"escrow_created"` | Funds locked in escrow |
| `TOPIC_ESCROW_RELEASED` | `"escrow_released"` | Funds released to business |
| `TOPIC_ESCROW_REFUNDED` | `"escrow_refunded"` | Funds returned to investor |
| `TOPIC_INVESTMENT_WITHDRAWN` | `"investment_withdrawn"` | Investor withdraws before settlement |
| `TOPIC_DISPUTE_CREATED` | `"dispute_created"` | Dispute opened |
| `TOPIC_DISPUTE_UNDER_REVIEW` | `"dispute_under_review"` | Dispute enters review |
| `TOPIC_DISPUTE_RESOLVED` | `"dispute_resolved"` | Dispute resolved |

Semantic aliases (`InvoiceCreated`, `FundsLocked`, `LoanSettled`,
`DisputeOpened`) are type aliases to the canonical structs above and share
the same topics.

For `PauseBlocked` events see [docs/pause-events.md](pause-events.md).

---

## Subscribing via Soroban RPC — worked example

The goal: collect every `BidPlaced` event for a specific contract, resuming
from a stored cursor.

```typescript
import { SorobanRpc, xdr, nativeToScVal } from "@stellar/stellar-sdk";

const server = new SorobanRpc.Server("https://soroban-testnet.stellar.org");

// Topic bytes for "bid_placed" — encode as a Symbol ScVal
const bidPlacedTopic = xdr.ScVal.scvSymbol(Buffer.from("bid_placed"));

async function fetchBidPlacedEvents(
  contractId: string,
  startLedger: number,
  cursor?: string
): Promise<SorobanRpc.Api.GetEventsResponse> {
  return server.getEvents({
    startLedger,
    filters: [
      {
        type: "contract",
        contractIds: [contractId],
        topics: [[bidPlacedTopic.toXDR("base64")]],
      },
    ],
    cursor,
    limit: 100,
  });
}

// Usage
let cursor: string | undefined;
while (true) {
  const resp = await fetchBidPlacedEvents("C...", 1234567, cursor);
  for (const ev of resp.events) {
    // ev.value is the encoded Map<Symbol, Val> from the #[contractevent] struct
    console.log(ev.id, ev.value);
  }
  cursor = resp.latestLedger.toString(); // or track per-event cursor
  await new Promise((r) => setTimeout(r, 5_000));
}
```

The `ev.value` XDR decodes to a `Map<Symbol, Val>` with keys matching the
Rust struct field names (`bid_id`, `invoice_id`, `investor`, `bid_amount`,
`expected_return`, `timestamp`, `expiration_timestamp`).

---

## Reading events in contract tests

In `soroban-sdk` test environments `env.events().all()` returns every emitted
event. The test helper pattern used throughout `src/test_events.rs`:

```rust
use soroban_sdk::{xdr, Symbol, Val, TryFromVal};

fn find_event_data(env: &Env, topic: &str) -> soroban_sdk::Map<Symbol, Val> {
    let topic_sym = Symbol::new(env, topic);
    let topic_xdr = xdr::ScVal::try_from_val(env, &topic_sym).unwrap();
    for ev in env.events().all().events().iter().rev() {
        if let xdr::ContractEventBody::V0(b) = &ev.body {
            if b.topics.first() == Some(&topic_xdr) {
                let data = Val::try_from_val(env, &b.data).unwrap();
                return soroban_sdk::Map::try_from_val(env, &data).unwrap();
            }
        }
    }
    panic!("event '{}' not found", topic);
}

#[test]
fn bid_placed_event_fields() {
    // ... setup omitted, see src/test_events.rs for full scaffolding ...
    let data = find_event_data(&env, TOPIC_BID_PLACED);
    let bid_amount: i128 = data
        .get(Symbol::new(&env, "bid_amount"))
        .unwrap()
        .try_into_val(&env)
        .unwrap();
    assert_eq!(bid_amount, 1_500_000);
}
```

Read-only entrypoints (queries, analytics, search) emit **no** events. This
is verified in `test_no_events_emitted_for_reads` in `src/test_events.rs`.

---

## Related docs

- [docs/events_complete.md](events_complete.md) — full per-event field reference
- [docs/pause-events.md](pause-events.md) — `PauseBlocked` event details
- [docs/diagnostics.md](diagnostics.md) — health and diagnostics endpoints
- [docs/backfill.md](backfill.md) — replaying historical events

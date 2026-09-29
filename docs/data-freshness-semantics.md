# Data Freshness Semantics

Closes #879.

Every QuickLendX API response is wrapped in an envelope that includes
`freshness` metadata alongside the `data` payload. Clients use this to
determine whether the data they display is near-real-time or lagging behind
the chain, and to warn users before they act on stale financial information.

## Response Envelope

```json
{
  "data": { ... },
  "freshness": {
    "lastIndexedLedger": 1042857,
    "indexLagSeconds": 10,
    "lastUpdatedAt": "2024-03-15T10:22:41Z",
    "cursor": "1042857_0"
  }
}
```

## Fields

| Field | Type | Description |
|---|---|---|
| `lastIndexedLedger` | `number` (u32) | Last ledger sequence number processed by the indexer |
| `indexLagSeconds` | `number` (f64) | Seconds the indexer is behind the chain tip. `0` when current. Computed as `lagLedgers × 5` (Stellar's ~5 s/ledger constant) |
| `lastUpdatedAt` | ISO 8601 UTC string | Wall-clock time of the indexed ledger close: `"YYYY-MM-DDTHH:MM:SSZ"` |
| `cursor` | opaque string | Pagination/replay cursor: `"<ledger_seq>_<offset>"` — treat as a black box |

## UI-Safe Semantics

Clients **must** surface lag to users making financial decisions (lending,
investing, accepting bids). The following thresholds are recommended:

| `indexLagSeconds` | Recommended UI behaviour |
|---|---|
| `0` | Show data as current |
| `1 – 30` | No warning required |
| `31 – 120` | Show a subtle "data may be slightly delayed" notice |
| `> 120` | Show a prominent warning: "Data is lagging. Financial decisions may be based on stale information." Consider blocking irreversible actions. |

**Never hide the lag indicator.** Users making lending or investment decisions
must be able to see whether the data reflects the current chain state.

## Cursor Usage

The cursor is opaque to clients. To paginate:

1. Call any data endpoint with no cursor (offset defaults to `0`).
2. The response includes a `cursor` in `freshness`.
3. Store the cursor alongside the result set.
4. Pass the cursor's embedded offset to the next query to resume from the same
   ledger position.

Do **not** parse or construct cursors manually. The format may change.
Cursors are not signed — do not use them for access control.

## Lag Computation

`indexLagSeconds = max(0, chainTipLedger − lastIndexedLedger) × 5`

The constant `5` seconds per ledger is the Stellar protocol average. No
internal node addresses, validator identities, peer lists, or network topology
are included in any freshness field.

## Security Assumptions

- `lastIndexedLedger` and `indexLagSeconds` expose only public ledger sequence
  numbers and a derived time estimate. No internal infrastructure details are
  included.
- `lastUpdatedAt` is derived from wall-clock time minus the computed lag — no
  external time source is used.
- `cursor` contains only decimal digits and an underscore separator. It cannot
  be reversed to reveal internal state.
- The Rust contract layer (`get_freshness`) derives all values from
  `env.ledger().timestamp()` and `env.ledger().sequence()`, which are
  tamper-proof in the Soroban execution environment.

## Contract Endpoint

The on-chain equivalent is available via:

```
get_freshness(indexed_ledger_seq: u32, indexed_ledger_timestamp: u64, offset: u32)
  → Map<String, String>
```

See [`quicklendx-contracts/docs/contracts/freshness.md`](../quicklendx-contracts/docs/contracts/freshness.md)
for the contract-level specification.

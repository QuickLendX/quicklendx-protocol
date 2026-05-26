# Data Freshness Semantics

Closes #879.

Every QuickLendX API response can be accompanied by freshness metadata so
clients know whether the data they display is near-real-time or lagging behind
the chain.

## Endpoint

```
get_freshness(indexed_ledger_seq: u32, indexed_ledger_timestamp: u64, offset: u32)
  → Map<String, String>
```

Call this alongside any data query. Pass the last ledger sequence and its close
timestamp that your indexer has processed, plus the current pagination offset.

## Fields

| Key | Type | Description |
|---|---|---|
| `last_indexed_ledger` | decimal string | Last ledger sequence number the indexer has processed |
| `index_lag_seconds` | decimal string (i64) | Seconds between the indexed ledger's close time and the current ledger's close time. `"0"` when the indexed ledger is the current ledger |
| `last_updated_at` | ISO 8601 string | UTC close time of the indexed ledger: `"YYYY-MM-DDTHH:MM:SSZ"` |
| `cursor` | opaque string | Pagination/replay cursor: `"<ledger_seq>_<offset>"` |

## UI-Safe Semantics

| `index_lag_seconds` | Recommended UI behaviour |
|---|---|
| `0` | Show data as current |
| `1 – 30` | No warning required |
| `31 – 120` | Show a subtle "data may be slightly delayed" notice |
| `> 120` | Show a prominent warning: "Data is lagging. Financial decisions may be based on stale information." |

**Never hide the lag indicator.** Users making lending or investment decisions
must be able to see whether the data reflects the current chain state.

## Cursor Usage

The cursor is opaque to clients. To paginate:

1. Call any data query with `offset = 0`.
2. Call `get_freshness` with the same `offset` to get a cursor.
3. Store the cursor alongside the result set.
4. To resume from the same position, decode the cursor as `"<seq>_<offset>"` and
   pass `offset` to the next query.

Cursors are not signed or encrypted — they are purely informational. Do not
use them for access control.

## Security Assumptions

- Only public ledger data is returned: ledger sequence, ledger close timestamp,
  and a derived pagination offset.
- No node addresses, validator identities, peer lists, or network topology are
  included.
- `index_lag_seconds` is derived entirely from `env.ledger().timestamp()` and
  `env.ledger().sequence()`, which are tamper-proof in the Soroban execution
  environment.
- The ISO 8601 timestamp is computed from the ledger close time using pure
  integer arithmetic — no external time source is used.

## Example Response

```json
{
  "last_indexed_ledger": "1042857",
  "index_lag_seconds": "4",
  "last_updated_at": "2024-03-15T10:22:41Z",
  "cursor": "1042857_0"
}
```

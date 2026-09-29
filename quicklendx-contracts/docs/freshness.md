# Freshness Drift Model

`get_freshness(indexed_ledger_seq, indexed_ledger_timestamp, offset)` returns
transport-friendly metadata that lets clients decide whether indexed data is
still safe to display or act on.

The contract does not return a boolean stale flag. Instead, clients derive it
from `index_lag_seconds`, which is computed as:

```text
current ledger timestamp - indexed ledger timestamp
```

## Drift Bound

The documented freshness bound is
`DEFAULT_MAX_FRESHNESS_DRIFT_SECS = 120`.

Client interpretation:

| `index_lag_seconds` | Freshness state | Recommended behavior |
|---|---|---|
| `<= 30` | Fresh | No warning required. |
| `31..=120` | Delayed but usable | Show a subtle delayed-data notice. |
| `> 120` | Stale | Show a prominent stale-data warning and block high-risk financial actions. |

The boundary is inclusive: `120` seconds is still usable, while `121` seconds
is stale. Tests in `src/test_freshness_bounds.rs` pin that one-ledger-second
transition so future changes cannot silently move the threshold.

## Monotonicity

For a fixed current ledger timestamp, increasing elapsed time must never report
fresher data. In practice, older indexed ledger timestamps produce larger
`index_lag_seconds` values. This keeps client-side stale checks stable across
pagination and refresh loops.

## Edge Cases

- When the indexed timestamp equals the current ledger timestamp, the lag is
  `0` and the data is fresh.
- At Unix epoch start, `0 - 0` is fresh.
- If the indexed timestamp is in the future, `index_lag_seconds` is negative.
  Treat this as clock/indexer skew, not stale data. Clients should show a
  diagnostic or retry, but should not permanently advance cursors from a future
  timestamp without a later consistent read.

## Client Guidance

Clients should:

1. Call `get_freshness` alongside any indexed data read.
2. Parse `index_lag_seconds` as an `i64`.
3. Treat values above `DEFAULT_MAX_FRESHNESS_DRIFT_SECS` as stale.
4. Keep `cursor` opaque; it is pagination metadata, not an authorization token.
5. Keep showing the lag indicator wherever indexed state affects financial
   decisions.

The detailed response-field contract is documented in
[`docs/contracts/freshness.md`](contracts/freshness.md).

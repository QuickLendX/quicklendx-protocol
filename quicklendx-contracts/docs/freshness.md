# Freshness Metadata

QuickLendX exposes freshness metadata through `get_freshness` so clients can
decide whether off-chain indexed data is current enough for user actions.

## Model

Clients pass the ledger sequence and timestamp of the data they indexed. The
contract compares that indexed timestamp with the current ledger timestamp and
returns a transport-friendly map containing:

| Key | Meaning |
| --- | --- |
| `last_indexed_ledger` | Ledger sequence represented by the indexed data |
| `index_lag_seconds` | Current ledger timestamp minus indexed ledger timestamp |
| `max_freshness_drift_seconds` | Maximum accepted positive lag before stale |
| `is_stale` | `"true"` when lag is above the drift bound, otherwise `"false"` |
| `last_updated_at` | Indexed timestamp rendered as UTC ISO-8601 |
| `cursor` | Stable pagination cursor in `ledger_offset` form |

Negative lag can occur if a caller supplies an indexed timestamp ahead of the
current ledger timestamp. Negative lag is not treated as stale by this signal;
callers should still validate their own indexer clock assumptions.

## Drift Bound

The default drift bound is `300` seconds.

Freshness uses a strict stale comparison:

```text
index_lag_seconds > max_freshness_drift_seconds
```

That means:

| Lag | `is_stale` |
| ---: | --- |
| `299` | `"false"` |
| `300` | `"false"` |
| `301` | `"true"` |

The signal is monotonic with elapsed time for the same indexed timestamp. Once
the lag crosses the bound, additional elapsed time must continue to report
stale.

## Client Guidance

- Treat `is_stale = "true"` as a safety signal. Disable actions that depend on
  current indexed state, such as acting on invoice or bid lists from an indexer.
- Continue to allow read-only views when useful, but label data as stale and
  show the current `index_lag_seconds`.
- Use `max_freshness_drift_seconds` from the response instead of hard-coding
  `300` in clients.
- Retry after the indexer advances, then re-check `get_freshness` before
  enabling sensitive actions.
- Do not use freshness as an authorization check; it is an operational safety
  indicator for clients and operators.


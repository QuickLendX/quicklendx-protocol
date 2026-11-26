# Currency whitelist

The currency whitelist controls which token contracts may be used for invoice currencies.

## Semantics

- An empty whitelist preserves the legacy open-currency behavior: all currencies are accepted.
- Once at least one currency is configured, only listed token addresses are accepted.
- Mutations require the stored admin address to authorize the call and are rejected while the protocol is paused or an upgrade is pending.
- Batch mutation calls are bounded by the protocol batch-input limit.
- Repeating an add is idempotent. Removing an absent currency is a no-op.
- Replacing or clearing the list changes the whitelist generation, invalidating stale pagination cursors.

## Contract entrypoints

| Entrypoint | Purpose |
|---|---|
| `add_currency(admin, currency)` | Add one token address |
| `add_currencies_batch(admin, currencies)` | Add multiple token addresses |
| `remove_currency(admin, currency)` | Remove one token address |
| `remove_currencies_batch(admin, currencies)` | Remove multiple token addresses |
| `set_currencies(admin, currencies)` | Replace the complete whitelist |
| `clear_currencies(admin)` | Clear the whitelist |
| `get_whitelisted_currencies()` | Read the complete list (legacy, unpaged) |
| `get_whitelisted_currencies_paged(cursor, limit)` | Read a cursor-paginated page |

Paged queries enforce the shared `MAX_QUERY_LIMIT` cap. Clients should follow the returned opaque `next_cursor` rather than constructing offsets. A cursor from an older whitelist generation is rejected after a mutation.

## Analytics and audit queries

The contract exposes read-only analytics entrypoints for platform metrics, versioned snapshots, financial/performance summaries, and generated business/investor reports. Audit entrypoints expose filtered cursor-paginated results, invoice/actor/operation pages, individual audit entries, and aggregate audit statistics. Paged reads use the same bounded query limit.

## Example

1. Initialize the contract admin.
2. Call `add_currency(admin, token_address)` (or the batch variant).
3. Query `get_whitelisted_currencies_paged(None, limit)` and continue with `next_cursor`.
4. Remove or replace currencies through the admin-only mutation entrypoints when needed.

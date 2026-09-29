# Storage TTL Policy

This document outlines the persistent storage lifecycle management for the
QuickLendX Protocol on Soroban.

## Overview

Soroban persistent storage entries have a finite Time-To-Live (TTL). If an
entry's TTL is not extended before it expires, the host reclaims the storage
and the entry becomes permanently inaccessible. This document describes the
protocol's strategy for keeping essential data alive.

## Extendable Kinds

The following root indexes are covered by the TTL extension routine:

| Kind         | Source                                                       | Field in `ExtendReport`        |
|-------------|--------------------------------------------------------------|--------------------------------|
| Invoice     | `InvoiceStorage::get_all_invoice_ids` (all statuses)         | `invoices_refreshed`           |
| Bid         | `BidStorage::get_all_bids` (all bid IDs)                     | `bids_refreshed`               |
| Investment  | `InvestmentStorage::get_active_investment_ids`               | `investments_refreshed`        |
| Escrow      | Escrows linked to invoices via `EscrowStorage::get_escrow_by_invoice` | `escrows_refreshed`    |
| Currency    | `CurrencyWhitelist::get_whitelisted_currencies`              | `currencies_refreshed`         |

Each entry is extended by calling `env.storage().persistent().extend_ttl(key, threshold, threshold)` with the protocol-wide threshold defined in `PERSISTENT_TTL_THRESHOLD` (~30 days at 5 s/ledger).

## Entrypoint

`extend_protocol_ttl(env: Env, admin: Address) -> ExtendReport`

- **Authorization**: admin-only (checked via `AdminStorage::require_admin`).
- **Returns**: `ExtendReport` with per-kind counts.
- **Events**: Emits one `TtlExtended` event per kind that had at least one entry refreshed.
- **Idempotent**: Calling repeatedly within the same ledger produces the same
  report (assuming no entries were added or removed between calls).

### ExtendReport

```rust
pub struct ExtendReport {
    pub invoices_refreshed: u32,
    pub bids_refreshed: u32,
    pub investments_refreshed: u32,
    pub escrows_refreshed: u32,
    pub currencies_refreshed: u32,
}
```

All fields are zero when no data exists — the call is a safe no-op.

## Operational Schedule

The extension routine should be invoked **weekly** by a cron job, keeper
network, or admin script. A weekly cadence provides ample margin against
the ~30-day TTL threshold.

### Suggested runbook

1. **Authenticate** as the protocol admin (or use a dedicated automation key).
2. **Invoke** `extend_protocol_ttl` on-chain.
3. **Verify** the returned report: confirm each non-zero field is populated
   as expected.
4. **Monitor** for `TtlExtended` events. Off-chain indexers should alert if
   no `TtlExtended` events have been seen for 10+ days.

## Monitoring

The `TtlExtended` event schema:

| Topic          | Data fields           |
|----------------|-----------------------|
| `ttl_extended` | `kind: String`, `count: u32` |

Off-chain services should:
- Subscribe to the `ttl_extended` topic.
- Track the last-seen timestamp per kind.
- Alert if any kind has not been refreshed for > 10 days.

## Archival Risk

Failure to run `extend_protocol_ttl` regularly may result in **permanent data
loss**. Once Soroban host reclaims an expired entry:

- Invoice and bid records become inaccessible.
- Active investments and escrows may break.
- The currency whitelist may become empty, blocking new invoice creation.

There is **no recovery mechanism** for entries that have been garbage-collected
by the host. Operators must ensure the weekly extension job is reliable and
monitored.

## Pruning Terminal Invoices

`prune_terminal_invoices` removes fully-resolved (terminal-state) invoices from
persistent storage after a configurable retention window. This bounds long-term
storage costs and reduces the number of keys touched by each TTL-extension sweep.

### Eligible statuses

Only invoices in a **terminal status** may be pruned:

| Status      | Terminal timestamp field      |
|-------------|-------------------------------|
| `Paid`      | `settled_at`                  |
| `Defaulted` | `created_at` (fallback)       |
| `Cancelled` | `created_at` (fallback)       |
| `Refunded`  | `created_at` (fallback)       |

Invoices in `Pending`, `Verified`, or `Funded` status are **never** pruned,
regardless of age. This safety guard protects active and disputed invoices.

### Retention window

The caller supplies `older_than_secs` — a minimum age in ledger seconds. An
invoice is pruned only when:

```
terminal_timestamp + older_than_secs < current_ledger_timestamp
```

A value of `0` prunes all terminal invoices (useful for testing or full cleanup).
A value of `30 * 24 * 3600` (~30 days) matches the protocol's
`PERSISTENT_TTL_THRESHOLD` and is the recommended production setting.

### Irreversibility

Pruning removes the invoice record **and all secondary index entries** (status,
business, customer, tax_id, tag, category) from persistent storage. **There is no
undo.** Operators should test with a dry-run retention window (e.g., 10 years)
to inspect which invoices would be affected before running with a real window.

### Pagination

The prune is paginated with a max page size of 100. Pass `next_offset` from the
returned `PruneReport` as `offset` on the next call. Because pruning deletes
entries, the offset may shift between pages; callers should restart from
`offset = 0` when a page returns `scanned = 0`.

### Entrypoint

```rust
fn prune_terminal_invoices(
    env: Env,
    admin: Address,
    older_than_secs: u64,
    offset: u32,
    limit: u32,
) -> Result<PruneReport, QuickLendXError>
```

- **Authorization**: admin-only (checked via `AdminStorage::require_admin`).
- **Returns**: `PruneReport { scanned, pruned, next_offset }`.
- **Safe**: never touches non-terminal invoices regardless of age.

# Resource and Rate Limits (#2439)

## Overview

This document describes the bounded input/work limits and per-address mutation rate limiter added in issue #2439. These controls prevent oversized payloads from exhausting transaction budgets and limit the rate at which any single address can mutate on-chain state.

## Design

### Input Size Ceilings

Hard ceilings are enforced *before* any expensive parsing, hashing, or storage write. They are intentionally tighter than Soroban's Host limits alone to prevent a single oversized payload from consuming the entire transaction budget.

| Constant | Value | Enforced At |
|----------|-------|-------------|
| `MAX_INPUT_DESCRIPTION_BYTES` | 4,096 bytes | `store_invoice` |
| `MAX_INPUT_KYC_DATA_BYTES` | 8,192 bytes | `submit_kyc_application`, `submit_investor_kyc` |
| `MAX_INPUT_TAGS` | 50 | `store_invoice` |
| `MAX_INPUT_BATCH_SIZE` | 25 | Batch entrypoints |
| `MAX_INPUT_STATUS_BATCH_SIZE` | 100 | `get_invoices_by_status_batch` |
| `MAX_INPUT_LINE_ITEMS` | 50 | Metadata updates |

These are static bounds checked by helper functions (`require_description_bound`, `require_kyc_data_bound`, `require_tags_bound`, `require_batch_size_bound`, `require_status_batch_bound`, `require_line_items_bound`).

### Per-Address Mutation Rate Limiter

A sliding-window counter tracks how many state-mutating calls each address issues within a contiguous range of ledger sequences.

**Constants:**
- `RATE_LIMIT_WINDOW_SEQUENCES`: 20 ledger sequences per window
- `MAX_MUTATIONS_PER_WINDOW`: 30 mutations per window

**Storage layout:**
- Key: `(Symbol("mut_rate"), Address)` tuple in Instance storage
- Value: `MutationRateRecord { window_start: u32, count: u32 }`

**Algorithm:**
1. On each mutating call, `check_mutation_limit()` reads the current record.
2. If `window_start` is 0 or `current_seq > window_start + WINDOW`, the window has expired and the counter is implicitly reset (lazy reset).
3. If `count >= MAX_MUTATIONS_PER_WINDOW`, the call is rejected with `MutationLimitExceeded`.
4. If the check passes, `record_mutation()` increments the counter (or starts a new window).

**Where applied:**
- `store_invoice` (rate-limited on the `business` address)
- `cancel_invoice` (rate-limited on the invoice owner)
- `complete_invoice` (rate-limited on the invoice owner)
- `place_bid` (rate-limited on the `investor` address)
- `update_invoice_metadata` (rate-limited on the invoice owner)
- `submit_kyc_application` (rate-limited on the `business` address)
- `submit_investor_kyc` (rate-limited on the `investor` address)

**Not rate-limited:**
- Admin-only operations (admin uses their own address, rate-limited separately)
- Read-only queries
- `get_invoices_by_status_batch` (read-only)

## Lifecycle Invariants

1. **Rejected operations leave no partial state:** Input size checks and rate-limit checks run *before* any storage writes. A rejected call returns an error without modifying storage.
2. **Idempotent operations remain idempotent:** Rate limits do not interfere with nonce-based idempotency. A repeated call with the same nonce returns the cached result regardless of the rate limit.
3. **Stale operations are rejected:** Operations on non-existent or already-terminal invoices return `InvoiceNotFound` or succeed as no-ops, without inflating the rate-limit counter for the wrong address.
4. **Cancelled/completed invoices cannot be double-mutated:** Once an invoice reaches a terminal status (`Paid`, `Cancelled`, `Defaulted`, `Refunded`), subsequent mutations either fail or are idempotent no-ops.
5. **Window expiry resets the counter:** After `RATE_LIMIT_WINDOW_SEQUENCES` ledger sequences pass, the counter resets automatically. This ensures legitimate users recover from throttling without admin intervention.

## Failure Behavior

| Scenario | Error | Recovery |
|----------|-------|----------|
| Description exceeds 4,096 bytes | `InputTooLarge` (2221) | Reduce description size |
| KYC data exceeds 8,192 bytes | `InputTooLarge` (2221) | Reduce KYC payload |
| Tags exceed 50 | `InputTooLarge` (2221) | Reduce tag count |
| Rate limit exceeded | `MutationLimitExceeded` (2220) | Wait for window expiry |
| Invoice not found | `InvoiceNotFound` (1000) | Verify invoice ID |
| Amount out of range | `InvalidAmount` (1200) | Adjust amount |
| Due date in past | `InvoiceDueDateInvalid` (1004) | Use future due date |

## Compatibility / Migration

- **No breaking ABI changes** to existing entrypoints except `get_invoices_by_status_batch` which now returns `Result<Vec<Option<InvoiceStatus>>, QuickLendXError>` instead of `Vec<Option<InvoiceStatus>>`. Callers using the generated Soroban client's `try_` prefix are unaffected.
- **New error variants:** `MutationLimitExceeded` (2220) and `InputTooLarge` (2221) are new. Existing clients that don't match on these codes are unaffected.
- **Rate-limit state is ephemeral per window:** The mutation counter resets automatically after `RATE_LIMIT_WINDOW_SEQUENCES` ledger sequences. No admin intervention or migration is needed.
- **No storage migration required:** Rate-limit records are stored under new keys that don't conflict with existing data.

## Rollback

If the rate limiter causes unexpected behavior in production:
1. The window counter resets automatically after `RATE_LIMIT_WINDOW_SEQUENCES` (20) ledger sequences.
2. Admin can deploy a patched contract without the rate limiter.
3. The input size checks are purely additive and safe to keep.

## Operational Limits

| Parameter | Value | Admin-configurable |
|-----------|-------|-------------------|
| `RATE_LIMIT_WINDOW_SEQUENCES` | 20 | No (compile-time constant) |
| `MAX_MUTATIONS_PER_WINDOW` | 30 | No (compile-time constant) |
| `MAX_INPUT_DESCRIPTION_BYTES` | 4,096 | No (compile-time constant) |
| `MAX_INPUT_KYC_DATA_BYTES` | 8,192 | No (compile-time constant) |
| `MAX_INPUT_TAGS` | 50 | No (compile-time constant) |
| `MAX_INPUT_BATCH_SIZE` | 25 | No (compile-time constant) |
| `MAX_INPUT_STATUS_BATCH_SIZE` | 100 | No (compile-time constant) |
| `MAX_INPUT_LINE_ITEMS` | 50 | No (compile-time constant) |

These are compile-time constants for predictable WASM behavior. Changing them requires a contract upgrade.

## Security Assumptions

1. **Soroban ledger sequence is monotonically increasing:** The rate limiter uses `env.ledger().sequence()` as the time anchor. If the ledger sequence were to go backwards, the window could be extended unexpectedly. This is a fundamental Soroban assumption.
2. **Instance storage is contract-private:** Rate-limit records are stored in the contract's Instance storage, which is inaccessible to external callers. An attacker cannot directly modify the counter.
3. **Transaction atomicity:** Each Soroban transaction is atomic. A rejected rate-limit check reverts all state changes, so there is no window for partial state.
4. **Per-address isolation:** Different addresses have independent rate-limit counters. One address exhausting its budget does not affect others.
5. **Admin operations use the admin's own address:** Admin operations are rate-limited against the admin address, not the target address. This prevents an attacker from exhausting the admin's rate limit via crafted inputs.

# Escrow & Token Transfer Error Handling

## Overview

The escrow module manages the full lifecycle of investor funds: locking them on
bid acceptance, releasing them to the business on settlement, and refunding them
to the investor on cancellation or dispute.

All token movements go through `payments::transfer_funds`, which surfaces
Stellar token failures as typed `QuickLendXError` variants **before** any state
is mutated.

---

## Token Transfer Error Variants

| Error | Code | When raised |
|---|---|---|
| `InvalidAmount` | 1200 | `amount <= 0` passed to `transfer_funds` |
| `InsufficientFunds` | 1400 | Sender's token balance is below `amount` |
| `OperationNotAllowed` | 1402 | Investor's allowance to the contract is below `amount` |
| `TokenTransferFailed` | 2200 | Reserved for future use if the token contract panics |

---

## Escrow Creation (`create_escrow` / `accept_bid`)

### Preconditions checked before any token call

1. `amount > 0` — `InvalidAmount` otherwise.
2. No existing escrow for the invoice — `InvoiceAlreadyFunded` otherwise.
3. Investor balance ≥ `amount` — `InsufficientFunds` otherwise.
4. Investor allowance to contract ≥ `amount` — `OperationNotAllowed` otherwise.

### Atomicity guarantee

The escrow record is written to storage **only after** `token.transfer_from`
returns successfully. If the token call fails, no escrow record is created and
the invoice/bid states are left unchanged. The operation is safe to retry.

### Failure scenarios

| Scenario | Error returned | State after failure |
|---|---|---|
| Investor has zero balance | `InsufficientFunds` | Invoice: `Verified`, Bid: `Placed`, no escrow |
| Investor has zero allowance | `OperationNotAllowed` | Invoice: `Verified`, Bid: `Placed`, no escrow |
| Investor has partial allowance | `OperationNotAllowed` | Invoice: `Verified`, Bid: `Placed`, no escrow |
| Escrow already exists for invoice | `InvoiceAlreadyFunded` | No change |

---

## Escrow Release (`release_escrow`)

Transfers funds from the contract to the business.

### Preconditions

1. Escrow record exists — `StorageKeyNotFound` otherwise.
2. Escrow status is `Held` — `InvalidStatus` otherwise (idempotency guard).
3. Contract balance ≥ escrow amount — `InsufficientFunds` otherwise.

### Atomicity guarantee

The escrow status is updated to `Released` **only after** `token.transfer`
returns successfully. If the transfer fails, the status remains `Held` and the
release can be safely retried.

---

## Escrow Refund (`refund_escrow` / `refund_escrow_funds`)

Transfers funds from the contract back to the investor.

### Preconditions

1. Escrow record exists — `StorageKeyNotFound` otherwise.
2. Escrow status is `Held` — `InvalidStatus` otherwise.
3. Contract balance ≥ escrow amount — `InsufficientFunds` otherwise.

### Atomicity guarantee

The escrow status is updated to `Refunded` **only after** `token.transfer`
returns successfully. If the transfer fails, the status remains `Held` and the
refund can be safely retried.

### Authorization

Only the contract admin or the invoice's business owner may call
`refund_escrow_funds`. Unauthorized callers receive `Unauthorized`.

---

## Security Assumptions

- **No partial transfers.** Balance and allowance are validated before the token
  call. The token contract is never invoked when these checks fail.
- **Idempotency.** Once an escrow transitions to `Released` or `Refunded`, all
  further release/refund attempts return `InvalidStatus` without moving funds.
- **One escrow per invoice.** A second `create_escrow` call for the same invoice
  returns `InvoiceAlreadyFunded` before any token interaction.
- **Reentrancy protection.** All public entry points that touch escrow are
  wrapped with the reentrancy guard in `lib.rs` (`OperationNotAllowed` on
  re-entry).

---

## Query Surfaces

Two read-only contract entry points expose escrow state:

| Function | Returns | Error when missing |
|---|---|---|
| `get_escrow_details(invoice_id)` | Full `Escrow` struct | `StorageKeyNotFound` |
| `get_escrow_status(invoice_id)` | `EscrowStatus` enum | `StorageKeyNotFound` |

### Consistency guarantee

`get_escrow_details(id).status` **must equal** `get_escrow_status(id)` at all
times. Both reads hit the same storage slot; there is no separate status cache.

### Immutable fields

The following fields are written once at creation and **never mutated**:

| Field | Set at |
|---|---|
| `escrow_id` | `create_escrow` |
| `invoice_id` | `create_escrow` |
| `investor` | `create_escrow` |
| `business` | `create_escrow` |
| `amount` | `create_escrow` |
| `currency` | `create_escrow` |
| `created_at` | `create_escrow` |

Only `status` changes across the lifecycle (`Held → Released` or `Held → Refunded`).

### Missing-record error stability

Both `get_escrow_details` and `get_escrow_status` return `StorageKeyNotFound`
when no escrow record exists for the given invoice ID. This is deterministic:

- An invoice ID that was never funded returns `StorageKeyNotFound`.
- A verified invoice that has no associated bid/escrow returns `StorageKeyNotFound`.
- The error is **not** `InvoiceNotFound` even when the invoice itself does not exist.
- Repeated calls return the same error; there is no side-effect on the store.

---

## Tests

Token transfer failure behavior is covered in:

- [`src/test_escrow.rs`](../../src/test_escrow.rs) — creation failures and query
  consistency:
  - `test_accept_bid_fails_when_investor_has_zero_balance`
  - `test_accept_bid_fails_when_investor_has_zero_allowance`
  - `test_accept_bid_fails_when_investor_has_partial_allowance`
  - `test_accept_bid_succeeds_after_topping_up_balance`
  - `test_query_details_status_match_held`
  - `test_query_details_status_match_released`
  - `test_query_details_status_match_refunded`
  - `test_query_immutable_fields_stable_across_release`
  - `test_query_immutable_fields_stable_across_refund`
  - `test_query_missing_record_both_surfaces_return_storage_key_not_found`
  - `test_query_missing_record_error_is_deterministic`
  - `test_query_verified_invoice_without_escrow_returns_storage_key_not_found`
  - `test_query_get_escrow_details_is_deterministic`
  - `test_query_independent_escrows_do_not_cross_contaminate`
- [`src/test_queries.rs`](../../src/test_queries.rs) — escrow query consistency
  (module `escrow_query_consistency`):
  - `test_status_match_held` / `_released` / `_refunded`
  - `test_immutable_fields_stable_held_to_released` / `_held_to_refunded`
  - `test_missing_record_both_surfaces_same_error`
  - `test_verified_invoice_no_escrow_returns_storage_key_not_found`
  - `test_missing_record_error_is_stable_across_repeated_calls`
  - `test_cross_invoice_queries_are_isolated`
- [`src/test_refund.rs`](../../src/test_refund.rs) — refund failures:
  - `test_refund_fails_when_contract_has_insufficient_balance`
  - `test_refund_succeeds_after_balance_restored`

Existing acceptance-hardening tests (state invariants, double-accept, mismatched
invoice/bid pairs) remain in the same files.

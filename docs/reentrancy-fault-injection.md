# Reentrancy fault-injection: hostile token callback suite

## What this document covers
This repository includes a **hostile-token fault injection** test suite intended to
validate the protocol’s payment/escrow reentrancy protection.

## Guard mechanism
QuickLendX protects all funds-moving entrypoints with a payment-path reentrancy
lock implemented in `src/reentrancy.rs` (see `with_payment_guard`).

- On entry, the guard sets an instance-storage boolean lock (e.g. `pay_lock`).
- If a nested call tries to re-enter a guarded entrypoint while the lock is held,
  the call must fail with:
  - `QuickLendXError::OperationNotAllowed`
- On exit (success or failure), the lock is cleared to avoid permanent DoS.

## Hostile token approach
Soroban token transfers can indirectly trigger nested contract execution.
A malicious token contract can exploit this by calling back into QuickLendX during
its `transfer`/`transfer_from` logic.

The tests implement a `HostileToken` that attempts to re-enter **multiple** public
funds-moving entrypoints while the payment guard is already held.

Targeted entrypoints:
- `accept_bid_and_fund`
- `process_partial_payment`
- `settle_invoice`
- `refund_escrow`
- `release_escrow`

## Assertions / expected outcome
For every re-entry attempt:
1. The re-entrant call must fail clearly with `OperationNotAllowed`.
2. The rejection must occur **before any state mutation** (invoice, escrow, payment
   records, or fund balances).
3. The guard lock must be cleared after the rejected nested attempt.

## Severity classification (P0)
If *any* successful re-entrant execution mutates protocol state (e.g., changes an
invoice status, records payments, releases/refunds escrow, or moves balances),
that behavior is a **P0 security finding**.


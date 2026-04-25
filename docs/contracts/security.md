# Contract Security

## Reentrancy Guards

Payment and escrow flows are protected by a **reentrancy guard** so that no nested execution attempt can double-spend funds, replay settlement logic, or corrupt escrow state.

### Mechanism

- A single process-wide lock (`pay_lock`) is stored in contract instance storage.
- Before any payment or escrow transfer runs, the guard checks that the lock is not set.
- If the lock is already set, the call fails with `QuickLendXError::OperationNotAllowed`.
- Otherwise the lock is set, the operation runs, and the lock is cleared on both success and failure.
- **Global Coverage**: The `pay_lock` is shared across all token-moving functions. Re-entry from one guarded function into another is strictly prohibited.

### Guarded Entry Points

The following public functions run inside the guard:

| Function                  | Purpose                                                              |
| ------------------------- | -------------------------------------------------------------------- |
| `accept_bid_and_fund`     | Transfer in: investor → contract (escrow)                            |
| `accept_bid`              | Transfer in: investor → contract (escrow)                            |
| `release_escrow_funds`    | Transfer out: contract → business                                    |
| `refund_escrow_funds`     | Transfer out: contract → investor                                    |
| `process_partial_payment` | Record payment progress and trigger final settlement when fully paid |
| `settle_invoice`          | Transfer out: business → investor (and fee routing)                  |

### Nested Execution Assumptions

- Soroban token transfers do not currently execute recipient fallback hooks the way EVM transfers can.
- The protocol still treats nested execution as a security boundary and rejects any payment-path re-entry while `pay_lock` is held.
- Regression tests model adversarial callback behavior by invoking guarded payment entry points while another guarded payment frame is already active.

### Usage

The guard is applied internally via `reentrancy::with_payment_guard(env, || { ... })`. Callers do not need to do anything; any nested call into the guarded payment surface is rejected before it can mutate escrow, balances, or settlement state.

### Errors

- **OperationNotAllowed** (symbol `OP_NA`, code 1009): Returned when a payment/escrow operation is invoked while another such operation is already in progress (reentrancy guard).

### Regression Coverage

The regression suite in `quicklendx-contracts/src/test_reentrancy.rs` validates:

- direct nested guard acquisition fails and the lock is released afterward
- callback-style nested calls into `accept_bid`, `accept_bid_and_fund`, `release_escrow_funds`, `refund_escrow_funds`, `process_partial_payment`, and `settle_invoice` all fail with `OperationNotAllowed`
- rejected nested calls leave invoice status, escrow status, balances, and payment history unchanged

### Soroban Token and Auth

Guards complement Soroban token transfer and auth patterns: all transfers use the standard token interface, and sensitive actions require the appropriate `require_auth()` so that only authorized roles can trigger payments or escrow changes.

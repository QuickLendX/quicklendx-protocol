# Contract Security

## Reentrancy Guards

Payment and escrow flows are protected by a **reentrancy guard** so that no intermediate re-entry can double-spend or corrupt state.

### Mechanism

- A single process-wide lock (`pay_lock`) is stored in contract instance storage.
- Before any payment or escrow transfer runs, the guard checks that the lock is not set.
- If the lock is already set, the call fails with `QuickLendXError::ReentrancyDetected`.
- Otherwise the lock is set, the operation runs, and the lock is cleared on both success and failure.

### Guarded Entry Points

The following public functions run inside the guard:

| Function | Purpose |
|----------|---------|
| `accept_bid_and_fund` | Transfer in: investor → contract (escrow) |
| `accept_bid` | Transfer in: investor → contract (escrow) |
| `release_escrow_funds` | Transfer out: contract → business |
| `refund_escrow_funds` | Transfer out: contract → investor |
| `settle_invoice` | Transfer out: business → investor (and fee routing) |

### Usage

The guard is applied internally via `reentrancy::with_payment_guard(env, || { ... })`. Callers do not need to do anything; re-entry into any of the above functions (e.g. from a token callback) will be rejected.

### Errors

- **OperationNotAllowed** (symbol `OP_NA`, code 1009): Returned when a payment/escrow operation is invoked while another such operation is already in progress (reentrancy guard).

### Soroban Token and Auth

Guards complement Soroban token transfer and auth patterns: all transfers use the standard token interface, and sensitive actions require the appropriate `require_auth()` so that only authorized roles can trigger payments or escrow changes.

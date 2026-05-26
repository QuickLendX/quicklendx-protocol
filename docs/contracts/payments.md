# Payments Module Documentation

The Payments module in QuickLendX Protocol handles all token transfers and escrow lifecycle operations. It provides the lowest-level financial primitives used by the higher-level escrow and settlement flows.

## Overview

The payments layer sits between the business-logic modules (escrow, settlement) and the Soroban token contracts. Its primary responsibilities are:

1. **Token Transfer Safety** — Validate balances and allowances before invoking any token contract method.
2. **Escrow CRUD** — Create, read, update, and store `Escrow` records atomically with token movements.
3. **Failure Isolation** — Ensure that failed transfers never leave partial state (no orphaned escrows, no half-updated statuses).

## Key Functions

### `transfer_funds`

The primitive token-movement function used by all higher-level operations.

```rust
pub fn transfer_funds(
    env: &Env,
    currency: &Address,
    from: &Address,
    to: &Address,
    amount: i128,
) -> Result<(), QuickLendXError>
```

**Security prechecks (executed in order):**

1. `amount > 0` — returns `InvalidAmount` otherwise.
2. `from == to` — treated as a no-op (`Ok(())`).
3. `token_client.balance(from) >= amount` — returns `InsufficientFunds` otherwise.
4. If `from` is **not** the contract:
   - `token_client.allowance(from, contract) >= amount` — returns `OperationNotAllowed` otherwise.
   - Executes `transfer_from(contract, from, to, amount)`.
5. If `from` **is** the contract:
   - Executes `transfer(from, to, amount)` directly (no allowance required).

> **Atomicity:** All checks run before any token contract call. If a check fails, no token state changes and no storage is written.

### `create_escrow`

Locks investor funds in the contract and creates an `Escrow` record.

```rust
pub fn create_escrow(
    env: &Env,
    invoice_id: &BytesN<32>,
    investor: &Address,
    business: &Address,
    amount: i128,
    currency: &Address,
) -> Result<BytesN<32>, QuickLendXError>
```

**Steps:**
1. Validate `amount > 0`.
2. Check that no escrow already exists for `invoice_id` (one-escrow-per-invoice invariant).
3. Call `transfer_funds` to move tokens from `investor` to the contract.
4. Generate a unique `escrow_id`.
5. Write the `Escrow` record to instance storage.
6. Emit `EscrowCreated` event.

**Errors:** `InvalidAmount`, `InvoiceAlreadyFunded`, `InsufficientFunds`, `OperationNotAllowed`, `TokenTransferFailed`.

### `release_escrow`

Releases held funds from the contract to the business.

```rust
pub fn release_escrow(
    env: &Env,
    invoice_id: &BytesN<32>,
) -> Result<(), QuickLendXError>
```

**Requirements:**
- An escrow record must exist for the invoice.
- Escrow status must be `Held`.
- The contract must hold sufficient token balance.

**Atomicity:** Funds are transferred before the escrow status is updated to `Released`. If the transfer fails, the status remains `Held` and the operation can be retried.

### `refund_escrow`

Returns held funds from the contract back to the investor.

```rust
pub fn refund_escrow(
    env: &Env,
    invoice_id: &BytesN<32>,
) -> Result<(), QuickLendXError>
```

**Requirements:**
- An escrow record must exist for the invoice.
- Escrow status must be `Held`.
- The contract must hold sufficient token balance.

**Atomicity:** Funds are transferred before the escrow status is updated to `Refunded`. If the transfer fails, the status remains `Held` and the operation can be retried.

## Data Structures

### `Escrow`

```rust
pub struct Escrow {
    pub escrow_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub business: Address,
    pub amount: i128,
    pub currency: Address,
    pub created_at: u64,
    pub status: EscrowStatus,
}
```

### `EscrowStatus`

| Variant | Description |
|---------|-------------|
| `Held` | Funds are locked in escrow. |
| `Released` | Funds have been released to the business. |
| `Refunded` | Funds have been refunded to the investor. |

## Error Reference

| Error | Code | When Raised |
|-------|------|-------------|
| `InvalidAmount` | 1200 | `amount` is zero or negative. |
| `InsufficientFunds` | 1400 | Sender token balance is below `amount`. |
| `OperationNotAllowed` | 1402 | Allowance granted to the contract is below `amount` (non-contract sender only). |
| `InvoiceAlreadyFunded` | 1002 | An escrow record already exists for the invoice. |
| `StorageKeyNotFound` | 1301 | No escrow record exists for the given invoice. |
| `InvalidStatus` | 1401 | Escrow is not in `Held` status (release/refund only). |
| `TokenTransferFailed` | 2200 | The underlying Stellar token contract panicked or returned an error. |

## Atomicity and Security Guarantees

1. **No partial escrow creation:** `create_escrow` only writes the `Escrow` record **after** the token transfer succeeds. A failed transfer means no storage is written and no event is emitted.
2. **No partial release/refund:** The escrow status is updated **after** the outbound token transfer succeeds. If the contract lacks funds, the status stays `Held` and the operation remains retryable once funds are restored.
3. **Balance/allowance prechecks:** `transfer_funds` inspects token state before invoking any transfer, preventing the protocol from entering an inconsistent partial-transfer state.
4. **Idempotency:** `release_escrow` and `refund_escrow` require `Held` status; once updated to `Released` or `Refunded`, repeated calls return `InvalidStatus`.

## NatSpec-Style Documentation

All public items in `payments.rs` carry Rust doc comments that serve as NatSpec-style documentation:

- Module-level `//!` docs describe overall purpose and security invariants.
- Function-level `///` docs list parameters, return values, error conditions, and atomicity notes.
- Doc comments use intra-doc links (e.g., `[`QuickLendXError::InsufficientFunds`]`) for cross-referencing.

These comments are compiled into the contract metadata and can be extracted by documentation generators.

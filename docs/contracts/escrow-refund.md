# Escrow Refund Documentation

This document describes the explicit refund path for escrowed funds in the QuickLendX protocol. This mechanism allows for the return of locked funds from the contract's escrow to the investor when an accepted bid needs to be reversed.

## Overview

In the QuickLendX protocol, funds are locked in escrow once a bid is accepted. Under normal circumstances, these funds are released to the business after invoice verification. However, there are scenarios where the transaction must be reversed:
- **Failed Verification**: The admin determines the invoice is invalid or fraudulent after it was already funded.
- **Policy-based Cancellation**: The business or platform policy requires a cancellation post-acceptance (e.g., mutual agreement or protocol-specific triggers).

The refund path ensures that investor capital is not permanently trapped in the contract and can be safely returned.

## Refund vs. Release

| Feature | Release (`release_escrow_funds`) | Refund (`refund_escrow_funds`) |
|---------|---------------------------------|-------------------------------|
| **Trigger** | Verification Success (Admin) | Verification Failure / Cancellation (Admin/Business) |
| **Recipient** | Business Owner | Investor |
| **Invoice Status** | `Funded` → `Paid` (or progressing) | `Funded` → `Refunded` |
| **Bid Status** | `Accepted` (remains) | `Accepted` → `Cancelled` |
| **Investment** | `Active` (remains) | `Active` → `Refunded` |
| **Escrow Status** | `Held` → `Released` | `Held` → `Refunded` |

## Authorization and Rules

### Authorized Actors
- **Platform Admin**: Can trigger a refund at any time while the invoice is in `Funded` status.
- **Business Owner**: Can choose to refund instead of progressing if they wish to reverse the funding (e.g., if they no longer need the advance or if there's a dispute).

### Pre-conditions
1. Invoice must be in `Funded` status.
2. Escrow must be in `Held` status.
3. Bid must be in `Accepted` status.

## Technical Implementation

### Core Function: `refund_escrow_funds`
The function performs the following steps atomically:
1. **Authorization**: Checks if the caller is an Admin or the Business owner.
2. **Status Check**: Validates that the invoice is currently `Funded`.
3. **Escrow Retrieval**: Fetches the associated escrow record.
4. **Token Transfer**: Executes a `transfer` of tokens from the contract's address back to the investor's address.
5. **State Update**: 
   - Marks the invoice as `Refunded`.
   - Marks the accepted bid as `Cancelled`.
   - Marks the investment as `Refunded`.
   - Marks the escrow as `Refunded`.
6. **Logging**: Emits an `esc_ref` event and writes to the audit log.

### Security Notes
- **Reentrancy Protection**: The function is wrapped in a payment guard to prevent reentrancy attacks during token transfers.
- **Atomic State Updates**: All state changes occur within the same transaction, ensuring consistency between invoice, bid, and investment records.
- **Authorization Enforcement**: `caller.require_auth()` ensures that only the intended actor can trigger the refund.

## Events

- **Symbol**: `esc_ref`
- **Data**: `(escrow_id, invoice_id, investor, amount)`

## Audit Logging

Every refund is recorded in the platform's audit trail with:
- **Operation**: `EscrowRefunded`
- **Actor**: The address that initiated the refund.
- **Invoice ID**: The associated invoice.
- **Timestamp**: Ledger timestamp.

---

## Mutual Exclusivity Guarantees (Issue #610)

### Overview

`release_escrow` and `refund_escrow` are **mutually exclusive terminal operations**. Once either succeeds, the escrow enters a terminal state (`Released` or `Refunded`) and no further token movement is possible.

### Terminal State Model

```
Held ──► Released  (terminal — funds sent to business)
Held ──► Refunded  (terminal — funds returned to investor)

Released ──► (any)  REJECTED — InvalidStatus
Refunded ──► (any)  REJECTED — InvalidStatus
```

`EscrowStatus::is_terminal()` encodes this:

```rust
pub fn is_terminal(&self) -> bool {
    matches!(self, EscrowStatus::Released | EscrowStatus::Refunded)
}
```

Both `release_escrow` and `refund_escrow` call `is_terminal()` as their first guard — before any token transfer — so the check is impossible to bypass.

### Blocked Scenarios

| Attempt | Result |
|---------|--------|
| Release after release | `InvalidStatus` |
| Refund after refund | `InvalidStatus` |
| Release after refund | `InvalidStatus` |
| Refund after release | `InvalidStatus` |
| Release/refund with no escrow | `StorageKeyNotFound` |

### Security Properties

1. **No double-spend** — funds can only leave the contract once per escrow record.
2. **No cross-path exploit** — a refund cannot be used to drain funds that were already released, and vice versa.
3. **Reentrancy safe** — both public entry points (`release_escrow_funds`, `refund_escrow_funds`) are wrapped in `with_payment_guard` in `lib.rs`, preventing reentrant calls.
4. **Isolated per invoice** — each invoice has its own escrow record; a terminal event on one invoice has no effect on any other.

### Test Coverage (test_escrow_mutual_exclusivity.rs)

| Test | Scenario |
|------|----------|
| `test_is_terminal_held_is_false` | `Held` is not terminal |
| `test_is_terminal_released_is_true` | `Released` is terminal |
| `test_is_terminal_refunded_is_true` | `Refunded` is terminal |
| `test_escrow_held_after_funding` | Escrow is `Held` immediately after `accept_bid` |
| `test_release_sets_status_released` | Settlement transitions escrow to `Released` |
| `test_double_release_rejected` | Second release fails with `InvalidStatus` |
| `test_refund_after_release_rejected` | Refund after release fails |
| `test_release_balance_accounting` | Business balance correct after release |
| `test_refund_sets_status_refunded` | Refund transitions escrow to `Refunded` |
| `test_double_refund_rejected` | Second refund fails with `InvalidStatus` |
| `test_release_after_refund_rejected` | Release after refund fails |
| `test_refund_balance_accounting` | Investor balance restored after refund |
| `test_release_no_escrow_returns_error` | Release on unfunded invoice returns error |
| `test_refund_no_escrow_returns_error` | Refund on unfunded invoice returns error |
| `test_independent_escrows_isolated` | Two invoices have isolated escrow state |
| `test_contract_balance_integrity` | Contract balance decreases after terminal event |

# Escrow Module Documentation

The Escrow module in QuickLendX Protocol provides a secure mechanism for holding investor funds when a bid is accepted until the invoice is verified. This ensures that businesses only receive funds after the invoice has been validated, protecting investors from fraud.

## Overview

When a business accepts a bid from an investor, the bid amount is locked in a contract-controlled escrow account. The invoice status is updated to "Funded". The funds remain in escrow until one of the following occurs:
1. **Verification**: The invoice is verified by an admin, triggering the release of funds to the business.
2. **Refund**: If the invoice verification fails or other issues arise, funds can be refunded to the investor.
3. **Dispute**: In case of disputes, funds are held until resolution.

## Workflow

1.  **Bid Acceptance**:
    *   Business owner calls `accept_bid_and_fund`.
    *   System validates invoice status (must be Verified) and bid status (must be Placed).
    *   Funds are transferred from the investor's wallet to the contract's escrow account.
    *   An `Escrow` record is created.
    *   Invoice status changes to `Funded`.
    *   Bid status changes to `Accepted`.
    *   Investment record is created.

2.  **Fund Release**:
    *   Admin calls `verify_invoice` (or manual release via `release_escrow_funds`).
    *   Funds are transferred from the contract to the business wallet.
    *   Escrow status changes to `Released`.

3.  **Refund**:
    *   Admin or the Business owner calls `refund_escrow_funds`.
    *   System validates invoice status (must be Funded).
    *   Funds are transferred back from the contract to the investor.
    *   Escrow status changes to `Refunded`.
    *   Invoice status changes to `Refunded`.
    *   Bid status changes to `Cancelled`.
    *   Investment status changes to `Refunded`.

## Key Functions

### `accept_bid_and_fund`
*   **Description**: Accepts a bid and locks funds in escrow.
*   **Parameters**: `invoice_id`, `bid_id`.
*   **Auth**: Requires business owner authorization.
*   **Events**: `EscrowCreated`, `InvoiceFunded`.

### `release_escrow_funds`
*   **Description**: Releases funds from escrow to the business.
*   **Parameters**: `invoice_id`.
*   **Auth**: Internal/Admin.
*   **Events**: `EscrowReleased`.

### `refund_escrow_funds`
*   **Description**: Refunds escrow funds back to the investor.
*   **Parameters**: `invoice_id`, `caller`.
*   **Auth**: Admin or Business Owner.
*   **Events**: `esc_ref` (EscrowRefunded), Audit logs.

### `get_escrow_details`
*   **Description**: Retrieves details of the escrow for a given invoice.
*   **Parameters**: `invoice_id`.
*   **Returns**: `Escrow` struct.

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
*   `Held`: Funds are locked in escrow.
*   `Released`: Funds have been released to the business.
*   `Refunded`: Funds have been returned to the investor.

## Invariants

The protocol enforces several critical invariants to ensure security and consistency of the escrow lifecycle:

1.  **Single Active Escrow**: Each invoice can have at most one active escrow record (Held status).
2.  **Creation Guard**: Escrow can only be created if the invoice is in `Verified` status and no existing escrow is found for the invoice ID.
3.  **Duplicate Rejection**: Any attempt to create a second escrow for an invoice that already has one will be rejected with the `InvoiceAlreadyFunded` error.
4.  **Release/Refund Mutex**: An escrow can be either released or refunded, but never both. The final status `Released` or `Refunded` is terminal.
5.  **Held-Only Transitions**: `release_escrow` and `refund_escrow` only operate on escrows in the `Held` state. Any attempt to call them on a `Released` or `Refunded` escrow is rejected with `InvalidStatus`.
6.  **No Double-Execution**: Once an escrow reaches a terminal state (`Released` or `Refunded`), no further fund movement is possible. Double-release and double-refund are both rejected with `InvalidStatus`.
7.  **No Double-Spend**: Funds are transferred exactly once per escrow. Storage is updated **after** the token transfer succeeds, so a failed transfer leaves the escrow in `Held` and the operation is safely retryable.
8.  **Non-Existent Escrow Guard**: Calling `release_escrow` or `refund_escrow` on an invoice with no escrow record returns `StorageKeyNotFound`.
9.  **Amount Validation**: `create_escrow` rejects zero or negative amounts with `InvalidAmount` before any token transfer is attempted.
10. **Escrow Isolation**: Independent escrows for different invoices do not interfere with each other.

### State Diagram

```
                    ┌─────────────────────────────────────┐
                    │           create_escrow              │
                    │  (only if no escrow exists,          │
                    │   amount > 0, invoice Verified)      │
                    └──────────────┬──────────────────────┘
                                   │
                                   ▼
                              ┌─────────┐
                              │  Held   │  ◄── only valid source state
                              └────┬────┘
                    ┌──────────────┴──────────────┐
                    │                             │
              release_escrow               refund_escrow
                    │                             │
                    ▼                             ▼
              ┌──────────┐                 ┌──────────┐
              │ Released │                 │ Refunded │
              │ (terminal│                 │ (terminal│
              │  state)  │                 │  state)  │
              └──────────┘                 └──────────┘
```

### Test Coverage

All invariants above are codified in `src/test_escrow_state_machine.rs` (issue #808):

| Test | Invariant |
|------|-----------|
| `invariant_release_from_held_succeeds` | Held → Released transition |
| `invariant_refund_from_held_succeeds` | Held → Refunded transition |
| `invariant_double_release_rejected` | Terminal state is final |
| `invariant_double_refund_rejected` | Terminal state is final |
| `invariant_refund_after_release_rejected` | Release/Refund mutex |
| `invariant_release_after_refund_rejected` | Release/Refund mutex |
| `invariant_release_nonexistent_escrow_rejected` | Non-existent escrow guard |
| `invariant_refund_nonexistent_escrow_rejected` | Non-existent escrow guard |
| `invariant_duplicate_create_escrow_rejected` | No double-funding |
| `invariant_create_escrow_zero_amount_rejected` | Amount validation |
| `invariant_create_escrow_negative_amount_rejected` | Amount validation |
| `invariant_release_transfers_funds_to_business` | No double-spend (balance check) |
| `invariant_refund_returns_funds_to_investor` | No double-spend (balance check) |
| `invariant_escrow_record_fields_correct_after_creation` | Record integrity |
| `invariant_independent_escrows_do_not_interfere` | Escrow isolation |
| `invariant_escrow_lookup_by_id_and_invoice_consistent` | Storage consistency |

## Key Functions

### `accept_bid_and_fund`
*   **Description**: Accepts a bid and locks funds in escrow.
*   **Invariant Enforcement**: Directly checks if an escrow already exists for the `invoice_id` before transferring funds.
*   **Parameters**: `invoice_id`, `bid_id`.
*   **Auth**: Requires business owner authorization.
*   **Events**: `esc_cr` (EscrowCreated), `inv_fnd` (InvoiceFunded).

### `release_escrow_funds`
*   **Description**: Releases funds from escrow to the business.
*   **Parameters**: `invoice_id`.
*   **Auth**: Internal/Admin.
*   **Events**: `esc_rel` (EscrowReleased).

### `refund_escrow_funds`
*   **Description**: Refunds escrow funds back to the investor.
*   **Parameters**: `invoice_id`, `caller`.
*   **Auth**: Admin or Business Owner.
*   **Events**: `esc_ref` (EscrowRefunded), Audit logs.

### `get_escrow_details`
*   **Description**: Retrieves details of the escrow for a given invoice.
*   **Parameters**: `invoice_id`.
*   **Returns**: `Escrow` struct.

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
*   `Held`: Funds are locked in escrow.
*   `Released`: Funds have been released to the business.
*   `Refunded`: Funds have been returned to the investor.

## Security Considerations

*   **Authorization**: Only the business owner can accept a bid. Only admins can verify invoices (or trigger release when invoice is funded). Only Admins and Business Owners can trigger a refund of funded invoices. Wrong-caller attempts return `Unauthorized`.
*   **Reentrancy**: All payment flows (`accept_bid`, `release_escrow_funds`, `refund_escrow_funds`, `settle_invoice`) are protected by a payment reentrancy guard so that token callbacks cannot re-enter and double-release or double-refund.
*   **Token integration**: Uses the Stellar token contract interface (`transfer`, `transfer_from`) for moving funds. Balances and allowances are checked before transfers; insufficient funds or allowance return `InsufficientFunds` or `OperationNotAllowed`.

## Events

*   `inv_fnd`: Invoice funded (contains invoice ID, investor, amount).
*   `esc_cr`: Escrow created.
*   `esc_rel`: Escrow released.
*   `esc_ref`: Escrow refunded.

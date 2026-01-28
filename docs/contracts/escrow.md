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
    *   Admin or authorized process calls `refund_escrow_funds`.
    *   Funds are transferred back to the investor.
    *   Escrow status changes to `Refunded`.

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
*   **Description**: Refunds escrow funds to the investor.
*   **Parameters**: `invoice_id`.
*   **Auth**: Internal/Admin.
*   **Events**: `EscrowRefunded`.

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

*   **Authorization**: Only the business owner can accept a bid. Only admins can verify invoices and trigger release (in the current flow).
*   **Invariants**:
    *   Escrow can only be created if the invoice is `Verified` (or ready for funding) and the bid is `Placed`.
    *   Funds can only be released if the escrow is in `Held` status.
    *   Double-spending prevention: Bids are marked `Accepted` immediately.
*   **Token Safety**: Uses Soroban token interface for secure transfers. Checks balances and allowances (though allowance is handled by `transfer_from`).

## Events

*   `inv_fnd`: Invoice funded (contains invoice ID, investor, amount).
*   `esc_cr`: Escrow created.
*   `esc_rel`: Escrow released.
*   `esc_ref`: Escrow refunded.

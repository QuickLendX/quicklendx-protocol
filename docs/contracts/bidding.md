# Bidding System Documentation

## Overview

The bidding system enables verified investors to place and withdraw bids on verified invoices. This document describes the entrypoints, validation rules, events, and security considerations.

## Entrypoints


### `place_bid`

Places a bid on a verified invoice.

**Signature:**
```rust
pub fn place_bid(
    env: Env,
    investor: Address,
    invoice_id: BytesN<32>,
    bid_amount: i128,
    expected_return: i128,
) -> Result<BytesN<32>, QuickLendXError>
```

**Parameters:**
- `investor`: Address of the investor placing the bid (must be authenticated)
- `invoice_id`: Unique identifier of the invoice
- `bid_amount`: Amount the investor is willing to fund (must be positive)
- `expected_return`: Expected return amount (must be greater than bid_amount)

**Returns:**
- `Ok(BytesN<32>)`: The unique bid ID on success
- `Err(QuickLendXError)`: Error code on failure

**Validation Rules:**
1. Invoice must exist and be in `Verified` status
2. Investor must be authenticated (require_auth)
3. Investor must be verified with valid KYC status
4. Bid amount must be positive and meet minimum threshold (100)
5. Bid amount cannot exceed invoice amount
6. Expected return must be greater than bid amount
7. Bid amount cannot exceed investor's investment limit
8. Investor cannot have an existing active bid on the same invoice
9. Expired bids are automatically cleaned up before validation

**Events Emitted:**
- `bid_plc`: Bid placed event with bid details

**Error Codes:**
- `InvoiceNotFound`: Invoice does not exist
- `InvalidStatus`: Invoice is not verified
- `BusinessNotVerified`: Investor is not verified
- `InvalidAmount`: Bid amount or expected return is invalid
- `InvoiceAmountInvalid`: Bid amount exceeds invoice amount
- `OperationNotAllowed`: Investor already has an active bid on this invoice

**Example:**
```rust
let bid_id = contract.place_bid(
    &env,
    investor_address,
    invoice_id,
    10000,  // bid_amount
    11000,  // expected_return
)?;
```

### `withdraw_bid`

Withdraws a previously placed bid before it is accepted.

**Signature:**
```rust
pub fn withdraw_bid(env: Env, bid_id: BytesN<32>) -> Result<(), QuickLendXError>
```

**Parameters:**
- `bid_id`: Unique identifier of the bid to withdraw

**Returns:**
- `Ok(())`: Success
- `Err(QuickLendXError)`: Error code on failure

**Validation Rules:**
1. Bid must exist
2. Only the investor who placed the bid can withdraw it (require_auth)
3. Bid must be in `Placed` status (cannot withdraw accepted, withdrawn, or expired bids)

**Events Emitted:**
- `bid_wdr`: Bid withdrawn event with bid details

**Error Codes:**
- `StorageKeyNotFound`: Bid does not exist
- `OperationNotAllowed`: Bid is not in Placed status

**Example:**
```rust
contract.withdraw_bid(&env, bid_id)?;
```

### `get_bids_for_invoice`

Retrieves all bids for a specific invoice, including expired and withdrawn bids.

**Signature:**
```rust
pub fn get_bids_for_invoice(env: Env, invoice_id: BytesN<32>) -> Vec<Bid>
```

**Parameters:**
- `invoice_id`: Unique identifier of the invoice

**Returns:**
- `Vec<Bid>`: Vector of all bid records for the invoice

**Notes:**
- Automatically refreshes expired bids before returning results
- Returns bids in all statuses (Placed, Withdrawn, Accepted, Expired)
- Use `get_bids_by_status` to filter by status if needed

**Example:**
```rust
let bids = contract.get_bids_for_invoice(&env, invoice_id);
for bid in bids.iter() {
    // Process bid
}
```

## Query Helpers

### `get_bid`

Retrieves a single bid by ID.

```rust
pub fn get_bid(env: Env, bid_id: BytesN<32>) -> Option<Bid>
```

### `get_best_bid`

Gets the highest ranked bid for an invoice (based on profit, return, and timestamp).

```rust
pub fn get_best_bid(env: Env, invoice_id: BytesN<32>) -> Option<Bid>
```

### `get_ranked_bids`

Gets all bids for an invoice sorted by platform ranking rules.

```rust
pub fn get_ranked_bids(env: Env, invoice_id: BytesN<32>) -> Vec<Bid>
```

### `get_bids_by_status`

Filters bids by status (Placed, Withdrawn, Accepted, Expired).

```rust
pub fn get_bids_by_status(env: Env, invoice_id: BytesN<32>, status: BidStatus) -> Vec<Bid>
```

### `get_bids_by_investor`

Gets all bids from a specific investor for an invoice.

```rust
pub fn get_bids_by_investor(env: Env, invoice_id: BytesN<32>, investor: Address) -> Vec<Bid>
```

### `cancel_bid`

Cancels a placed bid. Unlike withdraw, cancel is a hard termination.

**Signature:**
```rust
pub fn cancel_bid(env: Env, bid_id: BytesN<32>) -> bool
```

**Returns:** `true` if cancelled, `false` if bid not found or not in `Placed` status.

**Validation Rules:**
1. Bid must exist
2. Bid must be in `Placed` status

**Error Codes:** None — returns `false` for invalid states instead of error.

---

### `get_all_bids_by_investor`

Returns all bids placed by an investor across **all invoices** (all statuses).

**Signature:**
```rust
pub fn get_all_bids_by_investor(env: Env, investor: Address) -> Vec<Bid>
```

**Returns:** All bid records for the investor regardless of status or invoice.

**Notes:**
- Use `get_bids_by_investor` for per-invoice filtering
- Includes Placed, Withdrawn, Cancelled, Accepted, Expired bids

## Data Structures

### `Bid`

```rust
    pub struct Bid {
    pub bid_id: BytesN<32>,           // Unique bid identifier
    pub invoice_id: BytesN<32>,       // Associated invoice
    pub investor: Address,            // Investor who placed the bid
    pub bid_amount: i128,              // Amount being bid
    pub expected_return: i128,        // Expected return amount
    pub timestamp: u64,               // When bid was placed
    pub status: BidStatus,            // Current bid status
    pub expiration_timestamp: u64,    // When bid expires (default: 7 days, admin-configurable 1–30 days)
}
```

### `BidStatus`

```rust
pub enum BidStatus {
    Placed,    // Active bid awaiting acceptance
    Withdrawn, // Bid was withdrawn by investor
    Accepted,  // Bid was accepted by business
    Expired,   // Bid expired without acceptance
    Cancelled, // Bid was hard-cancelled
}
```

## Bid Lifecycle

1. **Place Bid**: Investor places a bid on a verified invoice
    - Status: `Placed`
    - Expiration: Default is 7 days from placement. This TTL is admin-configurable in days (1–30) without a code change.

### Bid TTL Configuration (Admin)

- `set_bid_ttl_days(env, days: u64) -> Result<u64, QuickLendXError>`: Admin-only entrypoint to set the default bid TTL in days. Must be between 1 and 30. The stored value is used for subsequent bids.
- `get_bid_ttl_days(env) -> u64`: Read-only entrypoint returning the configured TTL in days (returns 7 if not set).

Security: only the configured protocol admin may call `set_bid_ttl_days`. Calls require the admin to authorize the transaction.

2. **Withdraw Bid**: Investor withdraws their bid before acceptance
   - Status: `Withdrawn`
   - Only possible if status is `Placed`
  
3. **Cancel Bid**: Hard cancellation of a placed bid
   - Status: `Cancelled`
   - Returns `false` if already non-Placed
   - Excluded from ranking and best-bid selection

4. **Accept Bid**: Business accepts a bid (via `accept_bid` entrypoint)
   - Status: `Accepted`
   - Invoice status changes to `Funded`
   - Escrow is created

5. **Expire Bid**: Bid expires after expiration timestamp
   - Status: `Expired`
   - Automatically updated during cleanup operations

## Security Considerations

### Access Control
- Only authenticated investors can place bids
- Only the bid owner can withdraw their bid
- Investor verification is required before placing bids

### Validation
- Invoice must be verified before accepting bids
- Bid amounts are validated against invoice amount and investor limits
- Duplicate active bids from the same investor are prevented
- Expired bids are automatically cleaned up

### Status Enforcement
- Bids can only be withdrawn if in `Placed` status
- Bids cannot be placed on non-verified invoices
- Bid status transitions are strictly enforced

### Investment Limits
- Bid amounts are validated against investor's investment limit
- Risk-based restrictions apply for high-risk investors
- Investment limits are calculated based on investor tier and risk level

## Events

### `bid_plc` (Bid Placed)
Emitted when a bid is successfully placed.

**Event Data:**
- `bid_id`: BytesN<32>
- `invoice_id`: BytesN<32>
- `investor`: Address
- `bid_amount`: i128
- `expected_return`: i128
- `timestamp`: u64
- `expiration_timestamp`: u64

### `bid_wdr` (Bid Withdrawn)
Emitted when a bid is withdrawn.

**Event Data:**
- `bid_id`: BytesN<32>
- `invoice_id`: BytesN<32>
- `investor`: Address
- `bid_amount`: i128
- `withdrawn_at`: u64

### `bid_exp` (Bid Expired)
Emitted when a bid expires.

**Event Data:**
- `bid_id`: BytesN<32>
- `invoice_id`: BytesN<32>
- `investor`: Address
- `bid_amount`: i128
- `expiration_timestamp`: u64

## Error Handling

All entrypoints return `Result<T, QuickLendXError>` for proper error handling. Common errors include:

- `InvoiceNotFound`: Invoice does not exist
- `InvalidStatus`: Invalid invoice or bid status
- `BusinessNotVerified`: Investor verification required
- `InvalidAmount`: Invalid bid amount or expected return
- `StorageKeyNotFound`: Bid does not exist
- `OperationNotAllowed`: Operation not allowed in current state

## Best Practices

1. **Check Invoice Status**: Always verify invoice is in `Verified` status before placing bids
2. **Validate Amounts**: Ensure bid amounts are within investor limits and invoice constraints
3. **Handle Expiration**: Check bid expiration before accepting bids
4. **Event Monitoring**: Monitor events for bid lifecycle tracking
5. **Error Handling**: Always handle Result types and provide user feedback

## Testing

The bidding system should be tested for:
- Successful bid placement on verified invoices
- Bid withdrawal by authorized investor
- Rejection of bids on non-verified invoices
- Rejection of duplicate bids from same investor
- Automatic expiration handling
- Investment limit enforcement
- Status transition validation

## Related Entrypoints

- `accept_bid`: Business accepts a bid (changes status to Accepted)
- `get_best_bid`: Get the highest ranked bid
- `get_ranked_bids`: Get all bids sorted by ranking
- `cleanup_expired_bids`: Manually trigger expired bid cleanup

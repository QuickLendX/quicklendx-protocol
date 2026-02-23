# Escrow Module Documentation

The Escrow module in QuickLendX Protocol provides a secure mechanism for holding investor funds when a bid is accepted until the invoice is settled. This ensures that businesses receive funds only after proper validation, while protecting investors through atomic transactions and reentrancy guards.

## Overview

When a business accepts a bid from an investor, the bid amount is locked in a contract-controlled escrow account. The invoice status is updated to "Funded". The funds remain in escrow until one of the following occurs:
1. **Release**: The invoice is paid/settled, triggering the release of funds to the business
2. **Refund**: If issues arise, funds can be refunded to the investor
3. **Dispute**: In case of disputes, funds are held until resolution

## Workflow

### 1. Bid Acceptance (accept_bid_and_fund)

**Prerequisites:**
- Invoice must be in `Verified` status
- Bid must be in `Placed` status
- Bid must not be expired
- Investor must have sufficient token balance
- Investor must have approved contract to spend tokens
- Caller must be the business owner of the invoice

**Process:**
1. Business owner calls `accept_bid_and_fund(invoice_id, bid_id)`
2. System validates:
   - Caller is invoice owner (via `require_auth()`)
   - Invoice status is `Verified`
   - Bid status is `Placed`
   - Bid matches invoice (bid.invoice_id == invoice_id)
   - Bid is not expired
   - Bid amount > 0
3. Funds are transferred from investor to contract (via `transfer_from`)
4. Escrow record is created with status `Held`
5. Bid status changes to `Accepted`
6. Invoice status changes to `Funded`
7. Investment record is created with status `Active`
8. Events emitted: `EscrowCreated`, `InvoiceFunded`

**Atomicity:**
All state changes occur atomically. If any step fails, the entire transaction reverts with no partial state changes.

### 2. Fund Release (release_escrow_funds)

**Prerequisites:**
- Invoice must be in `Funded` status
- Escrow must be in `Held` status

**Process:**
1. Authorized party calls `release_escrow_funds(invoice_id)`
2. Funds are transferred from contract to business wallet
3. Escrow status changes to `Released`
4. Event emitted: `EscrowReleased`

### 3. Refund (refund_escrow_funds)

**Prerequisites:**
- Invoice must be in `Funded` status
- Escrow must be in `Held` status
- Caller must be Admin or Business owner

**Process:**
1. Admin or Business owner calls `refund_escrow_funds(invoice_id, caller)`
2. System validates caller authorization
3. Funds are transferred from contract back to investor
4. Escrow status changes to `Refunded`
5. Invoice status changes to `Refunded`
6. Bid status changes to `Cancelled`
7. Investment status changes to `Refunded`
8. Event emitted: `EscrowRefunded`

## Key Functions

### `accept_bid_and_fund`

**Signature:**
```rust
pub fn accept_bid_and_fund(
    env: &Env,
    invoice_id: &BytesN<32>,
    bid_id: &BytesN<32>,
) -> Result<BytesN<32>, QuickLendXError>
```

**Parameters:**
- `invoice_id`: Unique identifier for the invoice
- `bid_id`: Unique identifier for the bid to accept

**Returns:**
- `Ok(escrow_id)`: The newly created escrow identifier
- `Err(QuickLendXError)`: Specific error indicating failure reason

**Authorization:** Requires business owner authorization via `invoice.business.require_auth()`

**Events:** 
- `EscrowCreated` (via `create_escrow`)
- `InvoiceFunded`

**Errors:**
- `InvoiceNotFound`: Invoice ID doesn't exist
- `StorageKeyNotFound`: Bid ID doesn't exist
- `Unauthorized`: Caller is not invoice owner OR bid doesn't match invoice
- `InvalidStatus`: Bid not Placed, bid expired, or invoice not Verified
- `InvoiceAlreadyFunded`: Invoice already in Funded status
- `InvoiceNotAvailableForFunding`: Invoice not in Verified status
- `InvalidAmount`: Bid amount <= 0
- `InsufficientFunds`: Investor lacks token balance
- `OperationNotAllowed`: Insufficient allowance or reentrancy detected

### `release_escrow_funds`

**Signature:**
```rust
pub fn release_escrow_funds(
    env: &Env,
    invoice_id: &BytesN<32>,
) -> Result<(), QuickLendXError>
```

**Description**: Releases funds from escrow to the business.

**Authorization**: Internal/Admin controlled

**Events**: `EscrowReleased`

**Errors:**
- `StorageKeyNotFound`: No escrow found for invoice
- `InvalidStatus`: Escrow not in Held status

### `refund_escrow_funds`

**Signature:**
```rust
pub fn refund_escrow_funds(
    env: &Env,
    invoice_id: &BytesN<32>,
    caller: &Address,
) -> Result<(), QuickLendXError>
```

**Description**: Refunds escrow funds back to the investor.

**Authorization**: Admin or Business Owner (via `caller.require_auth()`)

**Events**: `EscrowRefunded`, Audit logs

**Errors:**
- `InvoiceNotFound`: Invoice doesn't exist
- `StorageKeyNotFound`: Escrow doesn't exist
- `InvalidStatus`: Invoice not in Funded status or escrow not Held
- `Unauthorized`: Caller is neither admin nor business owner

### `get_escrow_details`

**Description**: Retrieves details of the escrow for a given invoice.

**Parameters**: `invoice_id`

**Returns**: `Escrow` struct

## Data Structures

### `Escrow`
```rust
pub struct Escrow {
    pub escrow_id: BytesN<32>,      // Unique escrow identifier
    pub invoice_id: BytesN<32>,     // Associated invoice
    pub investor: Address,           // Investor who provided funds
    pub business: Address,           // Business receiving funds
    pub amount: i128,                // Amount held in escrow
    pub currency: Address,           // Token contract address
    pub created_at: u64,             // Timestamp of creation
    pub status: EscrowStatus,        // Current status
}
```

### `EscrowStatus`
```rust
pub enum EscrowStatus {
    Held,      // Funds are locked in escrow
    Released,  // Funds have been released to business
    Refunded,  // Funds have been returned to investor
}
```

### `Investment`
```rust
pub struct Investment {
    pub investment_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub amount: i128,
    pub funded_at: u64,
    pub status: InvestmentStatus,
    pub insurance: Vec<InsuranceCoverage>,
}
```

## Security Considerations

### Reentrancy Protection

The `accept_bid_and_fund` function is wrapped with a reentrancy guard in the public API:

```rust
pub fn accept_bid(env: Env, invoice_id: BytesN<32>, bid_id: BytesN<32>) 
    -> Result<BytesN<32>, QuickLendXError> 
{
    reentrancy::with_payment_guard(&env, || {
        do_accept_bid_and_fund(&env, &invoice_id, &bid_id)
    })
}
```

The guard prevents recursive calls during execution, protecting against:
- Double-spending attacks
- State corruption from reentrant calls
- Token transfer manipulation

If a reentrancy attempt is detected, the function returns `OperationNotAllowed` error.

### Authorization Model

**Business Authorization:**
- Only the invoice owner can accept bids
- Enforced via `invoice.business.require_auth()`
- No admin override for bid acceptance

**Investor Authorization:**
- Token transfer uses allowance mechanism
- Investor must pre-approve contract to spend tokens
- Balance and allowance checked before transfer

**Refund Authorization:**
- Admin OR business owner can trigger refunds
- Provides flexibility for dispute resolution
- Both parties must explicitly authorize via `require_auth()`

### Token Transfer Security

**Check-Effects-Interactions Pattern:**
1. All validations performed first (checks)
2. State updates applied (effects)
3. External token transfer last (interactions)

This ordering prevents:
- Reentrancy vulnerabilities
- State inconsistencies
- Failed transfers leaving partial state

**Transfer Validation:**
- Balance verification before transfer
- Allowance verification for `transfer_from`
- Amount validation (must be > 0)
- Atomic execution via Soroban transaction model

### State Consistency

**Unique Mappings:**
- One escrow per invoice (invoice_id → escrow_id)
- One investment per invoice (invoice_id → investment_id)
- Prevents duplicate escrows/investments

**One-Way Transitions:**
- Bid: Placed → Accepted (irreversible)
- Invoice: Verified → Funded (irreversible without refund)
- Escrow: Held → Released/Refunded (irreversible)

**Index Consistency:**
- Bid indices updated atomically with bid records
- Investment indices updated atomically with investment records
- Escrow lookup by invoice_id always consistent

### Validation Order

Validations are ordered to fail fast and minimize gas costs:

1. **Entity existence** (invoice, bid) - cheapest check
2. **Authorization** (business ownership) - auth check
3. **Status checks** (invoice Verified, bid Placed) - state reads
4. **Business logic** (bid matches invoice, not expired) - comparisons
5. **Financial checks** (amount > 0, sufficient balance) - token queries
6. **State mutations** (escrow creation, status updates) - most expensive

### Audit Trail

All funding operations create audit log entries:

```rust
invoice.mark_as_funded(env, investor, amount, timestamp);
```

This provides:
- Immutable record of funding events
- Timestamp tracking
- Amount verification
- Investor identification

## Events

### `inv_fnd` (InvoiceFunded)
Emitted when a bid is accepted and invoice is funded.

**Data:**
- `invoice_id`: Invoice identifier
- `investor`: Investor address
- `amount`: Funded amount

### `esc_cr` (EscrowCreated)
Emitted when escrow is created.

**Data:**
- `escrow_id`: Escrow identifier
- `invoice_id`: Associated invoice
- `investor`: Investor address
- `business`: Business address
- `amount`: Escrow amount
- `currency`: Token address

### `esc_rel` (EscrowReleased)
Emitted when escrow funds are released to business.

**Data:**
- `escrow_id`: Escrow identifier
- `invoice_id`: Associated invoice
- `amount`: Released amount

### `esc_ref` (EscrowRefunded)
Emitted when escrow funds are refunded to investor.

**Data:**
- `escrow_id`: Escrow identifier
- `invoice_id`: Associated invoice
- `investor`: Investor address
- `amount`: Refunded amount

## Error Scenarios

| Error | Cause | Resolution |
|-------|-------|------------|
| `InvoiceNotFound` | Invalid invoice ID | Verify invoice exists |
| `StorageKeyNotFound` | Invalid bid ID | Verify bid exists |
| `Unauthorized` | Not invoice owner or bid mismatch | Use correct business account |
| `InvalidStatus` | Wrong invoice/bid status | Check current status |
| `InvoiceAlreadyFunded` | Invoice already funded | Cannot accept multiple bids |
| `InvoiceNotAvailableForFunding` | Invoice not verified | Wait for admin verification |
| `InvalidAmount` | Bid amount <= 0 | Place bid with positive amount |
| `InsufficientFunds` | Investor lacks tokens | Add funds to investor account |
| `OperationNotAllowed` | Reentrancy or no allowance | Check allowance, avoid reentrancy |

## Example Usage

### Complete Funding Flow

```rust
// 1. Business uploads invoice
let invoice_id = client.store_invoice(
    &business,
    &10_000,
    &currency,
    &due_date,
    &description,
    &category,
    &tags
);

// 2. Admin verifies invoice
client.verify_invoice(&invoice_id);

// 3. Investor approves contract to spend tokens
token_client.approve(
    &investor,
    &contract_address,
    &10_000,
    &expiration
);

// 4. Investor places bid
let bid_id = client.place_bid(
    &investor,
    &invoice_id,
    &10_000,
    &11_000  // expected return
);

// 5. Business accepts bid (creates escrow)
let escrow_id = client.accept_bid(&invoice_id, &bid_id);

// 6. Verify escrow created
let escrow = client.get_escrow_details(&invoice_id);
assert_eq!(escrow.status, EscrowStatus::Held);
assert_eq!(escrow.amount, 10_000);

// 7. Later: Release funds to business
client.release_escrow_funds(&invoice_id);

// OR: Refund to investor if needed
client.refund_escrow_funds(&invoice_id, &caller);
```

### Integration Guide

**Frontend Integration:**

1. **Check Prerequisites:**
   - Verify invoice is in Verified status
   - Verify bid is in Placed status
   - Check investor token balance
   - Check investor allowance

2. **Prepare Transaction:**
   - Get business signature for authorization
   - Prepare `accept_bid` call with invoice_id and bid_id

3. **Handle Response:**
   - Success: Display escrow_id, update UI to show Funded status
   - Error: Display user-friendly error message based on error code

4. **Monitor Events:**
   - Listen for `EscrowCreated` event
   - Listen for `InvoiceFunded` event
   - Update UI when events received

**Backend Integration:**

1. **Validation:**
   - Verify all prerequisites before calling contract
   - Check token balances and allowances
   - Validate bid hasn't expired

2. **Transaction Submission:**
   - Use proper authorization (business signature)
   - Handle transaction failures gracefully
   - Implement retry logic for network issues

3. **Event Monitoring:**
   - Subscribe to contract events
   - Update database when escrow created
   - Trigger notifications to relevant parties

4. **Error Handling:**
   - Map contract errors to user-friendly messages
   - Log errors for debugging
   - Implement fallback mechanisms

## Performance Considerations

**Gas Optimization:**
- Early validation fails fast before expensive operations
- Minimal storage reads (cache invoice and bid after retrieval)
- Efficient ID generation (timestamp + counter)
- Batch updates in single transaction

**Storage Efficiency:**
- Symbol-based keys for efficient lookups
- No redundant data (reference by ID)
- Compact enum representation for status fields

**Scalability:**
- Per-invoice isolation (operations don't interfere)
- No global locks (reentrancy guard is per-transaction)
- No unbounded loops in critical path

## Testing

The escrow module has comprehensive test coverage including:

- **Authorization tests**: Only invoice owner can accept bids
- **Status validation tests**: Only verified invoices and placed bids
- **Token transfer tests**: Exact amounts transferred correctly
- **Idempotency tests**: Double-accept prevention
- **State transition tests**: All status changes occur atomically
- **Edge case tests**: Expired bids, cancelled invoices, withdrawn bids
- **Invariant tests**: Escrow data consistency
- **Event tests**: Correct events emitted on success/failure

See `src/test_escrow.rs` for full test suite.

## Future Enhancements

Potential improvements for future versions:

1. **Partial Funding**: Allow multiple bids to fund portions of an invoice
2. **Bid Ranking**: Automatically accept best bid based on ranking
3. **Time Locks**: Add minimum holding period before refund
4. **Fee Integration**: Deduct platform fees during escrow creation
5. **Multi-Currency**: Support multiple token types per invoice
6. **Batch Operations**: Accept multiple bids in single transaction
7. **Escrow Extensions**: Allow extending escrow hold period
8. **Conditional Release**: Release based on oracle data or milestones

## Backward Compatibility

Any future changes must maintain:
- Existing function signatures for public API
- Storage key formats for existing data
- Event structures for external integrations
- Error codes for client error handling

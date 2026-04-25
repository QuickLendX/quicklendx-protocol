# Invoice Settlement & Partial Payments

This document describes the settlement module's payment record storage architecture, bounded storage design, and security guarantees.

## Overview

The settlement module handles invoice payment processing with support for partial payments, durable per-payment storage records, and finalization safety guards. A critical design goal is preventing unbounded storage growth that could lead to denial-of-service (DoS) attacks or gas limit issues.

## Payment Record Storage Architecture

### Storage Keys

Payment records are stored in Soroban persistent storage using the following key structure:

```rust
enum SettlementDataKey {
    PaymentCount(BytesN<32>),           // Current count of payments for an invoice
    Payment(BytesN<32>, u32),           // Individual payment record by index
    PaymentNonce(BytesN<32>, String),   // Nonce tracking for replay protection
    Finalized(BytesN<32>),              // Settlement finalization flag
}
```

### Payment Record Structure

Each payment record contains:

```rust
pub struct SettlementPaymentRecord {
    pub payer: Address,      // Address that made the payment
    pub amount: i128,        // Amount applied (may be capped from requested)
    pub timestamp: u64,      // Ledger timestamp when recorded
    pub nonce: String,       // Transaction identifier for deduplication
}
```

## Bounded Storage Design

### Maximum Payment Count

To prevent unbounded storage growth and protect against payment-count overflow, the contract enforces a hard cap on the number of payment records per invoice:

| Constant | Value | Purpose |
|----------|-------|---------|
| `MAX_PAYMENT_COUNT` | 1,000 | Maximum discrete payment records per invoice |
| `MAX_INLINE_PAYMENT_HISTORY` | 32 | Maximum payments stored inline in the Invoice struct |

### Why Bounded Storage?

1. **DoS Prevention**: Without a cap, an attacker could force the contract to store unlimited payment records, exhausting storage quotas and increasing costs for all users.

2. **Gas Limit Protection**: Query operations that iterate over payment records (e.g., `get_payment_records`) must complete within Soroban's compute limits. A bounded count ensures predictable gas costs.

3. **Overflow Prevention**: The payment count is stored as `u32`. While `u32::MAX` (4.2 billion) is theoretically safe, practical limits prevent edge cases and ensure reasonable usage patterns.

4. **Storage Cost predictability**: Users and the protocol can predict maximum storage costs per invoice.

### Regression tests added

- `test_partial_payment_rejected_after_explicit_settlement` ensures explicit settlement blocks further partial payments and produces no side effects.
- `test_settlement_idempotency_no_side_effects` verifies repeated settlement attempts are rejected and cause no additional balance, event, or accounting changes.

### Accounting Invariant
Before disbursing funds, the settlement engine asserts:

The cap is enforced in `record_payment()`:

```rust
let payment_count = get_payment_count_internal(env, invoice_id);

// Guard against unbounded payment record growth.
if payment_count >= MAX_PAYMENT_COUNT {
    return Err(QuickLendXError::OperationNotAllowed);
}
```

### Behavior at Cap Boundary

| Scenario | Behavior |
|----------|----------|
| Payment count < 1000 | Payment accepted and recorded |
| Payment count = 1000 | Payment rejected with `OperationNotAllowed` |
| Payment count > 1000 | Impossible (cannot exceed cap) |

### Secondary Guards

In addition to the payment count cap, the contract includes secondary defenses:

1. **Status Guard**: Once an invoice is marked as `Paid`, further payment attempts are rejected with `InvalidStatus`.

2. **Finalization Flag**: The `Finalized` storage key marks an invoice as settled, preventing double-settlement.

3. **Overpayment Capping**: If a payment would exceed the remaining due amount, only the remaining amount is applied.

## Query Functions

All query functions remain stable and efficient near the cap boundary:

### `get_payment_count(env, invoice_id) -> u32`

Returns the total number of recorded payments for an invoice. O(1) operation.

### `get_payment_record(env, invoice_id, index) -> SettlementPaymentRecord`

Returns a single payment record by index. O(1) operation. Returns `StorageKeyNotFound` if index is out of bounds.

### `get_payment_records(env, invoice_id, from, limit) -> Vec<SettlementPaymentRecord>`

Returns a paginated slice of payment records. Records are returned in chronological order (index 0 = first payment).

**Parameters:**
- `from`: Starting index (inclusive)
- `limit`: Maximum number of records to return (capped at `MAX_QUERY_LIMIT` for gas safety)

**Behavior near cap:**
- Querying beyond available records returns an empty vector
- Partial pages at the end return only available records
- Full range query (0, 1000) at cap returns all 1000 records

### `get_invoice_progress(env, invoice_id) -> Progress`

Returns aggregate payment progress including:
- `total_due`: Original invoice amount
- `total_paid`: Sum of all applied payments
- `remaining_due`: Amount still owed
- `progress_percent`: Percentage paid (0-100)
- `payment_count`: Number of recorded payments
- `status`: Current invoice status

## Error Handling

| Error | Code | When Returned |
|-------|------|---------------|
| `OperationNotAllowed` | 2001 | Payment count has reached `MAX_PAYMENT_COUNT` |
| `InvalidStatus` | 1003 | Invoice is already `Paid`, `Cancelled`, or not in `Funded` state |
| `InvalidAmount` | 1200 | Payment amount ≤ 0, or would cause accounting invariant violation |
| `InvoiceNotFound` | 1001 | Invoice ID does not exist |
| `StorageKeyNotFound` | 3001 | Payment record at index does not exist |
| `NotBusinessOwner` | 1401 | Payer is not the invoice business |

## Security Considerations

### Threat Model

1. **Storage DoS**: An attacker attempts to exhaust contract storage by creating many small payments.
   - **Mitigation**: `MAX_PAYMENT_COUNT` cap prevents unbounded growth.

2. **Gas Exhaustion**: An attacker creates enough payments to make queries exceed gas limits.
   - **Mitigation**: Cap ensures maximum iteration count; `get_payment_records` enforces `MAX_QUERY_LIMIT`.

3. **Replay Attacks**: An attacker replays a valid payment transaction.
   - **Mitigation**: Nonce-based deduplication tracks seen transaction IDs per invoice.

4. **Double Settlement**: An attacker attempts to settle an already-settled invoice.
   - **Mitigation**: `Finalized` flag and status checks prevent re-entry.

### Security Invariants

1. **`total_paid <= total_due`**: Enforced at every payment recording step. Amounts are capped to prevent overpayment.

2. **Payment count bounded**: `payment_count <= MAX_PAYMENT_COUNT` for all invoices.

3. **Idempotent settlement**: Once `status == Paid`, further settlement attempts are rejected.

4. **Accounting identity**: `investor_return + platform_fee == total_paid` is asserted before fund disbursement.

### Authorization

- Payment recording requires authorization from the invoice business address.
- The business must explicitly approve each payment via Soroban's `require_auth()`.

## Testing

The payment count cap enforcement is validated by comprehensive tests in `test_partial_payments.rs`:

| Test | Purpose |
|------|---------|
| `test_payment_count_cap_is_enforced` | Verifies 1001st payment is rejected |
| `test_payment_just_before_cap_succeeds` | Verifies 1000th payment succeeds |
| `test_queries_stable_near_cap` | Validates query correctness with 999 payments |
| `test_pagination_at_cap_boundary` | Tests pagination at exactly 1000 records |
| `test_cap_is_per_invoice_not_global` | Confirms cap is per-invoice, not global |
| `test_settled_invoice_rejects_additional_payments` | Status guard validation |
| `test_finalization_flag_after_settlement` | Finalization flag correctness |
| `test_payment_record_fields_are_complete` | Record structure validation |
| `test_duplicate_nonce_does_not_increment_count` | Replay protection |
| `test_empty_nonce_each_creates_separate_record` | Empty nonce handling |

## Usage Examples

### Recording Partial Payments

```rust
// Record a partial payment
let result = record_payment(
    &env,
    &invoice_id,
    &business_address,
    500_000,  // Payment amount
    "tx-hash-123".to_string(),  // Unique transaction ID
);
```

### Querying Payment Progress

```rust
// Get current payment progress
let progress = get_invoice_progress(&env, &invoice_id)?;
println!("Paid: {}/{} ({}%)", 
    progress.total_paid, 
    progress.total_due, 
    progress.progress_percent
);
println!("Payment count: {}", progress.payment_count);
```

### Paginated Payment History

```rust
// Get first 50 payment records
let records = get_payment_records(&env, &invoice_id, 0, 50)?;
for record in records.iter() {
    println!("Payment: {} from {} at {}", 
        record.amount, 
        record.payer, 
        record.timestamp
    );
}
```

## References

- Implementation: `quicklendx-contracts/src/settlement.rs`
- Tests: `quicklendx-contracts/src/test_partial_payments.rs`
- Error definitions: `quicklendx-contracts/src/errors.rs`
- Related: `docs/contracts/limits.md`, `docs/contracts/invoice.md`
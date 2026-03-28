# Protocol Limits

This document describes the protocol limits enforced by the QuickLendX contract to ensure system stability and prevent abuse.

## Maximum Active Invoices Per Business

### Overview

The QuickLendX protocol enforces a limit on the maximum number of active invoices that any single business can have simultaneously. This limit prevents spam, ensures system resources are fairly distributed, and maintains platform performance.

### Definition

An **active invoice** is any invoice with one of the following statuses:
- `Pending` - Invoice uploaded, awaiting verification
- `Verified` - Invoice verified and available for bidding  
- `Funded` - Invoice has been funded by an investor

**Terminal statuses** (not counted toward the limit):
- `Paid` - Invoice has been paid and settled
- `Defaulted` - Invoice payment is overdue/defaulted
- `Cancelled` - Invoice has been cancelled by the business owner
- `Refunded` - Invoice has been refunded

### Current Limit

The maximum number of active invoices per business is **100**.

This limit is enforced by the `MAX_ACTIVE_INVOICES_PER_BUSINESS` constant in `src/protocol_limits.rs`.

### Implementation Details

#### Status Classification

The `is_active_status()` function provides exhaustive matching to determine if an invoice status is active:

```rust
pub fn is_active_status(status: &InvoiceStatus) -> bool {
    match status {
        InvoiceStatus::Pending => true,
        InvoiceStatus::Verified => true,
        InvoiceStatus::Funded => true,
        InvoiceStatus::Paid => false,
        InvoiceStatus::Defaulted => false,
        InvoiceStatus::Cancelled => false,
        InvoiceStatus::Refunded => false,
    }
}
```

**Security Note**: This function uses exhaustive matching without a wildcard arm to ensure compile-time errors when new `InvoiceStatus` variants are added without updating this classification.

#### Limit Enforcement

The `check_invoice_limit()` function enforces the limit before any invoice creation:

```rust
pub fn check_invoice_limit(env: &Env, business: &Address) -> Result<(), QuickLendXError> {
    let active_count = count_active_invoices(env, business)?;
    let limit = MAX_ACTIVE_INVOICES_PER_BUSINESS;
    
    if active_count >= limit {
        return Err(QuickLendXError::MaxInvoicesPerBusinessExceeded);
    }
    
    Ok(())
}
```

**Security Features**:
- **Off-by-one prevention**: Uses `>=` comparison (not `>`) to block businesses at exactly the limit
- **Check-before-insert ordering**: Limit check happens BEFORE new invoice is written to storage
- **Authoritative counting**: Always reads from on-chain storage, no cached values

#### Active Invoice Counting

The `count_active_invoices()` function counts active invoices for a business:

```rust
pub fn count_active_invoices(env: &Env, business: &Address) -> Result<u32, QuickLendXError> {
    let invoices = InvoiceStorage::get_business_invoices(env, business)?;
    let mut active_count = 0u32;
    
    for invoice_id in invoices.iter() {
        if let Some(invoice) = InvoiceStorage::get_invoice(env, invoice_id) {
            if is_active_status(&invoice.status) {
                active_count = active_count.saturating_add(1);
            }
        }
    }
    
    Ok(active_count)
}
```

### Integration Points

#### Invoice Creation

The limit check is integrated into the `Invoice::new()` function:

```rust
pub fn new(
    env: &Env,
    business: Address,
    // ... other parameters
) -> Result<Self, QuickLendXError> {
    check_string_length(&description, MAX_DESCRIPTION_LENGTH)?;

    // Enforce maximum active invoices per business (status-aware limit)
    // This check is performed BEFORE any storage writes to prevent race conditions
    check_invoice_limit(env, &business)?;
    
    // ... rest of invoice creation logic
}
```

### Error Handling

When a business exceeds the limit, the contract returns:

```
QuickLendXError::MaxInvoicesPerBusinessExceeded
```

**User-facing message**:
> "You have reached the maximum number of active invoices (100).
> An existing invoice must be resolved before you can submit a new one."

### Future Limit Changes

Any change to `MAX_ACTIVE_INVOICES_PER_BUSINESS` is a **breaking change** for businesses currently at or near the limit. Changes must:

1. **Be announced** in `CHANGELOG.md` with a version bump
2. **Apply only to NEW** invoice submissions (existing invoices are unaffected)
3. **Be subject to governance approval** (if governance is implemented)

### Testing

Comprehensive tests are located in `src/test_max_invoices_per_business.rs`:

- **Limit enforcement**: Verifies businesses cannot exceed the limit
- **Status-aware counting**: Confirms only active invoices count toward limit
- **Slot freeing**: Validates that resolving invoices frees up capacity
- **Multiple businesses**: Ensures limits are enforced per-business independently
- **Edge cases**: Tests limit of 1, unlimited (0), and boundary conditions

### Security Considerations

#### Race Condition Prevention
The limit check is performed **before** any storage writes. This prevents two concurrent invoice submissions from both passing the check and exceeding the limit.

#### Storage Consistency
Active invoice counts are always computed from current on-chain storage state. No cached or pre-computed counts are used that could be manipulated.

#### Exhaustive Matching
The `is_active_status()` function requires explicit handling of all status variants. Adding new statuses without updating this function causes compile errors, preventing silent misclassification.

#### Overflow Protection
All counting operations use `saturating_add()` to prevent overflow attacks.

### Monitoring

Operators should monitor:
- Frequency of `MaxInvoicesPerBusinessExceeded` errors
- Average active invoices per business
- Businesses consistently hitting the limit (potential candidates for higher limits)

### Related Documentation

- [Protocol Limits Implementation](../src/protocol_limits.rs)
- [Invoice Status Lifecycle](invoice-lifecycle.md)
- [Error Handling](errors.md)
- [Testing Guide](../testing/limits.md)

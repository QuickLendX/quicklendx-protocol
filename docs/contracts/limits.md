# Contract String Length Limits

To ensure predictable storage usage and prevent potential resource abuse, the QuickLendX protocol enforces maximum length limits on user-supplied strings and minimum/maximum value constraints on numeric inputs.

## String Length Limits

These limits are defined in `src/protocol_limits.rs` and enforced across the contract modules.

| Input Field | Maximum Length (Bytes) | Module |
|-------------|-------------------------|--------|
| Invoice Description | 500 | `invoice` |
| Rating Feedback | 200 | `invoice` |
| Customer Name (Metadata) | 100 | `invoice` |
| Customer Address (Metadata) | 200 | `invoice` |
| Tax ID (Metadata) | 50 | `invoice` |
| Notes (Metadata) | 1000 | `invoice` |
| Dispute Reason | 500 | `defaults` (disputes) |
| Dispute Evidence | 1000 | `defaults` (disputes) |
| Dispute Resolution | 2000 | `defaults` (disputes) |
| Notification Title | 100 | `notifications` |
| Notification Message | 500 | `notifications` |
| KYC Data | 5000 | `verification` |
| Rejection Reason | 1000 | `verification` |

## Numeric Value Limits

The protocol enforces minimum and maximum values for critical numeric inputs to ensure platform integrity and prevent abuse.

### Invoice Amount Limits

| Limit | Default Value | Configurable | Description |
|-------|---------------|--------------|-------------|
| `min_invoice_amount` | 1,000,000 (production)<br>1,000 (test) | Yes (admin only) | Minimum acceptable invoice value in smallest currency unit (e.g., stroops). Prevents dust invoices and ensures economic viability. |
| `min_bid_amount` | 100 | Yes (admin only) | Absolute minimum bid amount for dust protection |
| `min_bid_bps` | 100 (1%) | Yes (admin only) | Minimum bid as percentage of invoice amount |
| `max_due_date_days` | 365 | Yes (admin only) | Maximum days in the future for invoice due dates |
| `grace_period_seconds` | 604,800 (7 days) | Yes (admin only) | Grace period after due date before default |

### Validation Flow

When an invoice is created via `store_invoice` or `upload_invoice`:

1. **Basic validation**: Amount must be positive (`> 0`)
2. **Protocol limits validation**: Amount must meet or exceed `min_invoice_amount`
3. **Due date validation**: Must be in the future and within `max_due_date_days`

```rust
// Validation is performed in protocol_limits::ProtocolLimitsContract::validate_invoice
if amount < limits.min_invoice_amount {
    return Err(QuickLendXError::InvalidAmount);
}
```

### Admin Configuration

The admin can update protocol limits using `set_protocol_limits`:

```rust
client.set_protocol_limits(
    &admin,
    &5_000_000,  // min_invoice_amount (5 tokens with 6 decimals)
    &100,        // min_bid_amount
    &100,        // min_bid_bps (1%)
    &180,        // max_due_date_days (6 months)
    &86400       // grace_period_seconds (1 day)
);
```

## Error Handling

### String Length Errors

When a string exceeds its defined limit, the contract will return an `InvalidDescription` (Code 1204) error. 

> [!NOTE]
> `InvalidDescription` is used as a generic "invalid input string" error to maintain contract compatibility while adhering to SDK limitations on error variant counts.

### Amount Validation Errors

When an amount fails validation, the contract returns:
- `InvalidAmount` (Code 1200) - For amounts ≤ 0 or below `min_invoice_amount`
- `InvoiceDueDateInvalid` (Code 1004) - For due dates outside acceptable range

## Validation Logic

### String Validation

Validation is performed using the `check_string_length` helper:

```rust
pub fn check_string_length(s: &String, max_len: u32) -> Result<(), QuickLendXError> {
    if s.len() > max_len {
        return Err(QuickLendXError::InvalidDescription);
    }
    Ok(())
}
```

### Invoice Validation

Complete invoice validation including amount and due date:

```rust
pub fn validate_invoice(env: Env, amount: i128, due_date: u64) -> Result<(), QuickLendXError> {
    let limits = Self::get_protocol_limits(env.clone());
    let current_time = env.ledger().timestamp();

    // Check minimum amount
    if amount < limits.min_invoice_amount {
        return Err(QuickLendXError::InvalidAmount);
    }

    // Check maximum due date
    let max_due_date = current_time.saturating_add(limits.max_due_date_days.saturating_mul(86400));
    if due_date > max_due_date {
        return Err(QuickLendXError::InvoiceDueDateInvalid);
    }

    Ok(())
}
```

## Security Considerations

- **Single source of truth**: All limits are centralized in `protocol_limits.rs`
- **Admin-only updates**: Only the designated admin can modify protocol limits
- **Validation at entry points**: Both `store_invoice` and `upload_invoice` enforce limits
- **Immutable after creation**: Invoice amounts cannot be changed after creation
- **Test vs production defaults**: Different defaults allow for easier testing while maintaining production security

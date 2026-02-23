# Protocol Limits

## Overview

Configurable system-wide constraints for invoice validation and default handling. Admin-controlled parameters ensure consistent risk management across the platform.

The Protocol Limits module provides three critical parameters that govern invoice creation and default processing:
- **Minimum Invoice Amount**: Prevents spam invoices and ensures economic viability
- **Maximum Due Date**: Balances flexibility with risk management
- **Grace Period**: Provides buffer time before invoices can be marked as defaulted

## Configuration Parameters

| Parameter | Type | Description | Bounds | Default |
|-----------|------|-------------|--------|---------|
| `min_invoice_amount` | `i128` | Minimum acceptable invoice value | > 0 | 1,000,000 |
| `max_due_date_days` | `u64` | Maximum days from now for due dates | 1 - 730 | 365 |
| `grace_period_seconds` | `u64` | Default grace period after due date | 0 - 2,592,000 | 86400 |

### Default Values Rationale

```rust
min_invoice_amount: 1_000_000      // 1 token (6 decimals) - prevents spam
max_due_date_days: 365             // 1 year maximum - balances flexibility with risk
grace_period_seconds: 86400        // 24 hours - reasonable payment processing buffer
```

## Contract Interface

### Administrative Functions

#### `initialize(admin: Address) -> Result<(), QuickLendXError>`

Initializes protocol limits with default values. **One-time operation only.**

**Parameters:**
- `admin`: Address that will have permission to update limits

**Returns:**
- `Ok(())`: Initialization successful
- `Err(OperationNotAllowed)`: Already initialized

**Example:**
```rust
use quicklendx_contracts::QuickLendXContractClient;

let admin = Address::generate(&env);
client.initialize_protocol_limits(&admin)?;
```

**Security Notes:**
- Can only be called once
- No authorization required for initial setup
- Admin address is permanently stored
- Cannot be re-initialized or changed

---

#### `set_protocol_limits(admin: Address, min_invoice_amount: i128, max_due_date_days: u64, grace_period_seconds: u64) -> Result<(), QuickLendXError>`

Updates protocol limits. **Requires admin authorization.**

**Parameters:**
- `admin`: Admin address (must match stored admin)
- `min_invoice_amount`: New minimum invoice amount (must be > 0)
- `max_due_date_days`: New maximum due date days (must be 1-730)
- `grace_period_seconds`: New grace period (must be 0-2,592,000)

**Returns:**
- `Ok(())`: Update successful
- `Err(Unauthorized)`: Caller not admin
- `Err(NotAdmin)`: Admin not configured
- `Err(InvalidAmount)`: Amount ≤ 0
- `Err(InvoiceDueDateInvalid)`: Days outside 1-730 range
- `Err(InvalidTimestamp)`: Grace period > 30 days

**Example:**
```rust
// Update to more restrictive limits
client.set_protocol_limits(
    &admin,
    &5_000_000,    // 5 tokens minimum
    &180,          // 6 months max
    &43200         // 12 hours grace
)?;
```

**Security Notes:**
- Requires `admin.require_auth()`
- Verifies caller matches stored admin
- All parameters validated before storage
- Changes apply immediately to new operations

**Recovery from Errors:**
- `Unauthorized`: Use the correct admin address
- `NotAdmin`: Initialize the system first
- `InvalidAmount`: Provide a positive amount
- `InvoiceDueDateInvalid`: Use 1-730 days
- `InvalidTimestamp`: Use 0-2,592,000 seconds (30 days max)

---

### Query Functions

#### `get_protocol_limits() -> ProtocolLimits`

Returns current configuration. **Always available, never fails.**

**Returns:**
- Current limits if initialized
- Default values if not initialized

**Example:**
```rust
let limits = client.get_protocol_limits();
println!("Min amount: {}", limits.min_invoice_amount);
println!("Max days: {}", limits.max_due_date_days);
println!("Grace period: {}", limits.grace_period_seconds);
```

---

#### `validate_invoice(amount: i128, due_date: u64) -> bool`

Validates invoice against current limits. **Convenience function for invoice validation.**

**Parameters:**
- `amount`: Invoice amount to validate
- `due_date`: Invoice due date timestamp to validate

**Returns:**
- `true`: Invoice parameters are valid
- `false`: Invoice parameters violate limits

**Validation Logic:**
1. Check `amount >= min_invoice_amount`
2. Calculate `max_due_date = current_time + (max_due_date_days * 86400)`
3. Check `due_date <= max_due_date`

**Example:**
```rust
let amount = 5_000_000;
let due_date = env.ledger().timestamp() + (30 * 86400); // 30 days

if client.validate_invoice(&amount, &due_date) {
    // Proceed with invoice creation
} else {
    return Err(QuickLendXError::InvoiceAmountInvalid);
}
```

---

#### `get_default_date(due_date: u64) -> u64`

Calculates default date by adding grace period to due date.

**Parameters:**
- `due_date`: The invoice due date timestamp

**Returns:**
- Timestamp when default can be triggered (due_date + grace_period_seconds)

**Example:**
```rust
let due_date = 1_000_000u64;
let default_date = client.get_default_date(&due_date);
// default_date = 1_086_400 (with default 24h grace period)
```

## Integration

### Invoice Module Integration

Protocol limits are enforced during invoice creation in `verify_invoice_data`:

```rust
use crate::protocol_limits::ProtocolLimitsContract;

pub fn verify_invoice_data(
    env: &Env,
    _business: &Address,
    amount: i128,
    currency: &Address,
    due_date: u64,
    description: &String,
) -> Result<(), QuickLendXError> {
    // Use protocol limits for validation
    if !ProtocolLimitsContract::validate_invoice(env.clone(), amount, due_date) {
        let limits = ProtocolLimitsContract::get_protocol_limits(env.clone());
        
        if amount < limits.min_invoice_amount {
            return Err(QuickLendXError::InvoiceAmountInvalid);
        }
        
        return Err(QuickLendXError::InvoiceDueDateInvalid);
    }
    
    // ... other validation
    Ok(())
}
```

### Default Module Integration

Grace period is used in default handling:

```rust
use crate::protocol_limits::ProtocolLimitsContract;

pub fn handle_default(
    env: &Env,
    invoice_id: &BytesN<32>,
) -> Result<(), QuickLendXError> {
    let invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;
    
    let current_time = env.ledger().timestamp();
    let grace_deadline = ProtocolLimitsContract::get_default_date(
        env.clone(),
        invoice.due_date
    );
    
    if current_time < grace_deadline {
        return Err(QuickLendXError::OperationNotAllowed);
    }
    
    // ... mark as defaulted
    Ok(())
}
```

## Error Handling

All operations use `QuickLendXError` enum for consistent error reporting:

| Error | Code | Condition | Recovery |
|-------|------|-----------|----------|
| `InvalidAmount` | 1002 | Amount ≤ 0 | Provide positive amount |
| `InvoiceDueDateInvalid` | 1013 | Days outside 1-730 | Use 1-730 days |
| `InvalidTimestamp` | 1017 | Grace period > 30 days | Use ≤ 2,592,000 seconds |
| `Unauthorized` | 1004 | Non-admin update attempt | Use correct admin address |
| `NotAdmin` | 1005 | Admin not configured | Initialize first |
| `OperationNotAllowed` | 1009 | Re-initialization attempted | Cannot recover, already initialized |

## Usage Examples

### Complete Initialization and Update Flow

```rust
use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{Address, Env};

// Setup
let env = Env::default();
let contract_id = env.register(QuickLendXContract, ());
let client = QuickLendXContractClient::new(&env, &contract_id);
let admin = Address::generate(&env);

// 1. Initialize protocol limits
client.initialize_protocol_limits(&admin)?;

// 2. Query current limits
let limits = client.get_protocol_limits();
assert_eq!(limits.min_invoice_amount, 1_000_000);

// 3. Update limits (admin only)
client.set_protocol_limits(
    &admin,
    &5_000_000,    // 5 tokens minimum
    &180,          // 6 months max
    &43200         // 12 hours grace
)?;

// 4. Validate before invoice creation
let amount = 10_000_000;
let due_date = env.ledger().timestamp() + (90 * 86400);

if !client.validate_invoice(&amount, &due_date) {
    return Err(QuickLendXError::InvoiceAmountInvalid);
}

// 5. Calculate default date
let default_date = client.get_default_date(&due_date);
```

### Error Handling Example

```rust
match client.try_set_protocol_limits(&admin, &0, &180, &43200) {
    Ok(()) => println!("Limits updated"),
    Err(Ok(QuickLendXError::InvalidAmount)) => {
        println!("Amount must be positive");
        // Retry with valid amount
        client.set_protocol_limits(&admin, &1_000_000, &180, &43200)?;
    }
    Err(Ok(QuickLendXError::Unauthorized)) => {
        println!("Only admin can update limits");
    }
    Err(e) => return Err(e),
}
```

## Security Considerations

### Authorization Model

1. **Initialization**: No authorization required (first-time setup)
2. **Updates**: Requires admin authorization via `require_auth()`
3. **Queries**: Public read access (no authorization needed)

### Input Validation

All numeric inputs are validated against defined ranges:
- **Amount**: Must be positive (> 0)
- **Days**: Must be 1-730 (2 years maximum)
- **Grace Period**: Must be 0-2,592,000 seconds (30 days maximum)

### Arithmetic Safety

- Uses saturating arithmetic to prevent overflow
- Boundary checks before all calculations
- Type-safe storage operations

### Storage Security

- Unique storage keys prevent collisions
- Instance storage for frequently accessed data
- Atomic operations ensure consistency

## Testing

### Running Tests

```bash
# Run all protocol limits tests
cargo test test_protocol_limits --manifest-path quicklendx-contracts/Cargo.toml

# Run specific test
cargo test test_initialize_success --manifest-path quicklendx-contracts/Cargo.toml
```

### Test Coverage

The test suite includes:
- **Initialization**: Default values, double initialization prevention
- **Updates**: Admin authorization, parameter validation, boundary conditions
- **Queries**: Before/after initialization, persistence
- **Validation**: Amount and due date validation logic
- **Persistence**: Storage consistency across operations

All 15 tests pass with comprehensive coverage of:
- Success paths
- Error conditions
- Boundary values
- Authorization checks
- Storage persistence

## Best Practices

1. **Initialize Early**: Call `initialize_protocol_limits` during contract deployment
2. **Validate Before Storage**: Use `validate_invoice` before creating invoices
3. **Handle Errors Gracefully**: Check for specific error types and provide user feedback
4. **Monitor Limits**: Periodically review and adjust limits based on platform metrics
5. **Test Updates**: Verify limit changes in a test environment before production
6. **Document Changes**: Log all limit updates for audit trail

## Performance Considerations

- **Storage Access**: O(1) - Direct key lookup in instance storage
- **Validation**: O(1) - Two comparisons and one addition
- **Memory**: Fixed size (~24 bytes for 3 fields)
- **Gas Efficiency**: Minimal storage reads/writes, efficient validation logic

## Future Enhancements

Potential improvements for future versions:
1. **Dynamic Limits by Currency**: Different min amounts per currency
2. **Business-Specific Limits**: Verified businesses get higher limits
3. **Time-Based Limits**: Seasonal adjustments based on market conditions
4. **Limit History**: Track limit changes over time for audit trail
5. **Automated Adjustments**: Algorithm-based limit optimization

These enhancements would require additional design and security review.

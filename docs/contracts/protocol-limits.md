# Protocol Limits

## Overview

Configurable system-wide constraints for invoice validation and default handling. Admin-controlled parameters ensure consistent risk management across the platform.

## Configuration Parameters

| Parameter | Type | Description | Bounds |
|-----------|------|-------------|--------|
| `min_invoice_amount` | `i128` | Minimum acceptable invoice value | > 0 |
| `max_due_date_days` | `u64` | Maximum days from now for due dates | 1 - 730 |
| `grace_period_seconds` | `u64` | Default grace period after due date | 0 - 2,592,000 |

## Default Values

```rust
min_invoice_amount: 1_000_000      // 1 token (6 decimals)
max_due_date_days: 365             // 1 year maximum
grace_period_seconds: 86400        // 24 hours
```

## Contract Interface

### Administrative Functions

#### `initialize(admin: Address) -> Result<(), QuickLendXError>`
Initializes protocol limits with default values. One-time operation.

**Errors:**
- `OperationNotAllowed`: Already initialized

#### `set_protocol_limits(admin: Address, min_invoice_amount: i128, max_due_date_days: u64, grace_period_seconds: u64) -> Result<(), QuickLendXError>`
Updates protocol limits. Requires admin authorization.

**Errors:**
- `Unauthorized`: Caller not admin
- `NotAdmin`: Admin not configured
- `InvalidAmount`: Amount ≤ 0
- `InvoiceDueDateInvalid`: Days outside 1-730 range
- `InvalidTimestamp`: Grace period > 30 days

### Query Functions

#### `get_protocol_limits() -> ProtocolLimits`
Returns current configuration. Always available, returns defaults if not initialized.

#### `validate_invoice(amount: i128, due_date: u64) -> bool`
Validates invoice against current limits.

**Validation Logic:**
- Amount must meet minimum threshold
- Due date must not exceed maximum offset from current time

#### `get_default_date(due_date: u64) -> u64`
Calculates default date by adding grace period to due date.

## Integration

### Invoice Storage

Protocol limits are enforced during invoice creation:

```rust
// Validation in store_invoice
let limits = get_protocol_limits(&env);

if amount < limits.min_invoice_amount {
    return Err(QuickLendXError::InvoiceAmountInvalid);
}

let max_due_date = current_time + (limits.max_due_date_days * 86400);
if due_date > max_due_date {
    return Err(QuickLendXError::InvoiceDueDateInvalid);
}

let default_date = due_date + limits.grace_period_seconds;
```


## Error Handling

All operations use `QuickLendXError` enum for consistent error reporting:

| Error | Code | Condition |
|-------|------|-----------|
| `InvalidAmount` | 1002 | Amount validation failed |
| `InvoiceDueDateInvalid` | 1013 | Due date validation failed |
| `InvalidTimestamp` | 1017 | Grace period out of bounds |
| `Unauthorized` | 1004 | Non-admin attempted update |
| `NotAdmin` | 1005 | Admin not configured |
| `OperationNotAllowed` | 1009 | Re-initialization attempted |

## Usage Example

```rust
// Initialize protocol
initialize(env.clone(), admin_address)?;

// Update limits (admin only)
set_protocol_limits(
    env.clone(),
    admin_address,
    5_000_000,    // 5 tokens minimum
    180,          // 6 months max
    43200         // 12 hours grace
)?;

// Query current limits
let limits = get_protocol_limits(env.clone());

// Validate before storage
if !validate_invoice(env.clone(), amount, due_date) {
    return Err(QuickLendXError::InvoiceAmountInvalid);
}
```
# Protocol Limits

## Overview

Configurable system-wide constraints for invoice validation and default handling. Admin-controlled parameters ensure consistent risk management across the platform.

## Configuration Parameters

| Parameter | Type | Description | Bounds |
|-----------|------|-------------|--------|
| `min_invoice_amount` | `i128` | Minimum acceptable invoice value | > 0 |
| `max_due_date_days` | `u64` | Maximum days from now for due dates | 1 - 730 |
| `grace_period_seconds` | `u64` | Default grace period after due date | 0 - 2,592,000 |

## Default Values

```rust
min_invoice_amount: 1_000_000      // 1 token (6 decimals)
max_due_date_days: 365             // 1 year maximum
grace_period_seconds: 604800       // 7 days
```

## Contract Interface

### Administrative Functions

#### `initialize(admin: Address) -> Result<(), QuickLendXError>`
Initializes protocol limits with default values. One-time operation.

**Errors:**
- `OperationNotAllowed`: Already initialized

#### `set_protocol_limits(admin: Address, min_invoice_amount: i128, max_due_date_days: u64, grace_period_seconds: u64) -> Result<(), QuickLendXError>`
Updates protocol limits. Requires admin authorization.

**Errors:**
- `Unauthorized`: Caller not admin
- `NotAdmin`: Admin not configured
- `InvalidAmount`: Amount ≤ 0
- `InvoiceDueDateInvalid`: Days outside 1-730 range
- `InvalidTimestamp`: Grace period > 30 days

### Query Functions

#### `get_protocol_limits() -> ProtocolLimits`
Returns current configuration. Always available, returns defaults if not initialized.

#### `validate_invoice(amount: i128, due_date: u64) -> bool`
Validates invoice against current limits.

**Validation Logic:**
- Amount must meet minimum threshold
- Due date must not exceed maximum offset from current time

#### `get_default_date(due_date: u64) -> u64`
Calculates default date by adding grace period to due date.

## Integration

### Invoice Storage

Protocol limits are enforced during invoice creation:

```rust
// Validation in store_invoice
let limits = get_protocol_limits(&env);

if amount < limits.min_invoice_amount {
    return Err(QuickLendXError::InvoiceAmountInvalid);
}

let max_due_date = current_time + (limits.max_due_date_days * 86400);
if due_date > max_due_date {
    return Err(QuickLendXError::InvoiceDueDateInvalid);
}

let default_date = due_date + limits.grace_period_seconds;
```


## Error Handling

All operations use `QuickLendXError` enum for consistent error reporting:

| Error | Code | Condition |
|-------|------|-----------|
| `InvalidAmount` | 1002 | Amount validation failed |
| `InvoiceDueDateInvalid` | 1013 | Due date validation failed |
| `InvalidTimestamp` | 1017 | Grace period out of bounds |
| `Unauthorized` | 1004 | Non-admin attempted update |
| `NotAdmin` | 1005 | Admin not configured |
| `OperationNotAllowed` | 1009 | Re-initialization attempted |

## Usage Example

```rust
// Initialize protocol
initialize(env.clone(), admin_address)?;

// Update limits (admin only)
set_protocol_limits(
    env.clone(),
    admin_address,
    5_000_000,    // 5 tokens minimum
    180,          // 6 months max
    43200         // 12 hours grace
)?;

// Query current limits
let limits = get_protocol_limits(env.clone());

// Validate before storage
if !validate_invoice(env.clone(), amount, due_date) {
    return Err(QuickLendXError::InvoiceAmountInvalid);
}
```

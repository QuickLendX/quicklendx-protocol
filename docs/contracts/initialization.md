# QuickLendX Protocol Initialization

## Overview

The QuickLendX protocol uses a secure, one-time initialization flow that sets up all critical configuration parameters in a single atomic operation. This document describes the initialization system, its security model, and usage patterns.

## Table of Contents

- [Architecture](#architecture)
- [Initialization Flow](#initialization-flow)
- [Security Model](#security-model)
- [Configuration Parameters](#configuration-parameters)
- [API Reference](#api-reference)
- [Events](#events)
- [Error Handling](#error-handling)
- [Testing](#testing)
- [Best Practices](#best-practices)

## Architecture

### Module Structure

```
quicklendx-contracts/src/
├── init.rs           # Core initialization module
├── admin.rs          # Admin role management
├── currency.rs       # Currency whitelist
└── test_init.rs      # Comprehensive test suite
```

### Key Components

1. **ProtocolInitializer**: Main struct providing initialization and configuration management
2. **InitializationParams**: Struct bundling all initialization parameters
3. **ProtocolConfig**: On-chain storage for protocol-wide parameters
4. **Storage Keys**: Isolated storage keys for different configuration aspects

## Initialization Flow

### Single-Shot Initialization

The protocol supports atomic initialization where all parameters are set in one transaction:

```rust
use quicklendx_contracts::init::{InitializationParams, ProtocolInitializer};
use soroban_sdk::{Address, Env, Vec};

fn initialize_protocol(env: &Env) {
    let params = InitializationParams {
        admin: Address::generate(env),
        treasury: Address::generate(env),
        fee_bps: 200,                    // 2%
        min_invoice_amount: 1_000_000,   // 1 token (6 decimals)
        max_due_date_days: 365,
        grace_period_seconds: 604800,     // 7 days
        initial_currencies: Vec::new(env),
    };

    ProtocolInitializer::initialize(env, &params)
        .expect("Initialization failed");
}
```

### Phased Initialization (Future)

While the current implementation uses single-shot initialization, the architecture supports future phased initialization:

1. **Phase 1**: Set admin and basic configuration
2. **Phase 2**: Configure fees and treasury
3. **Phase 3**: Add whitelisted currencies

## Security Model

### One-Time Initialization

- The contract can only be initialized **once**
- Subsequent calls to `initialize()` will fail with `OperationNotAllowed`
- Initialization state is stored in instance storage under key `proto_in`

### Admin Authorization

- Initialization requires authorization from the admin address
- Uses Soroban's built-in `require_auth()` mechanism
- Admin address is stored and used for all future admin operations

### Re-initialization Protection

```rust
pub fn is_initialized(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&PROTOCOL_INITIALIZED_KEY)
        .unwrap_or(false)
}
```

The initialization flag is set **after** all other state changes, ensuring atomicity.

### Parameter Validation

All parameters are validated before any state changes:

| Parameter | Validation | Error |
|-----------|------------|-------|
| `fee_bps` | 0 ≤ fee ≤ 1000 | `InvalidFeeBasisPoints` |
| `min_invoice_amount` | > 0 | `InvalidAmount` |
| `max_due_date_days` | 1 ≤ days ≤ 730 | `InvoiceDueDateInvalid` |
| `grace_period_seconds` | ≤ 2,592,000 (30 days) | `InvalidTimestamp` |

## Configuration Parameters

### InitializationParams

```rust
#[contracttype]
pub struct InitializationParams {
    pub admin: Address,                    // Protocol admin
    pub treasury: Address,                 // Fee collection address
    pub fee_bps: u32,                      // Fee in basis points (e.g., 200 = 2%)
    pub min_invoice_amount: i128,          // Minimum invoice amount
    pub max_due_date_days: u64,            // Max days until due date
    pub grace_period_seconds: u64,         // Grace period before default
    pub initial_currencies: Vec<Address>,  // Initial whitelisted currencies
}
```

### ProtocolConfig

Stored on-chain after initialization:

```rust
#[contracttype]
pub struct ProtocolConfig {
    pub min_invoice_amount: i128,    // Minimum allowed invoice amount
    pub max_due_date_days: u64,      // Maximum due date extension
    pub grace_period_seconds: u64,   // Default grace period
    pub updated_at: u64,             // Last update timestamp
    pub updated_by: Address,         // Last updater address
}
```

### Default Values

| Parameter | Default Value | Description |
|-----------|---------------|-------------|
| `fee_bps` | 200 | 2% platform fee |
| `min_invoice_amount` | 1,000,000 | 1 token (6 decimals) |
| `max_due_date_days` | 365 | 1 year maximum |
| `grace_period_seconds` | 604,800 | 7 days |

## API Reference

### Initialization Functions

#### `initialize`

```rust
pub fn initialize(
    env: &Env,
    params: &InitializationParams,
) -> Result<(), QuickLendXError>
```

Initializes the protocol with all configuration parameters.

**Authorization**: Requires auth from `params.admin`

**Errors**:
- `OperationNotAllowed` - Already initialized
- `InvalidFeeBasisPoints` - Fee out of range
- `InvalidAmount` - Min amount ≤ 0
- `InvoiceDueDateInvalid` - Due date days out of range
- `InvalidTimestamp` - Grace period too long

---

#### `is_initialized`

```rust
pub fn is_initialized(env: &Env) -> bool
```

Checks if the protocol has been initialized.

---

### Admin Configuration Functions

#### `set_protocol_config`

```rust
pub fn set_protocol_config(
    env: &Env,
    admin: &Address,
    min_invoice_amount: i128,
    max_due_date_days: u64,
    grace_period_seconds: u64,
) -> Result<(), QuickLendXError>
```

Updates protocol configuration parameters.

**Authorization**: Requires auth from admin

---

#### `set_fee_config`

```rust
pub fn set_fee_config(
    env: &Env,
    admin: &Address,
    fee_bps: u32,
) -> Result<(), QuickLendXError>
```

Updates the platform fee in basis points.

**Authorization**: Requires auth from admin

---

#### `set_treasury`

```rust
pub fn set_treasury(
    env: &Env,
    admin: &Address,
    treasury: &Address,
) -> Result<(), QuickLendXError>
```

Updates the treasury address for fee collection.

**Authorization**: Requires auth from admin

---

### Query Functions

#### `get_protocol_config`

```rust
pub fn get_protocol_config(env: &Env) -> Option<ProtocolConfig>
```

Returns the current protocol configuration.

---

#### `get_fee_bps`

```rust
pub fn get_fee_bps(env: &Env) -> u32
```

Returns the current fee in basis points (defaults to 200).

---

#### `get_treasury`

```rust
pub fn get_treasury(env: &Env) -> Option<Address>
```

Returns the treasury address if set.

---

#### `get_min_invoice_amount`

```rust
pub fn get_min_invoice_amount(env: &Env) -> i128
```

Returns the minimum invoice amount (defaults to 1,000,000).

---

#### `get_max_due_date_days`

```rust
pub fn get_max_due_date_days(env: &Env) -> u64
```

Returns the maximum due date days (defaults to 365).

---

#### `get_grace_period_seconds`

```rust
pub fn get_grace_period_seconds(env: &Env) -> u64
```

Returns the grace period in seconds (defaults to 604,800).

## Events

All initialization and configuration changes emit events for audit trails:

### `proto_in` - Protocol Initialized

Emitted when the protocol is successfully initialized.

```rust
(admin: Address, treasury: Address, fee_bps: u32, 
 min_invoice_amount: i128, max_due_date_days: u64, 
 grace_period_seconds: u64, timestamp: u64)
```

### `proto_cfg` - Protocol Config Updated

Emitted when protocol configuration is updated.

```rust
(admin: Address, min_invoice_amount: i128, max_due_date_days: u64, 
 grace_period_seconds: u64, timestamp: u64)
```

### `fee_cfg` - Fee Config Updated

Emitted when fee configuration is updated.

```rust
(admin: Address, fee_bps: u32, timestamp: u64)
```

### `trsr_upd` - Treasury Updated

Emitted when treasury address is updated.

```rust
(admin: Address, treasury: Address, timestamp: u64)
```

## Error Handling

### Error Types

| Error | Code | Description |
|-------|------|-------------|
| `OperationNotAllowed` | 1009 | Contract already initialized |
| `NotAdmin` | 1005 | Caller is not the admin |
| `InvalidFeeBasisPoints` | 1034 | Fee outside valid range (0-1000) |
| `InvalidAmount` | 1002 | Amount must be positive |
| `InvoiceDueDateInvalid` | 1013 | Due date days out of range |
| `InvalidTimestamp` | 1017 | Grace period exceeds maximum |

### Error Handling Example

```rust
match ProtocolInitializer::initialize(env, &params) {
    Ok(()) => {
        // Initialization successful
    }
    Err(QuickLendXError::OperationNotAllowed) => {
        // Contract already initialized
    }
    Err(QuickLendXError::InvalidFeeBasisPoints) => {
        // Fee must be between 0 and 1000
    }
    Err(e) => {
        // Handle other errors
    }
}
```

## Testing

The initialization module includes comprehensive tests covering:

### Test Categories

1. **Successful Initialization** (6 tests)
   - Default parameters
   - Admin storage
   - Treasury storage
   - Fee BPS storage
   - Protocol config storage
   - Currency whitelist

2. **Re-initialization Protection** (4 tests)
   - Double initialization fails
   - State preservation on failure
   - `is_initialized` returns correct values

3. **Parameter Validation - Fee BPS** (3 tests)
   - Too high fails
   - Max value succeeds
   - Zero succeeds

4. **Parameter Validation - Min Amount** (4 tests)
   - Zero fails
   - Negative fails
   - Small positive succeeds
   - Large amount succeeds

5. **Parameter Validation - Due Date** (4 tests)
   - Zero fails
   - Too high fails
   - Max value succeeds
   - One day succeeds

6. **Parameter Validation - Grace Period** (3 tests)
   - Too long fails
   - Max value succeeds
   - Zero succeeds

7. **Set Protocol Config** (5 tests)
   - Success case
   - Non-admin fails
   - Parameter validation
   - Timestamp updates

8. **Set Fee Config** (4 tests)
   - Success case
   - Non-admin fails
   - Validation
   - Zero allowed

9. **Set Treasury** (2 tests)
   - Success case
   - Non-admin fails

10. **Query Functions** (6 tests)
    - Before/after initialization
    - Default values

11. **Edge Cases** (3 tests)
    - Boundary values
    - Multiple updates
    - Config immutability

12. **Integration** (1 test)
    - Full workflow

### Running Tests

```bash
cd quicklendx-contracts
cargo test test_init -- --nocapture
```

### Test Coverage

Target: 95%+ coverage of initialization module

## Best Practices

### 1. Initialization Check

Always check if the protocol is initialized before dependent operations:

```rust
if !ProtocolInitializer::is_initialized(env) {
    panic!("Protocol not initialized");
}
```

### 2. Parameter Validation

Validate all parameters client-side before calling initialize:

```rust
fn validate_params(params: &InitializationParams) -> Result<(), String> {
    if params.fee_bps > 1000 {
        return Err("Fee too high".to_string());
    }
    if params.min_invoice_amount <= 0 {
        return Err("Min amount must be positive".to_string());
    }
    // ... more validation
    Ok(())
}
```

### 3. Event Monitoring

Monitor initialization events for audit purposes:

```rust
// Listen for proto_in events
// Verify all parameters match expected values
// Alert on unexpected initialization attempts
```

### 4. Secure Admin Key Management

- Use a multi-sig or hardware wallet for the admin address
- Consider time-locked admin operations for critical changes
- Have a plan for admin key rotation

### 5. Treasury Configuration

- Use a secure treasury address (multi-sig recommended)
- Verify treasury can receive the protocol's token types
- Consider using a separate treasury per currency

## Security Considerations

### Threat Model

| Threat | Mitigation |
|--------|------------|
| Re-initialization attack | Atomic initialization flag |
| Unauthorized config changes | Admin-only functions with auth |
| Parameter manipulation | Comprehensive validation |
| Front-running | Single-shot initialization |

### Audit Checklist

- [ ] Verify initialization can only happen once
- [ ] Verify all admin functions require authorization
- [ ] Verify parameter validation covers all edge cases
- [ ] Verify events are emitted for all state changes
- [ ] Verify default values are reasonable
- [ ] Verify storage keys don't collide with other modules

## Future Enhancements

1. **Phased Initialization**: Support for multi-phase initialization
2. **Pause/Unpause**: Emergency pause functionality
3. **Upgrade Support**: Migration path for configuration updates
4. **Multi-sig Admin**: Support for multi-signature admin operations
5. **Configuration Proposals**: Time-locked configuration changes

## References

- [Soroban Authorization](https://soroban.stellar.org/docs/fundamentals/authorization)
- [Contract Storage](https://soroban.stellar.org/docs/fundamentals/persisting-data)
- [Events](https://soroban.stellar.org/docs/fundamentals/events)

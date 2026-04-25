# QuickLendX Protocol Initialization

## Overview

The QuickLendX protocol uses a secure, one-time initialization flow that sets up all critical
configuration parameters in a single atomic operation. This document describes the initialization
system, its security model, and usage patterns.

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

---

## Architecture

### Module Structure

```
quicklendx-contracts/src/
├── init.rs                   # Core initialization module
├── admin.rs                  # Admin role management
├── currency.rs               # Currency whitelist
├── test_init.rs              # Comprehensive test suite
├── test_init_debug.rs        # Address / currency edge-case tests
└── test_init_invariants.rs   # Initialization invariants (Issue #833)
```

### Key Components

1. **`ProtocolInitializer`** — main struct providing initialization and configuration management
2. **`InitializationParams`** — struct bundling all initialization parameters
3. **`ProtocolConfig`** — on-chain storage for protocol-wide parameters
4. **Storage Keys** — isolated storage keys for different configuration aspects

---

## Initialization Flow

### Single-Shot Initialization

The protocol supports atomic initialization where all parameters are set in one transaction.
The function is **idempotent**: if called again with the exact same parameters it returns `Ok(())`
without altering state. If called with different parameters after initial setup it reverts with
`OperationNotAllowed`.

```rust
use quicklendx_contracts::init::{InitializationParams, ProtocolInitializer};
use soroban_sdk::{Address, Env, Vec};

fn initialize_protocol(env: &Env) {
    let params = InitializationParams {
        admin: Address::generate(env),
        treasury: Address::generate(env),
        fee_bps: 200,               // 2 %
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604_800, // 7 days
        initial_currencies: Vec::new(env),
    };

    ProtocolInitializer::initialize(env, &params)
        .expect("Initialization failed");
}
```

---

## Security Model

### One-Time Initialization

- The contract can only be initialized **once**.
- Subsequent calls with different parameters fail with `OperationNotAllowed`.
- The initialization flag is stored in instance storage under key `proto_in`.

### Admin Authorization

- Initialization requires `require_auth()` from the admin address.
- Admin address is stored and used for all future privileged operations.

### Address Sanity

- Admin and treasury must be **distinct** addresses.
- Neither admin nor treasury may equal the contract address itself.
- Initial currency list must be unique and must not include admin, treasury, or the contract.

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
| `admin` / `treasury` | Distinct, not the contract address | `InvalidAddress` |
| `initial_currencies` | No duplicates; not admin/treasury/contract | `InvalidCurrency` |
| `fee_bps` | `0 ≤ fee ≤ 1000` | `InvalidFeeBasisPoints` |
| `min_invoice_amount` | `> 0` | `InvalidAmount` |
| `max_due_date_days` | `1 ≤ days ≤ 730` | `InvoiceDueDateInvalid` |
| `grace_period_seconds` | `≤ 2,592,000` (30 days) | `InvalidTimestamp` |

---

## Configuration Parameters

### `InitializationParams`

```rust
#[contracttype]
pub struct InitializationParams {
    pub admin: Address,                    // Protocol admin
    pub treasury: Address,                 // Fee collection address
    pub fee_bps: u32,                      // Fee in basis points (e.g. 200 = 2 %)
    pub min_invoice_amount: i128,          // Minimum invoice amount
    pub max_due_date_days: u64,            // Max days until due date
    pub grace_period_seconds: u64,         // Grace period before default
    pub initial_currencies: Vec<Address>,  // Initial whitelisted currencies
}
```

### `ProtocolConfig`

Stored on-chain after initialization:

```rust
#[contracttype]
pub struct ProtocolConfig {
    pub min_invoice_amount: i128,
    pub max_due_date_days: u64,
    pub grace_period_seconds: u64,
    pub updated_at: u64,
    pub updated_by: Address,
}
```

### Default Values

| Parameter | Default | Description |
|-----------|---------|-------------|
| `fee_bps` | 200 | 2 % platform fee |
| `min_invoice_amount` | 1,000,000 | 1 token (6 decimals) |
| `max_due_date_days` | 365 | 1 year maximum |
| `grace_period_seconds` | 604,800 | 7 days |

---

## API Reference

### `initialize`

```rust
pub fn initialize(env: &Env, params: &InitializationParams) -> Result<(), QuickLendXError>
```

Initializes the protocol. Requires auth from `params.admin`.

**Errors**: `OperationNotAllowed`, `InvalidAddress`, `InvalidCurrency`,
`InvalidFeeBasisPoints`, `InvalidAmount`, `InvoiceDueDateInvalid`, `InvalidTimestamp`

---

### `is_initialized`

```rust
pub fn is_initialized(env: &Env) -> bool
```

Returns `true` after a successful `initialize` call.

---

### `set_protocol_config` *(admin only)*

```rust
pub fn set_protocol_config(
    env: &Env, admin: &Address,
    min_invoice_amount: i128, max_due_date_days: u64, grace_period_seconds: u64,
) -> Result<(), QuickLendXError>
```

Updates protocol configuration. Enforces the same bounds as `initialize`.

---

### `set_fee_config` *(admin only)*

```rust
pub fn set_fee_config(env: &Env, admin: &Address, fee_bps: u32) -> Result<(), QuickLendXError>
```

Updates the platform fee. Valid range: `0–1000`.

---

### `set_treasury` *(admin only)*

```rust
pub fn set_treasury(env: &Env, admin: &Address, treasury: &Address) -> Result<(), QuickLendXError>
```

Updates the treasury address. Treasury must differ from admin.

---

### Query functions

| Function | Returns | Default |
|----------|---------|---------|
| `get_fee_bps` | `u32` | 200 |
| `get_treasury` | `Option<Address>` | `None` |
| `get_min_invoice_amount` | `i128` | 1,000,000 |
| `get_max_due_date_days` | `u64` | 365 |
| `get_grace_period_seconds` | `u64` | 604,800 |
| `get_protocol_config` | `Option<ProtocolConfig>` | `None` |
| `get_version` | `u32` | `PROTOCOL_VERSION` |

---

## Events

| Symbol | Trigger | Payload |
|--------|---------|---------|
| `proto_in` | Successful `initialize` | admin, treasury, fee_bps, min_amount, max_days, grace, ts |
| `proto_cfg` | `set_protocol_config` | admin, min_amount, max_days, grace, ts |
| `fee_cfg` | `set_fee_config` | admin, fee_bps, ts |
| `trsr_upd` | `set_treasury` | admin, treasury, ts |

---

## Error Handling

| Error | Description |
|-------|-------------|
| `OperationNotAllowed` | Contract already initialized |
| `NotAdmin` | Caller is not the admin |
| `InvalidFeeBasisPoints` | `fee_bps` outside `[0, 1000]` |
| `InvalidAmount` | `min_invoice_amount ≤ 0` |
| `InvoiceDueDateInvalid` | `max_due_date_days` outside `[1, 730]` |
| `InvalidTimestamp` | `grace_period_seconds > 2,592,000` |
| `InvalidAddress` | admin == treasury, or either equals contract address |
| `InvalidCurrency` | Duplicate or reserved currency in initial list |

---

## Testing

### Test files

| File | Purpose |
|------|---------|
| `test_init.rs` | Full lifecycle, config updates, query functions, events |
| `test_init_debug.rs` | Address / currency edge cases |
| `test_init_invariants.rs` | **Initialization invariants (Issue #833)** |

### Invariants test suite (`test_init_invariants.rs`)

Added in Issue #833, this suite verifies the four core security invariants:

#### 1. One-time initialization

| Test | What it checks |
|------|---------------|
| `test_init_succeeds_first_call` | Happy path succeeds |
| `test_init_second_call_different_params_fails` | Re-init with different params → `OperationNotAllowed` |
| `test_init_idempotent_same_params` | Re-init with identical params → `Ok(())` |
| `test_is_initialized_flag_lifecycle` | Flag is `false` before, `true` after |
| `test_failed_reinit_preserves_state` | Failed re-init leaves all stored values unchanged |
| `test_multiple_failed_reinits_preserve_state` | Five consecutive failed re-inits, state intact |
| `test_version_written_at_init_and_stable` | Version constant written at init, stable across calls |

#### 2. Admin / treasury distinct

| Test | What it checks |
|------|---------------|
| `test_admin_equals_treasury_rejected` | admin == treasury → `InvalidAddress` |
| `test_admin_is_contract_address_rejected` | admin == contract → `InvalidAddress` |
| `test_treasury_is_contract_address_rejected` | treasury == contract → `InvalidAddress` |
| `test_stored_admin_and_treasury_are_distinct` | Stored values are distinct after init |
| `test_set_treasury_same_as_admin_rejected` | `set_treasury(admin)` → `InvalidAddress` |
| `test_set_treasury_valid_update` | Valid new treasury is accepted and stored |
| `test_set_treasury_non_admin_rejected` | Non-admin → `NotAdmin` |

#### 3. Fee bps bounds

| Test | What it checks |
|------|---------------|
| `test_fee_bps_zero_accepted` | `fee_bps = 0` accepted |
| `test_fee_bps_max_accepted` | `fee_bps = 1000` accepted |
| `test_fee_bps_above_max_rejected` | `fee_bps = 1001` → `InvalidFeeBasisPoints` |
| `test_fee_bps_u32_max_rejected` | `fee_bps = u32::MAX` → `InvalidFeeBasisPoints` |
| `test_fee_bps_midrange_accepted` | `fee_bps = 500` accepted |
| `test_set_fee_config_bounds_enforced` | `set_fee_config` enforces same bounds |
| `test_set_fee_config_non_admin_rejected` | Non-admin → `NotAdmin` |
| `test_set_fee_config_persisted` | Updated fee is immediately readable |

#### 4. Limits configuration bounds

| Test | What it checks |
|------|---------------|
| `test_min_invoice_amount_zero_rejected` | `min_invoice_amount = 0` → `InvalidAmount` |
| `test_min_invoice_amount_negative_rejected` | Negative → `InvalidAmount` |
| `test_min_invoice_amount_one_accepted` | `min_invoice_amount = 1` accepted |
| `test_min_invoice_amount_large_accepted` | Large value accepted |
| `test_max_due_date_days_zero_rejected` | `max_due_date_days = 0` → `InvoiceDueDateInvalid` |
| `test_max_due_date_days_above_max_rejected` | `731` → `InvoiceDueDateInvalid` |
| `test_max_due_date_days_max_accepted` | `730` accepted |
| `test_max_due_date_days_one_accepted` | `1` accepted |
| `test_grace_period_zero_accepted` | `grace_period_seconds = 0` accepted |
| `test_grace_period_max_accepted` | `2,592,000` accepted |
| `test_grace_period_above_max_rejected` | `2,592,001` → `InvalidTimestamp` |
| `test_set_protocol_config_bounds_enforced` | `set_protocol_config` enforces same bounds |
| `test_set_protocol_config_valid_update_atomic` | All three fields updated atomically |
| `test_set_protocol_config_non_admin_rejected` | Non-admin → `NotAdmin` |

#### 5. Currency whitelist invariants

| Test | What it checks |
|------|---------------|
| `test_init_duplicate_currencies_rejected` | Duplicate → `InvalidCurrency` |
| `test_init_currency_equals_admin_rejected` | Currency == admin → `InvalidCurrency` |
| `test_init_currency_equals_treasury_rejected` | Currency == treasury → `InvalidCurrency` |
| `test_init_currency_equals_contract_rejected` | Currency == contract → `InvalidCurrency` |
| `test_init_valid_currencies_accepted` | Two distinct valid currencies accepted |

#### 6. Authorization invariant

| Test | What it checks |
|------|---------------|
| `test_init_requires_admin_auth` | `initialize` without auth panics |

#### 7. Query defaults & post-init values

| Test | What it checks |
|------|---------------|
| `test_query_defaults_before_init` | All getters return safe defaults before init |
| `test_query_values_after_init` | All getters return stored values after init |
| `test_protocol_config_none_before_init` | `get_protocol_config` returns `None` before init |
| `test_protocol_config_some_after_init` | Returns correct `ProtocolConfig` after init |

#### 8. Boundary combinations

| Test | What it checks |
|------|---------------|
| `test_all_params_at_minimum_boundary` | All params at minimum valid values |
| `test_all_params_at_maximum_boundary` | All params at maximum valid values |

#### 9. Admin transfer

| Test | What it checks |
|------|---------------|
| `test_admin_transfer_revokes_old_admin_config_access` | New admin can update; old admin is rejected |

#### 10. Deterministic validation

| Test | What it checks |
|------|---------------|
| `test_validation_is_deterministic` | Same invalid input always returns same error |
| `test_validation_order_fee_before_amount` | `fee_bps` validated before `min_invoice_amount` |

### Running the invariants tests

```bash
cd quicklendx-contracts
cargo test test_init_invariants -- --nocapture
```

### Security assumptions validated

- **No config bypass** — invalid params are always rejected before any state write.
- **Deterministic validation** — the same invalid input always produces the same error code.
- **State immutability after init** — a failed re-init leaves all stored values unchanged.
- **Admin/treasury separation** — the protocol enforces role separation at the storage level.

---

## Best Practices

1. **Check initialization** before dependent operations:
   ```rust
   assert!(ProtocolInitializer::is_initialized(env), "Protocol not initialized");
   ```

2. **Validate params client-side** before calling `initialize`.

3. **Monitor `proto_in` events** for audit purposes.

4. **Use a multi-sig** for the admin address.

5. **Use a separate treasury** per currency if needed.

---

## Security Considerations

| Threat | Mitigation |
|--------|------------|
| Re-initialization attack | Atomic initialization flag |
| Unauthorized config changes | Admin-only functions with `require_auth` |
| Parameter manipulation | Comprehensive pre-write validation |
| Front-running | Single-shot initialization |

---

## References

- [Soroban Authorization](https://soroban.stellar.org/docs/fundamentals/authorization)
- [Contract Storage](https://soroban.stellar.org/docs/fundamentals/persisting-data)
- [Events](https://soroban.stellar.org/docs/fundamentals/events)

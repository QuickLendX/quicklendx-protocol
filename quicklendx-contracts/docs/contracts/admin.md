# Admin Management

The QuickLendX protocol uses a centralized admin role for managing privileged operations, including invoice verification, fee configuration, and system parameters.

## Admin Role

- **Single Admin**: The protocol currently uses a single admin address.
- **One-time Initialization**: The admin address can only be set once during the initial setup of the contract.
- **Role Transfer**: The current admin can transfer the role to a new address at any time.

## Initialization

There are two ways to set up the admin role, but the **recommended** path is the unified protocol initialization flow.

### Recommended: Protocol Initialization

Use the `initialize` function to set up the admin address along with other critical protocol parameters in a single atomic transaction.

```rust
pub fn initialize(env: Env, params: InitializationParams) -> Result<(), QuickLendXError>
```

The `InitializationParams` struct includes:
- `admin`: The initial admin address.
- `treasury`: The address that will receive platform fees.
- `fee_bps`: The initial platform fee in basis points.
- `min_invoice_amount`: Minimum allowed amount for an invoice.
- `max_due_date_days`: Maximum duration for an invoice.
- `grace_period_seconds`: Time before a late invoice is considered defaulted.
- `initial_currencies`: A list of initially whitelisted token addresses.

### Deprecated: Standalone Admin Setup

The `initialize_admin` function is legacy and is primarily kept for backward compatibility and specialized setup scenarios.

> [!WARNING]
> `initialize_admin` is deprecated and will be removed in a future version. Use `initialize` instead to ensure all protocol-wide configuration is correctly set.

```rust
#[deprecated(note = "use 'initialize' for full protocol setup")]
pub fn initialize_admin(env: Env, admin: Address) -> Result<(), QuickLendXError>
```

## Migration Guide

If you are currently using `initialize_admin`, you should migrate to `initialize` to ensure your protocol instance is fully configured.

**Old way:**
```typescript
await client.initialize_admin(admin);
```

**New way:**
```typescript
await client.initialize({
  admin,
  treasury,
  fee_bps: 200, // 2%
  min_invoice_amount: 1000000,
  max_due_date_days: 365,
  grace_period_seconds: 604800,
  initial_currencies: []
});
```

## Guardrails

- **Once-per-Contract**: Regardless of which initialization function is called first, all subsequent calls to either `initialize` or `initialize_admin` (with different parameters) will fail with `OperationNotAllowed`.
- **Authorization**: Initialization requires the explicit authorization of the target admin address (`admin.require_auth()`). This prevents third parties from arbitrarily locking in an admin address.

## Privileged Operations

The admin role is required for the following operations:
- `verify_invoice`: Approve an invoice after off-chain verification.
- `set_platform_fee`: Update the protocol fee percentage.
- `transfer_admin`: Transfer the admin role to a new address.
- `add_currency` / `remove_currency`: Manage the whitelist of allowed settlement tokens.
- `pause` / `unpause`: Emergency control for the contract.
- `initiate_emergency_withdraw`: Emergency recovery of stuck funds.

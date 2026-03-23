# Admin Access Control

This document describes the admin model used by the QuickLendX Soroban contract.

## Design Goals

- Enforce a single canonical admin address in contract storage.
- Allow one-time initialization only.
- Require authenticated transfer by the current admin.
- Guard all privileged entrypoints with consistent checks.
- Provide emergency pause functionality to halt mutating operations while preserving read access.

## Storage Model

Admin state is stored in `src/admin.rs` using instance storage:

- `ADMIN_KEY` (`"admin"`): current admin address.
- `ADMIN_INITIALIZED_KEY` (`"adm_init"`): boolean flag preventing re-initialization.
- `PAUSED_KEY` (`"paused"`): boolean flag for protocol pause state.

`AdminStorage` is the source of truth for admin checks.
`PauseControl` is the source of truth for pause state checks.

## Initialization Rules

`initialize_admin(admin)` and protocol `initialize(...)` both enforce:

- `admin.require_auth()` must succeed.
- Admin can only be set once.
- Re-initialization returns `OperationNotAllowed`.

## Transfer Rules

`transfer_admin(new_admin)` enforces:

- Current admin must already exist.
- Current admin must authenticate (`require_auth`).
- Stored admin must match the authenticated caller.
- Admin is updated atomically and an admin transfer event is emitted.

## Pause Control

The protocol includes an emergency pause mechanism (`src/pause.rs`) that allows the admin to halt all mutating operations while preserving read-only access.

### Pause Behavior

When the protocol is paused (`PauseControl::is_paused(env) == true`):

- **Mutating operations** MUST fail with `QuickLendXError::ContractPaused`
- **Read/getter operations** MUST continue functioning normally
- **Admin operations** (pause/unpause) remain available to the admin

### Protected Operations

All mutating entrypoints check pause state via `PauseControl::require_not_paused(&env)`:

**Invoice Operations:**
- `store_invoice`, `upload_invoice`, `cancel_invoice`
- `verify_invoice`, `update_invoice_status`
- `update_invoice_metadata`, `clear_invoice_metadata`

**Bid Operations:**
- `place_bid`, `cancel_bid`, `withdraw_bid`
- `accept_bid`, `accept_bid_and_fund`
- `cleanup_expired_bids`

**Escrow Operations:**
- `release_escrow_funds`, `refund_escrow_funds`

**Investment Operations:**
- `add_investment_insurance`

**Admin/Protocol Operations:**
- `pause`, `unpause` (these modify pause state but are admin-only)
- `initiate_emergency_withdraw`, `execute_emergency_withdraw`
- `add_currency`, `remove_currency`, `set_currencies`, `clear_currencies`
- `set_platform_fee`, `set_bid_ttl_days`
- All protocol limits configuration methods

### Security Guarantees

The pause mechanism provides the following security guarantees:

1. **Bypass Prevention**: All mutating operations check pause state BEFORE any state modifications
2. **Read Preservation**: Getters and query functions continue operating during pause
3. **Admin Recovery**: Admin can always unpause to restore normal operations
4. **Atomic State**: Pause state is stored in instance storage and checked atomically

### Implementation Pattern

All protected functions follow this pattern:

```rust
pub fn mutating_operation(env: Env, /* args */) -> Result</* return */, QuickLendXError> {
    // 1. Check pause state FIRST
    pause::PauseControl::require_not_paused(&env)?;
    
    // 2. Then perform auth, validation, and state changes
    // ...
}
```

This ordering ensures that pause checks cannot be bypassed by early returns or error conditions.

## Privileged Operations

Privileged methods are guarded by one of two internal checks in `src/lib.rs`:

- `require_current_admin(&Env)`:
  - Loads the stored admin,
  - requires auth from that address,
  - returns the verified admin address.
- `require_specific_admin(&Env, &Address)`:
  - Validates caller address equals stored admin,
  - then requires auth for that exact address.

This is applied to admin-sensitive methods including:

- invoice verification and status mutation,
- platform fee updates and fee-system configuration,
- dispute review/resolution,
- investor verification/rejection and limit management,
- analytics export/update operations,
- revenue distribution controls,
- backup management,
- invoice clearing utilities.

## Backward Compatibility

Legacy `set_admin(...)` remains available for compatibility with existing tests/integrations.

Behavior:

- If admin is uninitialized, it performs authenticated initialization.
- If admin is initialized, it performs authenticated transfer from current admin.
- Legacy verification storage is synchronized after updates for compatibility reads.

## Security Notes

- Privileged wrappers no longer rely on caller-supplied addresses alone.
- Anonymous admin initialization is blocked.
- Admin-only comments now match actual runtime enforcement.
- Legacy compatibility path still preserves single-admin invariants.
- **Pause state is checked before any state mutations to prevent bypass attacks**
- **All pause-protected functions are tested in `src/test_pause.rs`**

## Testing

Pause functionality is tested in `src/test_pause.rs` with comprehensive coverage:

- Mutating operations fail with `ContractPaused` when paused
- Read APIs continue functioning when paused
- Only admin can pause/unpause
- Non-admin cannot pause/unpause
- Bid operations: `place_bid`, `cancel_bid`, `withdraw_bid`, `accept_bid`, `cleanup_expired_bids`
- Escrow operations: `accept_bid_and_fund`, `refund_escrow_funds`, `release_escrow_funds`
- Read APIs: `get_bid`, `get_bids_for_invoice`, `get_bids_by_status`, `get_all_bids_by_investor`, `get_escrow_details`, `get_ranked_bids`, `get_best_bid`

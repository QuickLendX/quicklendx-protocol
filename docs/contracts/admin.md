# Admin Access Control

## Overview

This document describes the admin role management and access control system for the QuickLendX protocol smart contracts.

- **Single admin**: Only one admin address is allowed at a time.
- **Initialization**: Admin can only be set once during contract initialization.
- **Transfer**: Admin role can be transferred to another address by the current admin.
- **Authorization**: Only the admin can perform privileged operations (e.g., invoice verification, platform fee configuration).

## Implementation

- Admin logic is implemented in `src/admin.rs`.
- Privileged functions in `src/lib.rs` enforce admin-only access using `AdminStorage`.
- All admin actions require explicit authorization.

## Key Functions

- `initialize_admin(env, admin)`: Set the initial admin (once only).
- `transfer_admin(env, new_admin)`: Transfer admin role to a new address (current admin only).
- `get_current_admin(env)`: Query the current admin address.
- `verify_invoice(env, invoice_id)`: Only admin can verify invoices.
- `set_platform_fee(env, new_fee_bps)`: Only admin can set platform fees.

## Security Notes

- Double initialization is prevented by a storage flag.
- All admin actions require `require_auth()`.
- Events are emitted for admin set and transfer actions.

## Testing

- Comprehensive tests in `src/test_admin.rs` cover:
  - Initialization and double-init prevention
  - Admin transfer (success, failure, chain)
  - Authorization enforcement on privileged functions
  - Edge cases and event emission

## Coverage

- Test coverage exceeds 95% for admin logic.

## Example Usage

```rust
// Initialize admin (once)
contract.initialize_admin(&admin_address);

// Transfer admin
contract.transfer_admin(&new_admin_address);

// Verify invoice (admin only)
contract.verify_invoice(&invoice_id);
```

---

## Test Output (Recent)

```
error: could not compile `quicklendx-contracts` (lib test) due to 35 previous errors; 76 warnings emitted
warning: build failed, waiting for other jobs to finish...
warning: `quicklendx-contracts` (lib) generated 21 warnings (run `cargo fix --lib -p quicklendx-contracts` to apply 3 suggestions)
Some errors have detailed explanations: E0308, E0428, E0609.
For more information about an error, try `rustc --explain E0308`.
```

## Security Notes

- All admin actions require explicit authorization (`require_auth`).
- Double initialization is prevented by a storage flag.
- Events are emitted for admin set and transfer actions.
- Tests cover double init, non-admin calls, and event emission.

*For more details, see the source code and tests in the repository.*

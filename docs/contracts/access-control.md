# Access Control

QuickLendX uses an explicit single-admin role model for protocol configuration and emergency recovery.

## Role Model

- `admin` is the only privileged governance role.
- Admin-only entrypoints require both conditions:
  - the supplied or stored admin address matches the current on-chain admin
  - that same admin address authorizes the invocation
- Failed authorization must leave protocol state unchanged.

## Protected Flows

The contract has comprehensive coverage for unauthorized callers attempting to modify:

- admin rotation through legacy and primary admin setters
- protocol configuration, fee basis points, and treasury settings during initialization/configuration flows
- protocol limits and max-invoices policy
- pause and unpause state
- emergency withdraw initiation, execution, and cancellation
- fee-system initialization and treasury routing configuration
- currency whitelist management (add, remove, set, clear)
- bid TTL and active bid limits configuration
- backup operations (create, restore, archive, cleanup)
- protocol limits configuration
- vesting schedule creation
- business/investor verification operations

## Access Control Matrix

The following table documents all admin-gated methods and their access control behavior:

| Method Category | Method | Auth Required | Non-Admin Error | Edge Cases |
|-----------------|--------|---------------|-----------------|------------|
| **AdminStorage** | `initialize` | Caller (self-auth) | `OperationNotAllowed` | One-time only |
| | `transfer_admin` | Current Admin | `NotAdmin` | Self-transfer blocked |
| | `initiate_admin_transfer` | Current Admin | `NotAdmin` | Requires initialized state |
| | `accept_admin_transfer` | Pending Admin | `Unauthorized` | Only pending admin accepted |
| | `cancel_admin_transfer` | Current Admin | `NotAdmin` | Clears pending state |
| | `set_two_step_enabled` | Current Admin | `NotAdmin` | Toggle two-step mode |
| **Protocol Initializer** | `set_protocol_config` | Admin + Auth | `NotAdmin` | Validates params |
| | `set_fee_config` | Admin + Auth | `NotAdmin` | Fee bps range check |
| | `set_treasury` | Admin + Auth | `NotAdmin` | Cannot be admin address |
| **Pause Control** | `set_paused` | Admin + Auth | `NotAdmin` | Both pause and unpause |
| **Emergency Withdraw** | `initiate` | Admin + Auth | `NotAdmin` | Timelock applies |
| | `execute` | Admin + Auth | `NotAdmin` | After timelock, before expiration |
| | `cancel` | Admin + Auth | `NotAdmin` | Marks nonce cancelled |
| **Currency Whitelist** | `add_currency` | Admin + Auth | `NotAdmin` | Idempotent |
| | `remove_currency` | Admin + Auth | `NotAdmin` | No-op if absent |
| | `set_currencies` | Admin + Auth | `NotAdmin` | Atomic replacement |
| | `clear_currencies` | Admin + Auth | `NotAdmin` | Empty = allow-all |
| **Bid Configuration** | `set_bid_ttl_days` | Admin + Auth | `NotAdmin` | Bounds: 1..=30 |
| | `set_max_active_bids_per_investor` | Admin + Auth | `NotAdmin` | 0 = disabled |
| | `reset_bid_ttl_to_default` | Admin + Auth | `NotAdmin` | Resets to 7 days |
| **Protocol Limits** | `set_protocol_limits` | Admin + Auth | `NotAdmin` | Validates all params |
| | `initialize_protocol_limits` | Admin + Auth | `NotAdmin` | One-time init |
| | `update_protocol_limits` | Admin + Auth | `NotAdmin` | Partial update |
| | `update_limits_max_invoices` | Admin + Auth | `NotAdmin` | With max invoices |
| **Backup** | `create_backup` | Admin + Auth | `NotAdmin` | Requires not paused |
| | `restore_backup` | Admin + Auth | `NotAdmin` | Validates before restore |
| | `archive_backup` | Admin + Auth | `NotAdmin` | Marks as Archived |
| | `cleanup_backups` | Admin + Auth | `NotAdmin` | Based on retention policy |
| | `set_backup_retention_policy` | Admin + Auth | `NotAdmin` | Max age and count limits |
| **Vesting** | `create_vesting_schedule` | Admin + Auth | `NotAdmin` | Requires not paused |
| **Fee Management** | `initialize_fee_system` | Admin + Auth | `NotAdmin` | One-time init |
| | `configure_treasury` | Admin + Auth | `NotAdmin` | Fee routing config |
| | `update_platform_fee_bps` | Admin + Auth | `NotAdmin` | Fee basis points |
| | `update_fee_structure` | Admin + Auth | `NotAdmin` | Fee type config |
| | `configure_revenue_distribution` | Admin + Auth | `NotAdmin` | Revenue split |
| | `distribute_revenue` | Admin + Auth | `NotAdmin` | Period distribution |
| | `set_platform_fee` | Admin + Auth | `NotAdmin` | Platform fee config |
| **Verification** | `verify_business` | Admin + Auth | `NotAdmin` | Business KYC |
| | `reject_business` | Admin + Auth | `NotAdmin` | Business rejection |
| | `verify_investor` | Admin + Auth | `NotAdmin` | Investor KYC |
| | `reject_investor` | Admin + Auth | `NotAdmin` | Investor rejection |
| | `set_investment_limit` | Admin + Auth | `NotAdmin` | Investment cap |
| **Admin Management** | `set_admin` | Current Admin | `NotAdmin` | Admin transfer |

## Security Assumptions

- Identity checks without signer authorization are insufficient for admin-only flows.
- Facade methods that derive the admin from storage still require the stored admin signature.
- Public helper modules must enforce the same role model as top-level contract entrypoints so they remain safe if reused internally.
- Non-admin callers are rejected with `NotAdmin` error code (1103) consistently across all entrypoints.
- State is immutable after a rejected access control check - no partial state changes occur.

## Edge Cases Covered

The comprehensive access-control matrix tests (`src/test_admin.rs`) cover:

1. **Pre-initialization state**: All admin methods return `OperationNotAllowed` before admin is set
2. **Admin transfer**: Former admin is rejected, new admin is accepted immediately
3. **Revoked caller**: After admin transfer, the former admin cannot perform any admin operations
4. **Self-transfer prevention**: Admin cannot transfer to themselves
5. **Two-step transfer flow**: Proper authentication for initiate/accept/cancel flow
6. **State immutability**: Verified that rejected calls leave protocol state unchanged
7. **Partial auth prevention**: Address matching without proper authentication is insufficient
8. **Consistency across modules**: All admin-gated methods use the same check pattern

## Test Coverage

The comprehensive access-control tests live in:

- `quicklendx-contracts/src/test_admin.rs` - Full access-control matrix with 60+ test cases

Test modules:
- `test_admin` - Basic admin transfer safety tests (existing)
- `access_control_matrix` - Core access control tests for all admin-gated methods
- `access_control_matrix_extended` - Extended tests for fee, verification, vesting methods

These tests assert both rejection behavior and state immutability after rejected calls.

## Privilege Escalation Prevention

To prevent privilege escalation regressions:

1. Every new admin-gated method must be added to the access control matrix
2. Tests must cover both non-admin rejection and admin acceptance
3. Edge cases must include pre-init, transferred, and revoked scenarios
4. Documentation must be updated when new methods are added

### Checklist for New Admin Methods

- [ ] Add method to access control matrix documentation
- [ ] Add test: non-admin caller is rejected with `NotAdmin`
- [ ] Add test: admin caller is accepted
- [ ] Add test: pre-initialization state rejection
- [ ] Add test: revoked caller rejection (after admin transfer)
- [ ] Add test: state immutability after rejection
- [ ] Update `docs/contracts/access-control.md` with the new method

## Running Tests

To validate the access control implementation:

```bash
cd quicklendx-contracts
cargo test test_admin
```

This will run all admin-related tests including the comprehensive access control matrix.
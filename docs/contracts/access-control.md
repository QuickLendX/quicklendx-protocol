# Access Control

QuickLendX uses an explicit single-admin role model for protocol configuration and emergency recovery.

## Role Model

- `admin` is the only privileged governance role.
- Admin-only entrypoints require both conditions:
  - the supplied or stored admin address matches the current on-chain admin
  - that same admin address authorizes the invocation
- Failed authorization must leave protocol state unchanged.

## Protected Flows

The contract now has negative-path coverage for unauthorized callers attempting to modify:

- admin rotation through legacy and primary admin setters
- protocol configuration, fee basis points, and treasury settings during initialization/configuration flows
- protocol limits and max-invoices policy
- pause and unpause state
- emergency withdraw initiation, execution, and cancellation
- fee-system initialization and treasury routing configuration

## Security Assumptions

- Identity checks without signer authorization are insufficient for admin-only flows.
- Facade methods that derive the admin from storage still require the stored admin signature.
- Public helper modules must enforce the same role model as top-level contract entrypoints so they remain safe if reused internally.

## Test Coverage

The negative access-control tests live in:

- `quicklendx-contracts/src/test_admin.rs`
- `quicklendx-contracts/src/test_init.rs`
- `quicklendx-contracts/src/test_pause.rs`
- `quicklendx-contracts/src/test_emergency_withdraw.rs`
- `quicklendx-contracts/src/test_protocol_limits.rs`
- `quicklendx-contracts/src/test_fees.rs`

These tests assert both rejection behavior and state immutability after rejected calls.

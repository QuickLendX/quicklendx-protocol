# Invoice Lifecycle Documentation

## Overview

This document describes the complete lifecycle of invoices and investments in the QuickLendX protocol, focusing on terminal state transitions and their side effects.

## Investment Terminal States

### Investment Status Enum

The `InvestmentStatus` enum defines the possible states of an investment:

```rust
pub enum InvestmentStatus {
    Active,     // Investment is currently active and funded
    Withdrawn,  // Investment funds were withdrawn (terminal)
    Completed,  // Investment completed successfully (terminal)
    Defaulted,  // Investment defaulted due to non-payment (terminal)
    Refunded,   // Investment was refunded due to cancellation (terminal)
}
```

### Valid State Transitions

| From State | To State(s) | Trigger | Side Effects |
|------------|-------------|---------|--------------|
| **Active** | Completed | Full invoice settlement | - Investment marked as completed<br>- Removed from active index<br>- Settlement events emitted |
| **Active** | Withdrawn | Investment withdrawal | - Investment marked as withdrawn<br>- Removed from active index<br>- Escrow refunded if applicable |
| **Active** | Defaulted | Invoice default | - Investment marked as defaulted<br>- Removed from active index<br>- Default events emitted |
| **Active** | Refunded | Invoice cancellation | - Investment marked as refunded<br>- Removed from active index<br>- Escrow refunded |
| **Withdrawn** | *(none)* | - | Terminal state - no further transitions |
| **Completed** | *(none)* | - | Terminal state - no further transitions |
| **Defaulted** | *(none)* | - | Terminal state - no further transitions |
| **Refunded** | *(none)* | - | Terminal state - no further transitions |

### Terminal State Invariants

Once an investment reaches a terminal state, the following invariants are maintained:

1. **Immutability**: Terminal states cannot transition to any other state
2. **Active Index Cleanup**: Terminal investments are removed from the active investment index
3. **Storage Persistence**: Investment records remain in storage for audit purposes
4. **Event Emission**: Appropriate events are emitted for terminal transitions

## Invoice-Investment Relationship

### Invoice States and Investment Impact

| Invoice State | Investment Impact | Allowed Transitions |
|---------------|------------------|---------------------|
| **Pending** | No investment | → Verified, Cancelled |
| **Verified** | No investment | → Funded, Cancelled |
| **Funded** | Investment Active | → Paid, Defaulted, Refunded |
| **Paid** | Investment Completed | *(terminal)* |
| **Defaulted** | Investment Defaulted | *(terminal)* |
| **Cancelled** | Investment Refunded | *(terminal)* |
| **Refunded** | Investment Refunded | *(terminal)* |

### Bid Acceptance and Investment Creation

When a bid is accepted:
1. Escrow is created and funded
2. Investment record is created with `Active` status
3. Investment is added to active index
4. Invoice state transitions to `Funded`

### Settlement Flow

During invoice settlement:
1. Payment processing validates sufficient funds
2. Escrow is released to business (if held)
3. Investor receives expected return
4. Platform fees are calculated and routed
5. Investment transitions to `Completed`
6. Invoice transitions to `Paid`

## Security Considerations

### State Transition Validation

All state transitions are validated through `InvestmentStatus::validate_transition()`:

```rust
pub fn validate_transition(
    from: &InvestmentStatus,
    to: &InvestmentStatus,
) -> Result<(), QuickLendXError>
```

This function ensures:
- Only valid transitions are allowed
- Terminal states cannot be changed
- Active state can only transition to terminal states

### Active Investment Index

The protocol maintains an index of all active investments to:
- Enable efficient queries for active investments
- Prevent orphaned active investments
- Support investment limit calculations

### Storage Invariants

The `validate_no_orphan_investments()` function ensures:
- All investments in the active index have `Active` status
- No terminal investments remain in the active index
- Index consistency is maintained

## Event Emission

### Terminal State Events

Each terminal transition emits specific events:

- **Completed**: `inv_setlf` (invoice settled final)
- **Withdrawn**: `esc_ref` (escrow refunded)
- **Defaulted**: Default handling events
- **Refunded**: `esc_ref` (escrow refunded)

### Audit Trail

All state transitions are logged with:
- Timestamp
- Previous and new states
- Associated entities (invoice, investment, investor)
- Authorization information

## Testing Coverage

### Test Suite

The `test_investment_terminal_states.rs` module provides comprehensive testing:

1. **Completion Flow**: Tests Active → Completed transition
2. **Withdrawal Flow**: Tests Active → Withdrawn transition
3. **Default Flow**: Tests Active → Defaulted transition
4. **Refund Flow**: Tests Active → Refunded transition
5. **Invalid Transitions**: Tests rejection of invalid state changes
6. **State Immutability**: Tests terminal state preservation
7. **Storage Invariants**: Tests index consistency and cleanup

### Coverage Requirements

- **95% test coverage** for investment state transitions
- **All terminal states** tested with positive and negative cases
- **Storage invariants** validated after each transition
- **Event emission** verified for all transitions
- **Error conditions** tested for invalid transitions

## Implementation Notes

### NatSpec Documentation

All public functions include NatSpec-style comments:

```rust
/// @notice Validates investment state transition
/// @dev Ensures only valid transitions are allowed
/// @param from Current investment status
/// @param to Target investment status
/// @return Success if transition is valid
/// @error InvalidStatus if transition is not allowed
pub fn validate_transition(/* ... */) -> Result<(), QuickLendXError>
```

### Error Handling

- `InvalidStatus`: Returned for invalid state transitions
- `StorageKeyNotFound`: Returned when investment doesn't exist
- `OperationNotAllowed`: Returned for unauthorized operations

### Gas Optimization

- Active index cleanup happens during state transitions
- Storage updates are batched when possible
- Event emission is minimized for gas efficiency

## Migration Considerations

### Backward Compatibility

- Existing investment records are preserved
- State transition logic is additive
- No breaking changes to public interfaces

### Upgrade Path

- New terminal states can be added via enum extension
- Transition validation can be enhanced without breaking changes
- Storage format remains stable

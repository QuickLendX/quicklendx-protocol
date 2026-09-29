# Notification Idempotency and Replay Protection

## Overview

The QuickLendX notification system implements **idempotency keys** to ensure one-shot delivery semantics even when transactions are replayed or resubmitted. This document describes the design, implementation, and testing of this replay-safe notification model.

## Problem Statement

Without idempotency protection, a notification could be emitted multiple times if:
1. A transaction is resubmitted due to network issues
2. A contract is upgraded and replayed events are re-indexed
3. An indexer crashes and replays historical events

This could result in duplicate notifications being delivered to users, degrading the user experience and potentially causing confusion or alarm.

## Solution: Idempotency Keys

### Key Derivation

Each notification is assigned a deterministic **idempotency key** derived from:

```
idempotency_key = keccak256(event_kind || target_id || ledger_seq || nonce)
```

Where:
- **event_kind**: The notification type (encoded as a single byte discriminant)
  - `0x00` = InvoiceCreated
  - `0x01` = InvoiceVerified
  - `0x02` = InvoiceStatusChanged
  - `0x03` = BidReceived
  - `0x04` = BidAccepted
  - `0x05` = PaymentReceived
  - `0x06` = PaymentOverdue
  - `0x07` = InvoiceDefaulted
  - `0x08` = SystemAlert
  - `0x09` = General

- **target_id**: The recipient address (encoded as XDR bytes)
- **ledger_seq**: The Stellar ledger sequence number at the time of notification
- **nonce**: A unique nonce (typically the ledger timestamp)

### Collision Resistance

The key derivation uses **keccak256**, which provides:
- **256-bit security** against collision attacks
- **Deterministic output** for the same inputs
- **Avalanche effect**: Small changes in input produce completely different outputs

The combination of four independent parameters (event_kind, target_id, ledger_seq, nonce) ensures uniqueness across all notification scenarios.

### Stability Across Versions

The key derivation is stable because it uses only:
1. Fundamental protocol data (notification type, recipient address)
2. Ledger metadata (sequence number, timestamp)
3. Standard cryptographic primitives (keccak256)

This means:
- Keys remain valid across contract upgrades
- Keys are reproducible by external systems (indexers, auditors)
- Keys do not depend on implementation details that might change

## Implementation Details

### Storage Model

Idempotency keys are tracked using a **bloom-resistant set** stored in contract storage:

```rust
// Per-key tracking
DataKey::IdempotencyKey(BytesN<32>) -> bool

// Set size tracking (for pruning)
DataKey::IdempotencyKeySet -> Vec<BytesN<32>>
```

The set maintains up to `MAX_IDEMPOTENCY_KEYS` (10,000) keys. When the limit is reached, the oldest entries are pruned to prevent unbounded storage growth.

### Duplicate Detection

When `create_notification` is called:

1. **Derive the idempotency key** from the notification parameters
2. **Check if the key exists** in the bloom-resistant set
3. **If found**: Return `NotificationDuplicate` error (no event emitted)
4. **If not found**: Record the key, store the notification, and emit the event

### Error Handling

A new error variant is added to the error enum:

```rust
pub enum QuickLendXError {
    // ...
    NotificationDuplicate = 2002,
}
```

This error is returned when a duplicate notification is detected, allowing callers to distinguish between:
- `NotificationBlocked`: User opted out of this notification type
- `NotificationDuplicate`: Notification was already emitted (replay detected)

## Interplay with Indexer Deduplication

The backend indexer maintains a **database-level UNIQUE constraint** on `(event_id, user_id)`. This provides:

1. **Hard idempotency** at the database layer
2. **Automatic deduplication** of duplicate events
3. **Crash-safe semantics** via atomic batch commits

The contract-level idempotency key provides:

1. **Immediate rejection** at the contract level (no event emission)
2. **Deterministic key derivation** that survives contract upgrades
3. **Replay protection** for the same logical notification event

Together, these two layers ensure:
- **Contract layer**: Prevents duplicate events from being emitted
- **Database layer**: Prevents duplicate notifications from being stored
- **End-to-end**: One-shot delivery semantics across the entire system

## Testing

### Test Coverage

The test suite includes 95%+ coverage of idempotency scenarios:

#### Determinism Tests
- `test_idempotency_key_derivation_is_deterministic`: Keys are stable across versions
- `test_notif_payload_fields_are_deterministic`: Payloads are consistent

#### Replay Protection Tests
- `test_replay_same_notification_is_rejected`: Exact replays are rejected
- `test_replay_rejection_across_all_notification_kinds`: All types are protected
- `test_replay_protection_per_notification_type`: Per-type tracking works

#### Edge Case Tests
- `test_different_recipients_not_rejected_as_duplicates`: Different targets are distinct
- `test_different_ledger_sequences_produce_different_keys`: Ledger height matters
- `test_blocked_notification_does_not_consume_idempotency_key`: Blocked notifications don't consume keys

#### Storage Tests
- `test_idempotency_key_stored_in_notification`: Keys are persisted
- `test_replay_same_notification_is_rejected`: No duplicate events emitted

### Running Tests

```bash
# Run all notification tests
cargo test test_notifications

# Run only idempotency tests
cargo test test_replay
cargo test test_idempotency

# Run with coverage
cargo tarpaulin --out Html --exclude-files tests/
```

### Expected Results

All tests should pass with 95%+ coverage of the notification system:

```
test test_single_notif_event_per_creation ... ok
test test_n_notifications_emit_n_events ... ok
test test_blocked_notification_emits_no_event_on_retry ... ok
test test_preference_update_emits_one_event_per_call ... ok
test test_notif_payload_contains_no_sensitive_strings ... ok
test test_status_event_payload_contains_no_pii ... ok
test test_pref_up_payload_contains_only_address ... ok
test test_each_status_transition_emits_one_event ... ok
test test_failed_status_emits_one_event ... ok
test test_all_types_blocked_emits_no_events ... ok
test test_below_minimum_priority_emits_no_event ... ok
test test_at_minimum_priority_emits_event ... ok
test test_notification_ids_differ_across_timestamps ... ok
test test_read_only_queries_emit_no_events ... ok
test test_status_update_on_missing_notification_emits_no_event ... ok
test test_notif_payload_fields_are_deterministic ... ok
test test_idempotency_key_derivation_is_deterministic ... ok
test test_replay_same_notification_is_rejected ... ok
test test_replay_rejection_across_all_notification_kinds ... ok
test test_different_recipients_not_rejected_as_duplicates ... ok
test test_different_ledger_sequences_produce_different_keys ... ok
test test_replay_protection_per_notification_type ... ok
test test_idempotency_key_stored_in_notification ... ok
test test_blocked_notification_does_not_consume_idempotency_key ... ok
```

## Security Considerations

### Collision Attacks

The use of keccak256 provides 256-bit security against collision attacks. An attacker would need to:
1. Find two different (event_kind, target_id, ledger_seq, nonce) tuples
2. That hash to the same value
3. This is computationally infeasible (2^128 operations expected)

### Replay Attacks

The idempotency key prevents replay attacks by:
1. Deriving a unique key from the notification parameters
2. Storing the key in contract storage
3. Rejecting any notification with a previously-seen key

This ensures that even if a transaction is resubmitted, the notification is only emitted once.

### Storage Exhaustion

The bloom-resistant set is bounded to `MAX_IDEMPOTENCY_KEYS` (10,000) entries. This prevents:
1. Unbounded storage growth
2. Denial-of-service attacks via storage exhaustion
3. Performance degradation over time

When the limit is reached, the oldest entries are pruned using a FIFO strategy.

## Migration and Upgrade

### Backward Compatibility

The idempotency key is added as a new field to the `Notification` struct. Existing notifications stored before this upgrade will not have an idempotency key.

To handle this:
1. New notifications always include an idempotency key
2. Old notifications are not affected (they remain in storage)
3. The idempotency check only applies to new notifications

### Contract Upgrade Path

When upgrading the contract:
1. Deploy the new contract code with idempotency support
2. Existing notifications remain unchanged
3. New notifications use the idempotency key
4. The indexer continues to use its database-level deduplication

## Performance Implications

### Storage Cost

Each idempotency key entry costs:
- 32 bytes for the key (BytesN<32>)
- 1 byte for the value (bool)
- ~50 bytes overhead (Soroban storage encoding)
- **Total: ~83 bytes per entry**

With `MAX_IDEMPOTENCY_KEYS = 10,000`, the maximum storage is ~830 KB.

### Computation Cost

Key derivation adds:
- 1 XDR encoding of the recipient address (~100 bytes)
- 1 keccak256 hash (~1 microsecond)
- 1 storage lookup (~10 microseconds)
- **Total: ~11 microseconds per notification**

This is negligible compared to the overall transaction cost.

## Future Improvements

### Adaptive Pruning

Instead of FIFO pruning, implement adaptive pruning based on:
- Ledger age (prune keys older than N ledgers)
- Access patterns (keep frequently-accessed keys)
- Storage pressure (prune more aggressively when storage is full)

### Distributed Idempotency

For multi-contract systems, consider:
- Shared idempotency key registry
- Cross-contract deduplication
- Coordinated pruning strategies

### Audit Trail

Maintain an audit trail of:
- Duplicate detection events
- Pruning operations
- Storage usage over time

## Wired triggers

The following contract entrypoints now emit lifecycle notifications. Notification
failures are isolated (`let _ = ...`) and never roll back fund-moving state.

| Entrypoint | Helper | `NotificationType` | Recipients |
|---|---|---|---|
| `escrow::accept_bid_and_fund` | `notify_bid_accepted` | `BidAccepted` | Investor |
| `settlement::process_partial_payment` | `notify_payment_received` | `PaymentReceived` | Business, investor |
| `settlement::settle_invoice_internal` | `notify_invoice_status_changed` | `InvoiceStatusChanged` | Business, investor |
| `defaults::handle_default` | `notify_invoice_defaulted` | `InvoiceDefaulted` | Business, investor |
| `QuickLendXContract::create_dispute` | `notify_dispute_opened` | `SystemAlert` | Business, investor |
| `QuickLendXContract::resolve_dispute` | `notify_dispute_resolved` | `SystemAlert` | Business, investor |

Preference filtering and idempotency keys remain enforced inside `create_notification`.

## References

- [Soroban SDK Documentation](https://docs.rs/soroban-sdk/)
- [Keccak-256 Specification](https://keccak.team/keccak_specs_summary.html)
- [Stellar Ledger Sequence](https://developers.stellar.org/docs/learn/concepts/ledger)
- [Backend Notifications Documentation](../backend/docs/notifications.md)
- [Indexer Deduplication](../backend/docs/indexer.md)

# Invoice and Investment Storage Consistency

This document outlines the architecture and security measures implemented to ensure consistency between invoices and their corresponding investments in the QuickLendX protocol.

## Storage Architecture

The protocol uses a dual-index strategy for linking invoices and investments:

1.  **Primary Storage**: Core entities (Invoice, Investment, Bid) are stored using their unique 32-byte IDs as keys.
2.  **Mapping Indexes**: Secondary indexes are used for efficient lookups, such as finding an investment by its associated `invoice_id`.

### Storage Types
- **Invoices & Investments**: Stored in `instance()` or `persistent()` storage based on their expected lifecycle.
- **Indexes**: Stored in `instance()` storage for rapid access during protocol operations.

## Consistency Guarantees

### 1. Back-pointer Validation (Hardening)
To prevent "stale mapping pointers" (where an index points to an entity that no longer exists or belongs to a different parent), the `get_invoice_investment` function performs a mandatory back-pointer check:

```rust
// In src/investment.rs
pub fn get_investment_by_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<Investment> {
    let index_key = Self::invoice_index_key(invoice_id);
    let investment_id: Option<BytesN<32>> = env.storage().instance().get(&index_key);
    investment_id
        .and_then(|id| Self::get_investment(env, &id))
        .filter(|inv| inv.invoice_id == *invoice_id) // Mandatory consistency check
}
```

This filter ensures that even if a mapping index is corrupted or stale, the protocol will not return an incorrect investment record.

### 2. Unified Storage Cleanup
The `StorageManager::clear_all_mappings` function provides a centralized way to reset protocol state, ensuring that when invoices are cleared (e.g., during a backup restore or system reset), all associated mapping counters and pointers are also invalidated.

This is integrated into the `clear_all_invoices` administrative function.

## Security Assumptions
- **ID Uniqueness**: Invoice and Investment IDs are generated using a combination of ledger timestamp and monotonic counters to ensure 32-byte uniqueness.
- **Authorization**: All state-changing operations require appropriate `Address::require_auth()` checks.
- **Migration Safety**: The back-pointer check provides a safety net during contract upgrades where storage layouts or indexing strategies might change.

## Verification
Consistency is verified through:
- **Unit Tests**: `src/test_investment_queries.rs` simulates stale pointer scenarios.
- **Consistency Tests**: `src/test_investment_consistency.rs` verifies mapping integrity across lifecycle events.

# QuickLendX Storage & Indexing Strategy

This document describes the storage layout, indexing strategy, and integrity audit mechanisms used in the QuickLendX smart contracts.

## Storage Layout

QuickLendX uses Soroban's persistent storage to manage core entities:

- **Invoices**: Stored with a `DataKey::Invoice(id)`.
- **Bids**: Stored with a `DataKey::Bid(id)`.
- **Investments**: Stored with an internal key mapping to `Investment` records.

## Secondary Indexes

To support efficient queries, the protocol maintains several secondary indexes using `soroban_sdk::Vec`:

### Invoices
- **Status Index**: Invoices are grouped by their `InvoiceStatus` (Pending, Verified, Funded, etc.).
- **Business Index**: Lists all invoice IDs owned by a specific business address.
- **Metadata Indexes**: Optional indexes for Customer Name and Tax ID to support search.

### Bids
- **Global Index**: A list of all bid IDs in the protocol.
- **Invoice Bid Index**: Lists all bids placed on a specific invoice.
- **Investor Bid Index**: Lists all bids placed by a specific investor.

### Investments
- **Active Index**: Lists all currently `Active` investment IDs.
- **Investor Investment Index**: Lists all investments made by an investor.
- **Invoice Mapping**: Maps an invoice ID directly to its associated investment ID.

## Storage Integrity Audit

The `StorageIntegrityAudit` helper provides a mechanism to verify that all indexes are consistent and free of orphans.

### Audit Mechanisms

1. **Orphan Detection**: Iterates through every index and verifies that the corresponding primary record exists in storage.
2. **Status Consistency**: Verifies that a record's current status matches the index bucket it is placed in.
3. **Cross-Index Validation**: Ensures that if a record exists, it is present in all relevant indexes (e.g., an invoice must be in both its status index and its business index).
4. **Global Sync**: Uses source-of-truth lists (like the global bid list) to verify secondary mapping consistency.

### Security Assumptions

- **Index Poisoning Prevention**: All index updates are performed within the same atomic transaction as the record update.
- **Cleanup Bounds**: Index cleanup (like expired bids) is performed periodically to prevent unbounded list growth and ensure storage efficiency.
- **Bounded Lists**: Protocol limits (e.g., `MAX_BIDS_PER_INVOICE`) ensure that index lists remain within Soroban's resource limits.

## Usage in Tests

Integrity audits are integrated into the test suite to ensure that every major state transition (upload, bid, accept, settle, refund, cleanup) leaves the storage in a consistent state.

```rust
StorageIntegrityAudit::audit_all(&env).expect("Storage must be consistent");
```

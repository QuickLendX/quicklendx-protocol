# QuickLendX Off-Chain Indexer Contract Reliance

This document outlines what the off-chain indexer relies on from the on-chain Soroban smart contracts. It is intended for **downstream integrators** and **indexer operators** to understand the exact events, topics, data structures, and state mutations that the indexer expects.

## Event Subscriptions

The indexer relies on specific, stable topics emitted by the contract to track state changes. The contract guarantees that the primary topic for each event is stable and defined as a constant. 

### Topic Constants

The indexer should subscribe to the following topic strings (which correspond to `Symbol`s in Soroban):

- `invoice_uploaded`: Emitted when an invoice is created.
- `invoice_settled`: Emitted when an invoice is fully settled (loan repaid).
- `bid_placed`: Emitted when a new bid is placed on an invoice.
- `bid_accepted`: Emitted when a bid is accepted.
- `escrow_created`: Emitted when investor funds are locked.
- `dispute_created`: Emitted when a dispute is opened.

### Concrete Example: `InvoiceUploaded` Event

When a new invoice is uploaded, the contract emits an `InvoiceUploaded` event. The indexer relies on this to populate its database of available invoices.

**Topic (Indexer view):**
`"invoice_uploaded"` (Symbol)

**Data Payload (Indexer view):**
The indexer decodes the data payload into the following struct:

```rust
pub struct InvoiceUploaded {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub amount: i128,
    pub currency: Address,
    pub due_date: u64,
    pub timestamp: u64,
}
```

**Real-world Request/Output:**
When a business uploads an invoice, the contract publishes:
```rust
env.events().publish(
    (symbol_short!("invoice_uploaded"),), // Topic
    InvoiceUploaded {
        invoice_id: <32-byte-id>,
        business: <address>,
        amount: 10000000,
        currency: <address>,
        due_date: 1730000000,
        timestamp: 1720000000,
    }
);
```
The indexer captures this via the Soroban RPC, decodes the payload, and inserts a new row in the `invoices` table.

## Data Structure Invariants

The indexer relies on the following structural invariants:
1. **No PII**: Events like `InvoiceSettled` or `DisputeCreated` do not include any Personally Identifiable Information (PII) to comply with data protection regulations. The indexer never expects `customer_name` or `tax_id` from the contract.
2. **Stable Identifiers**: `invoice_id`, `bid_id`, and `escrow_id` are 32-byte arrays (`BytesN<32>`) and serve as primary keys in the indexer database.
3. **Amounts**: All monetary amounts (`amount`, `total_paid`, `bid_amount`, `platform_fee`) are represented as `i128` integers in the smallest currency unit.
4. **Timestamps**: All dates and timestamps (`timestamp`, `due_date`, `expiration_timestamp`) are `u64` Unix epoch timestamps.

## Storage Keys & Queries

The indexer primarily uses events for state transitions to avoid polling. However, it periodically issues queries to verify its state against the contract's current state. The indexer relies on entrypoints like `get_invoice(id)` and `get_bid(id)` to return exactly matching representations of the structs it constructed from events.

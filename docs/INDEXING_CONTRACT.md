# Indexing Contract Reference Guide

This document defines the contract-level schemas, event emission formats, storage layouts, and data structures that off-chain indexers and dashboards rely on from the QuickLendX Soroban smart contracts. 

Any change to the structures, names, or values documented here is a **breaking change** for the indexer and must be managed via coordinated migrations.

---

## 1. Target Audience

This guide is intended for:
- **Smart Contract Contributors:** Developers making modifications to the Soroban contracts who need to ensure their changes do not break off-chain tracking.
- **Indexer/Backend Contributors:** Developers building and maintaining the ingestion pipelines, GraphQL endpoints, and caching layers for QuickLendX.

---

## 2. Event Emission & Topics

QuickLendX smart contracts publish structured events to the ledger using Soroban's event system:
```rust
env.events().publish(topics, data);
```

### Event Topics Schema
In Soroban, events are published with a vector of topics and a single data value.
- **`topic_0` (First Topic):** Represents the event type name. In QuickLendX, this is either the snake_case name of the event struct (e.g. `invoice_uploaded`) or a pinned symbol (e.g. `adm_trf`).
- **`topic_1`..`topic_n`:** Optional routing/filtering keys such as invoice ID or address.
- **Event Data:** The serialized event struct containing the payload.

### Pinned Event Topics List

The following event topics are pinned as compile-time string constants in [src/events.rs](file:///c:/Users/hp/Desktop/quicklendx-protocol/quicklendx-contracts/src/events.rs). **Do not rename or change these topics without a major version upgrade.**

| Event Name | `topic_0` String | Description | Emitter |
|---|---|---|---|
| `InvoiceUploaded` | `"invoice_uploaded"` | A new invoice has been uploaded | `store_invoice` |
| `InvoiceVerified` | `"invoice_verified"` | Admin has verified the invoice | `verify_invoice` |
| `InvoiceCancelled` | `"invoice_cancelled"` | Business has cancelled the invoice | `cancel_invoice` |
| `InvoiceFunded` | `"invoice_funded"` | Invoice bid was accepted & funded | `accept_bid_and_fund` |
| `InvoiceSettled` | `"invoice_settled"` | Invoice was fully settled (loan repaid) | `settle_invoice` |
| `InvoiceSettledFinal` | `"invoice_settled_final"`| Final settlement acknowledgment | `settle_invoice` |
| `InvoiceDefaulted` | `"invoice_defaulted"` | Invoice passed grace period without repayment | `mark_invoice_defaulted` |
| `InvoiceExpired` | `"invoice_expired"` | Invoice expired before being verified/funded | `prune_terminal_invoices` |
| `PartialPayment` | `"partial_payment"` | A partial payment is applied to the invoice | `record_partial_payment` |
| `PaymentRecorded` | `"payment_recorded"` | Durably stored payment record | `record_payment` |
| `BidPlaced` | `"bid_placed"` | Investor has placed a bid | `place_bid` |
| `BidAccepted` | `"bid_accepted"` | Invoice owner accepts a bid | `accept_bid_and_fund` |
| `BidWithdrawn` | `"bid_withdrawn"` | Investor cancels their unaccepted bid | `withdraw_bid` |
| `BidCancelled` | `"bid_cancelled"` | Bid has been cancelled | `cancel_bid` |
| `BidExpired` | `"bid_expired"` | Bid TTL elapsed | `cleanup_expired_bids` |
| `EscrowCreated` | `"escrow_created"` | Atomic escrow contract initialization | `accept_bid_and_fund` |
| `EscrowReleased` | `"escrow_released"` | Escrow funds released to business | `release_escrow` |
| `EscrowRefunded` | `"escrow_refunded"` | Escrow funds refunded to investor | `refund_escrow` |
| `InvestmentWithdrawn`| `"investment_withdrawn"`| Investor withdraws active position | `withdraw_investment` |
| `DisputeCreated` | `"dispute_created"` | Invoice dispute was opened | `open_dispute` |
| `DisputeUnderReview` | `"dispute_under_review"`| Dispute escalated to admin review | `escalate_dispute` |
| `DisputeResolved` | `"dispute_resolved"` | Dispute resolved by admin | `resolve_dispute` |
| `DisputeRejected` | `"dispute_rejected"` | Dispute rejected by admin | `resolve_dispute` |

---

## 3. Storage Layout & Keys

The indexer relies on both real-time events and periodic state snapshots. State snapshots require reading raw ledger keys from Soroban storage.

### 3.1 Instance Storage (Eagerly Loaded)
Used for globally shared configuration that is loaded automatically on every invocation.

- **Admin Account Key:**
  ```rust
  const ADMIN_KEY: Symbol = symbol_short!("admin");
  // Value: Option<Address>
  ```
- **Platform Fees Configuration:**
  ```rust
  // Key derived from:
  StorageKeys::platform_fees() // symbol_short!("fees")
  // Value: PlatformFeeConfig
  ```
- **Pending Treasury Address Key:**
  ```rust
  const PENDING_TREASURY_KEY: Symbol = symbol_short!("pnd_trs");
  // Value: Option<(Address, u64)> (Address and execution timestamp)
  ```

### 3.2 Persistent Storage (Lazy Lookup)
Used for primary business entities and lookup indexes.

#### Core Entity Storage Keys
Core entities are wrapped in the `DataKey` enum:
```rust
#[contracttype]
pub enum DataKey {
    Invoice(BytesN<32>),
    Bid(BytesN<32>),
    Investment(BytesN<32>),
    FrozenInvoice(BytesN<32>),
}
```

#### Secondary Query Indexes
QuickLendX maintains secondary indexing structures inside Persistent storage to support efficient lookups. Indexers use these to verify consistency.

| Helper Method | Storage Key (Topic Tuple) | Value Type |
|---|---|---|
| `Indexes::invoices_by_business` | `(symbol_short!("inv_bus"), Address)` | `Vec<BytesN<32>>` (Invoice IDs) |
| `Indexes::invoices_by_status` | `(symbol_short!("inv_st"), Symbol)` | `Vec<BytesN<32>>` (Invoice IDs) |
| `Indexes::bids_by_invoice` | `(symbol_short!("bids_inv"), BytesN<32>)` | `Vec<BytesN<32>>` (Bid IDs) |
| `Indexes::bids_by_investor` | `(symbol_short!("bids_invr"), Address)` | `Vec<BytesN<32>>` (Bid IDs) |
| `Indexes::bids_by_status` | `(symbol_short!("bids_stat"), Symbol)` | `Vec<BytesN<32>>` (Bid IDs) |
| `Indexes::investments_by_invoice`| `(symbol_short!("invst_inv"), BytesN<32>)`| `Vec<BytesN<32>>` (Investment IDs) |
| `Indexes::investments_by_investor`| `(symbol_short!("inv_invst"), Address)` | `Vec<BytesN<32>>` (Investment IDs) |
| `Indexes::invoices_by_customer` | `(symbol_short!("inv_cust"), String)` | `Vec<BytesN<32>>` (Invoice IDs) |

---

## 4. Core Data Structures (`no_std`)

The contracts adhere to strict `#![no_std]` discipline. All data structures indexed off-chain use `soroban_sdk` primitives.

### 4.1 Enums (State Machines)

```rust
use soroban_sdk::contracttype;

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvoiceStatus {
    Pending,
    Verified,
    Funded,
    Paid,
    Defaulted,
    Cancelled,
    Refunded,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BidStatus {
    Placed,
    Accepted,
    Withdrawn,
    Expired,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestmentStatus {
    Active,
    Withdrawn,
    Completed,
    Defaulted,
    Refunded,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    None,
    Disputed,
    UnderReview,
    Resolved,
}
```

### 4.2 Entity Structs

```rust
use soroban_sdk::{contracttype, Address, BytesN, String, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineItemRecord(pub String, pub u32, pub i128, pub i128);

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRecord {
    pub amount: i128,
    pub payer: Address,
    pub timestamp: u64,
    pub transaction_id: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Invoice {
    pub id: BytesN<32>,
    pub business: Address,
    pub amount: i128,
    pub currency: Address,
    pub due_date: u64,
    pub status: InvoiceStatus,
    pub created_at: u64,
    pub description: String,
    pub metadata_customer_name: Option<String>,
    pub metadata_customer_address: Option<String>,
    pub metadata_tax_id: Option<String>,
    pub metadata_notes: Option<String>,
    pub metadata_line_items: Vec<LineItemRecord>,
    pub category: InvoiceCategory,
    pub tags: Vec<String>,
    pub funded_amount: i128,
    pub funded_at: Option<u64>,
    pub investor: Option<Address>,
    pub settled_at: Option<u64>,
    pub dispute_status: DisputeStatus,
    pub dispute: Dispute,
    pub total_paid: i128,
    pub payment_history: Vec<PaymentRecord>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bid {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub bid_amount: i128,
    pub expected_return: i128,
    pub timestamp: u64,
    pub status: BidStatus,
    pub expiration_timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Investment {
    pub investment_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub amount: i128,
    pub funded_at: u64,
    pub status: InvestmentStatus,
    pub insurance: Vec<InsuranceCoverage>,
}
```

---

## 5. Read-Only Query Entry Points

The indexer periodically verifies the local database consistency by querying the smart contract's read-only entry points.

```rust
use soroban_sdk::{Env, Address, BytesN, Vec};

// Fetch a single invoice by its unique 32-byte hash
pub fn get_invoice(env: Env, invoice_id: BytesN<32>) -> Result<Invoice, QuickLendXError>;

// Fetch a single bid by its unique 32-byte hash
pub fn get_bid(env: Env, bid_id: BytesN<32>) -> Option<Bid>;

// Fetch an investment structure for a given investment ID
pub fn get_investment(env: Env, investment_id: BytesN<32>) -> Option<Investment>;

// Fetch all bids ranked deterministically for an invoice
pub fn get_ranked_bids(env: Env, invoice_id: BytesN<32>) -> Vec<Bid>;

// Fetch all invoice IDs uploaded by a specific business address
pub fn get_business_invoices(env: Env, business: Address) -> Vec<BytesN<32>>;
```

---

## 6. Compatibility Checklist for Smart Contract Developers

To ensure you do not break off-chain indexing pipelines, always follow this checklist when modifying the Rust contracts:

1. **Do not modify string constants in `events.rs`:** Changing any `TOPIC_` string (e.g. `TOPIC_INVOICE_UPLOADED`) will orphan existing indexer subscriptions.
2. **Never change enums without back-compatibility:** 
   - Never insert a new variant in the middle of a `#[contracttype]` enum (e.g. `InvoiceStatus`). Variants are represented as integers in XDR. Inserting a variant shifts subsequent values, causing decoding errors or corrupting historical status fields.
   - Only append new variants to the end of the enum.
3. **Do not change struct field names or order:** Indexers decode serialized XDR struct mappings. Changing a field name or removing a field will prevent the indexer from deserializing older ledger records. Use `Option<T>` for any new optional metadata fields.
4. **Ensure `no_std` compliance:** Do not introduce standard library primitives (`std::collections::HashMap`, `std::vec::Vec`). Use `soroban_sdk::Map` and `soroban_sdk::Vec` to preserve binary deserialization properties.

# QuickLendX Protocol — Complete Event Schema Reference

> **Version:** 1.0  
> **Source of truth:** `src/events.rs`  
> **Off-chain indexers:** subscribe to the `TOPIC_*` constants; never hard-code string literals.

---

## Overview

All protocol state transitions emit structured events via `env.events().publish(topics, data)`.
Events are the canonical mechanism for off-chain indexers, analytics pipelines, and audit tooling
to reconstruct protocol state without querying contract storage directly.

### Design Principles

| Principle | Implementation |
|-----------|---------------|
| **Schema stability** | `TOPIC_*` constants are compile-time `&str` values; any rename is a breaking change |
| **No PII** | No customer names, tax IDs, or private metadata appear in any event payload |
| **Determinism** | Events depend only on validated contract state; no external randomness |
| **Gas efficiency** | `#[contractevent]` macro generates minimal XDR; topics use `Symbol::new` |
| **Semantic aliases** | Domain names (`InvoiceCreated`, `FundsLocked`, `LoanSettled`, `DisputeOpened`) are type aliases to canonical structs |

---

## Semantic Aliases

The task specification uses domain-level names. These are type aliases to canonical event types:

| Domain Name | Canonical Type | Topic Constant |
|-------------|---------------|----------------|
| `InvoiceCreated` | `InvoiceUploaded` | `TOPIC_INVOICE_UPLOADED` = `"invoice_uploaded"` |
| `FundsLocked` | `EscrowCreated` | `TOPIC_ESCROW_CREATED` = `"escrow_created"` |
| `LoanSettled` | `InvoiceSettled` | `TOPIC_INVOICE_SETTLED` = `"invoice_settled"` |
| `DisputeOpened` | `DisputeCreated` | `TOPIC_DISPUTE_CREATED` = `"dispute_created"` |

---

## Invoice Events

### `InvoiceUploaded` / `InvoiceCreated`

Emitted when a business uploads a new invoice.

**Topic:** `"invoice_uploaded"`  
**Constant:** `TOPIC_INVOICE_UPLOADED`

| Field | Type | Description |
|-------|------|-------------|
| `invoice_id` | `BytesN<32>` | Unique 32-byte invoice identifier |
| `business` | `Address` | Business that owns the invoice |
| `amount` | `i128` | Invoice face value in smallest currency unit |
| `currency` | `Address` | Token contract address for the invoice currency |
| `due_date` | `u64` | Unix timestamp when the invoice is due |
| `timestamp` | `u64` | Ledger timestamp at emission time |

**Emitted by:** `upload_invoice()`

---

### `InvoiceVerified`

Emitted when an admin verifies an invoice.

**Topic:** `"invoice_verified"`  
**Constant:** `TOPIC_INVOICE_VERIFIED`

| Field | Type | Description |
|-------|------|-------------|
| `invoice_id` | `BytesN<32>` | Invoice identifier |
| `business` | `Address` | Business that owns the invoice |
| `timestamp` | `u64` | Ledger timestamp at emission time |

**Emitted by:** `verify_invoice()`, `update_invoice_status(Verified)`

---

### `InvoiceCancelled`

Emitted when a business cancels an invoice (only valid from Pending or Verified status).

**Topic:** `"invoice_cancelled"`  
**Constant:** `TOPIC_INVOICE_CANCELLED`

| Field | Type | Description |
|-------|------|-------------|
| `invoice_id` | `BytesN<32>` | Invoice identifier |
| `business` | `Address` | Business that owns the invoice |
| `timestamp` | `u64` | Ledger timestamp at emission time |

**Emitted by:** `cancel_invoice()`

---

### `InvoiceSettled` / `LoanSettled`

Emitted when an invoice is fully settled (loan repaid in full).

**Topic:** `"invoice_settled"`  
**Constant:** `TOPIC_INVOICE_SETTLED`

| Field | Type | Description |
|-------|------|-------------|
| `invoice_id` | `BytesN<32>` | Invoice identifier |
| `business` | `Address` | Business that owns the invoice |
| `investor` | `Address` | Investor who funded the invoice |
| `investor_return` | `i128` | Total amount returned to investor (principal + profit) |
| `platform_fee` | `i128` | Platform fee deducted from settlement |
| `timestamp` | `u64` | Ledger timestamp at emission time |

**Security:** `investor_return` and `platform_fee` are derived from validated contract state only. No PII.

**Emitted by:** `make_payment()` (on full settlement), `update_invoice_status(Paid)`

---

### `InvoiceSettledFinal`

Emitted when all funds are disbursed after final settlement.

**Topic:** `"invoice_settled_final"`  
**Constant:** `TOPIC_INVOICE_SETTLED_FINAL`

| Field | Type | Description |
|-------|------|-------------|
| `invoice_id` | `BytesN<32>` | Invoice identifier |
| `business` | `Address` | Business that owns the invoice |
| `investor` | `Address` | Investor who funded the invoice |
| `total_paid` | `i128` | Total amount paid |
| `timestamp` | `u64` | Ledger timestamp at emission time |

**Emitted by:** Settlement finalization path

---

### `InvoiceDefaulted`

Emitted when an invoice is marked as defaulted (past due date, no payment).

**Topic:** `"invoice_defaulted"`  
**Constant:** `TOPIC_INVOICE_DEFAULTED`

| Field | Type | Description |
|-------|------|-------------|
| `invoice_id` | `BytesN<32>` | Invoice identifier |
| `business` | `Address` | Business that owns the invoice |
| `investor` | `Address` | Investor who funded the invoice |
| `timestamp` | `u64` | Ledger timestamp at emission time |

**Emitted by:** `handle_default()`, `update_invoice_status(Defaulted)`

---

### `InvoiceExpired`

Emitted when an invoice expires past its due date without being funded.

**Topic:** `"invoice_expired"`  
**Constant:** `TOPIC_INVOICE_EXPIRED`

| Field | Type | Description |
|-------|------|-------------|
| `invoice_id` | `BytesN<32>` | Invoice identifier |
| `business` | `Address` | Business that owns the invoice |
| `due_date` | `u64` | The due date that was missed |

**Emitted by:** `expire_invoice()`

---

### `InvoiceFunded`

Emitted when an invoice transitions to Funded status.

**Topic:** `"invoice_funded"`  
**Constant:** `TOPIC_INVOICE_FUNDED`

| Field | Type | Description |
|-------|------|-------------|
| `invoice_id` | `BytesN<32>` | Invoice identifier |
| `investor` | `Address` | Investor who funded the invoice |
| `amount` | `i128` | Amount funded |
| `timestamp` | `u64` | Ledger timestamp at emission time |

**Emitted by:** `accept_bid()`, `update_invoice_status(Funded)`

---

### `PartialPayment`

Emitted on each partial payment towards an invoice.

**Topic:** `"partial_payment"`  
**Constant:** `TOPIC_PARTIAL_PAYMENT`

| Field | Type | Description |
|-------|------|-------------|
| `invoice_id` | `BytesN<32>` | Invoice identifier |
| `business` | `Address` | Business making the payment |
| `payment_amount` | `i128` | Amount paid in this transaction |
| `total_paid` | `i128` | Cumulative total paid so far |
| `progress` | `u32` | Progress in basis points (0–10000 = 0%–100%) |
| `transaction_id` | `String` | External transaction reference |

**Emitted by:** `make_payment()` (when payment is partial)

---

### `PaymentRecorded`

Emitted when a payment record is durably stored.

**Topic:** `"payment_recorded"`  
**Constant:** `TOPIC_PAYMENT_RECORDED`

| Field | Type | Description |
|-------|------|-------------|
| `invoice_id` | `BytesN<32>` | Invoice identifier |
| `payer` | `Address` | Address of the payer |
| `amount` | `i128` | Amount paid |
| `transaction_id` | `String` | External transaction reference |
| `timestamp` | `u64` | Ledger timestamp at emission time |

---

### `InvoiceMetadataUpdated`

Emitted when structured metadata is updated on an invoice.

**Topic:** `"invoice_metadata_updated"`

| Field | Type | Description |
|-------|------|-------------|
| `invoice_id` | `BytesN<32>` | Invoice identifier |
| `line_item_count` | `u32` | Number of line items in the metadata |
| `total_value` | `i128` | Aggregate value of all line items |
| `timestamp` | `u64` | Ledger timestamp at emission time |

**Security:** ⚠️ `customer_name` and `tax_id` are **intentionally excluded** to prevent PII leakage on-chain. Only aggregate statistics are emitted.

---

## Bid Events

### `BidPlaced`

Emitted when an investor places a bid on an invoice.

**Topic:** `"bid_placed"`  
**Constant:** `TOPIC_BID_PLACED`

| Field | Type | Description |
|-------|------|-------------|
| `bid_id` | `BytesN<32>` | Unique bid identifier |
| `invoice_id` | `BytesN<32>` | The invoice being bid on (`auction_id` in protocol terms) |
| `investor` | `Address` | Address of the bidder |
| `bid_amount` | `i128` | Amount offered by the investor |
| `expected_return` | `i128` | Total expected repayment amount |
| `timestamp` | `u64` | Ledger timestamp when bid was placed |
| `expiration_timestamp` | `u64` | Timestamp after which the bid expires |

**Emitted by:** `place_bid()`

---

### `BidAccepted`

Emitted when a business accepts a bid.

**Topic:** `"bid_accepted"`  
**Constant:** `TOPIC_BID_ACCEPTED`

| Field | Type | Description |
|-------|------|-------------|
| `bid_id` | `BytesN<32>` | Unique bid identifier |
| `invoice_id` | `BytesN<32>` | The invoice being funded |
| `investor` | `Address` | Address of the investor |
| `business` | `Address` | Address of the business |
| `bid_amount` | `i128` | Amount locked in escrow |
| `expected_return` | `i128` | Total expected repayment amount |
| `timestamp` | `u64` | Ledger timestamp at emission time |

**Emitted by:** `accept_bid()`  
**Note:** Always co-emitted with `EscrowCreated` / `FundsLocked`.

---

### `BidWithdrawn`

Emitted when an investor withdraws their bid.

**Topic:** `"bid_withdrawn"`  
**Constant:** `TOPIC_BID_WITHDRAWN`

| Field | Type | Description |
|-------|------|-------------|
| `bid_id` | `BytesN<32>` | Unique bid identifier |
| `invoice_id` | `BytesN<32>` | The invoice the bid was on |
| `investor` | `Address` | Address of the investor |
| `bid_amount` | `i128` | Amount that was bid |
| `timestamp` | `u64` | Ledger timestamp at emission time |

**Emitted by:** `withdraw_bid()`

---

### `BidExpired`

Emitted when a bid expires past its TTL.

**Topic:** `"bid_expired"`  
**Constant:** `TOPIC_BID_EXPIRED`

| Field | Type | Description |
|-------|------|-------------|
| `bid_id` | `BytesN<32>` | Unique bid identifier |
| `invoice_id` | `BytesN<32>` | The invoice the bid was on |
| `investor` | `Address` | Address of the investor |
| `bid_amount` | `i128` | Amount that was bid |
| `expiration_timestamp` | `u64` | Timestamp at which the bid expired |

**Emitted by:** `clean_expired_bids()`

---

## Escrow Events

### `EscrowCreated` / `FundsLocked`

Emitted when investor funds are locked in escrow upon bid acceptance.

**Topic:** `"escrow_created"`  
**Constant:** `TOPIC_ESCROW_CREATED`

| Field | Type | Description |
|-------|------|-------------|
| `escrow_id` | `BytesN<32>` | Unique escrow identifier |
| `invoice_id` | `BytesN<32>` | The invoice being funded |
| `investor` | `Address` | Address of the investor whose funds are locked |
| `business` | `Address` | Address of the business receiving the funds |
| `amount` | `i128` | Amount locked in escrow |

**Security:** Funds are locked atomically with bid acceptance. No PII included.  
**Emitted by:** `accept_bid()` (always co-emitted with `BidAccepted`)

---

### `EscrowReleased`

Emitted when escrow funds are released to the business.

**Topic:** `"escrow_released"`  
**Constant:** `TOPIC_ESCROW_RELEASED`

| Field | Type | Description |
|-------|------|-------------|
| `escrow_id` | `BytesN<32>` | Unique escrow identifier |
| `invoice_id` | `BytesN<32>` | The invoice that was funded |
| `business` | `Address` | Address of the business receiving funds |
| `amount` | `i128` | Amount released |

**Emitted by:** `release_escrow_funds()`

---

### `EscrowRefunded`

Emitted when escrow funds are refunded to the investor.

**Topic:** `"escrow_refunded"`  
**Constant:** `TOPIC_ESCROW_REFUNDED`

| Field | Type | Description |
|-------|------|-------------|
| `escrow_id` | `BytesN<32>` | Unique escrow identifier |
| `invoice_id` | `BytesN<32>` | The invoice that was funded |
| `investor` | `Address` | Address of the investor receiving the refund |
| `amount` | `i128` | Amount refunded |

**Emitted by:** `refund_escrow()`

---

## Dispute Events

### `DisputeCreated` / `DisputeOpened`

Emitted when a dispute is opened on an invoice.

**Topic:** `"dispute_created"`  
**Constant:** `TOPIC_DISPUTE_CREATED`

| Field | Type | Description |
|-------|------|-------------|
| `invoice_id` | `BytesN<32>` | The disputed invoice |
| `created_by` | `Address` | Address of the dispute initiator (business or investor) |
| `reason` | `String` | Reason code or short description (no PII, max 1000 chars) |
| `timestamp` | `u64` | Ledger timestamp at emission time |

**Security:** Only the business owner or investor on the invoice may open a dispute. The `reason` field must not contain PII.  
**Emitted by:** `create_dispute()`

---

### `DisputeUnderReview`

Emitted when a dispute is moved to admin review.

**Topic:** `"dispute_under_review"`  
**Constant:** `TOPIC_DISPUTE_UNDER_REVIEW`

| Field | Type | Description |
|-------|------|-------------|
| `invoice_id` | `BytesN<32>` | The disputed invoice |
| `reviewed_by` | `Address` | Address of the admin reviewer |
| `timestamp` | `u64` | Ledger timestamp at emission time |

**Emitted by:** `put_dispute_under_review()`

---

### `DisputeResolved`

Emitted when a dispute is resolved by an admin.

**Topic:** `"dispute_resolved"`  
**Constant:** `TOPIC_DISPUTE_RESOLVED`

| Field | Type | Description |
|-------|------|-------------|
| `invoice_id` | `BytesN<32>` | The disputed invoice |
| `resolved_by` | `Address` | Address of the admin who resolved the dispute |
| `resolution` | `String` | Resolution description (no PII) |
| `timestamp` | `u64` | Ledger timestamp at emission time |

**Emitted by:** `resolve_dispute()`

---

## Platform Fee Events

### `PlatformFeeUpdated`

Emitted when the platform fee configuration is updated.

**Topic:** `"platform_fee_updated"`

| Field | Type | Description |
|-------|------|-------------|
| `fee_bps` | `u32` | New fee in basis points |
| `updated_at` | `u64` | Timestamp of the update |
| `updated_by` | `Address` | Admin who made the update |

**Emitted by:** `set_platform_fee()`

---

### `PlatformFeeRouted`

Emitted when platform fees are routed to the treasury.

**Topic:** `"platform_fee_routed"`

| Field | Type | Description |
|-------|------|-------------|
| `invoice_id` | `BytesN<32>` | Invoice the fee was collected from |
| `recipient` | `Address` | Treasury address receiving the fee |
| `fee_amount` | `i128` | Fee amount routed |
| `timestamp` | `u64` | Ledger timestamp at emission time |

---

## Admin Events

### `AdminSet`

Emitted when the admin address is set.

**Topic:** `"admin_set"`

| Field | Type | Description |
|-------|------|-------------|
| `admin` | `Address` | The new admin address |
| `timestamp` | `u64` | Ledger timestamp at emission time |

---

### `AdminTransferred`

Emitted when the admin role is transferred.

**Topic:** `"adm_trf"` (raw `symbol_short!`)

| Field | Type | Description |
|-------|------|-------------|
| `old_admin` | `Address` | Previous admin address |
| `new_admin` | `Address` | New admin address |
| `timestamp` | `u64` | Ledger timestamp at emission time |

---

### `ProtocolInitialized`

Emitted once when the protocol is initialized.

**Topic:** `"protocol_initialized"`

| Field | Type | Description |
|-------|------|-------------|
| `admin` | `Address` | Initial admin address |
| `treasury` | `Address` | Treasury address |
| `fee_bps` | `u32` | Initial fee in basis points |
| `min_invoice_amount` | `i128` | Minimum invoice amount |
| `max_due_date_days` | `u64` | Maximum due date in days |
| `grace_period_seconds` | `u64` | Grace period in seconds |
| `timestamp` | `u64` | Ledger timestamp at emission time |

---

## Emergency Events

### `EmergencyWithdrawalInitiated`

Emitted when an emergency withdrawal is initiated (timelock starts).

**Topic:** `"emergency_withdrawal_initiated"`

| Field | Type | Description |
|-------|------|-------------|
| `token` | `Address` | Token contract address |
| `amount` | `i128` | Amount to withdraw |
| `target` | `Address` | Target address for funds |
| `unlock_at` | `u64` | Timestamp when withdrawal can be executed |
| `admin` | `Address` | Admin who initiated the withdrawal |

---

### `EmergencyWithdrawalExecuted`

Emitted when an emergency withdrawal is executed after the timelock.

**Topic:** `"emergency_withdrawal_executed"`

| Field | Type | Description |
|-------|------|-------------|
| `token` | `Address` | Token contract address |
| `amount` | `i128` | Amount withdrawn |
| `target` | `Address` | Target address that received funds |
| `admin` | `Address` | Admin who executed the withdrawal |

---

### `EmergencyWithdrawalCancelled`

Emitted when a pending emergency withdrawal is cancelled.

**Topic:** `"emergency_withdrawal_cancelled"`

| Field | Type | Description |
|-------|------|-------------|
| `token` | `Address` | Token contract address |
| `amount` | `i128` | Amount that was pending |
| `target` | `Address` | Target address |
| `admin` | `Address` | Admin who cancelled the withdrawal |

---

## Security Guarantees

### No PII in Events

The following fields are **explicitly excluded** from all event payloads:

- Customer names
- Tax IDs / VAT numbers
- Contact information
- Private metadata fields

The `InvoiceMetadataUpdated` event emits only `line_item_count` and `total_value` — never `customer_name` or `tax_id`.

### Tamper-Proof Timestamps

All `timestamp` fields use `env.ledger().timestamp()`, which is set by the Stellar network consensus and cannot be manipulated by contract callers.

### No Events on Failed Transactions

Soroban's execution model guarantees that if a contract function panics or returns an error, all state changes (including event emissions) are rolled back. This means:

- Failed bid placements emit zero events
- Duplicate dispute attempts emit zero events
- Unauthorized operations emit zero events

This is verified by the negative tests in `src/test_events.rs`.

### Determinism

Events depend only on validated contract state. No external randomness, no block hashes, no caller-supplied timestamps are used in event payloads.

---

## Topic Constant Reference

```rust
// Invoice
pub const TOPIC_INVOICE_UPLOADED: &str      = "invoice_uploaded";
pub const TOPIC_INVOICE_VERIFIED: &str      = "invoice_verified";
pub const TOPIC_INVOICE_CANCELLED: &str     = "invoice_cancelled";
pub const TOPIC_INVOICE_SETTLED: &str       = "invoice_settled";
pub const TOPIC_INVOICE_DEFAULTED: &str     = "invoice_defaulted";
pub const TOPIC_INVOICE_EXPIRED: &str       = "invoice_expired";
pub const TOPIC_PARTIAL_PAYMENT: &str       = "partial_payment";
pub const TOPIC_PAYMENT_RECORDED: &str      = "payment_recorded";
pub const TOPIC_INVOICE_SETTLED_FINAL: &str = "invoice_settled_final";
pub const TOPIC_INVOICE_FUNDED: &str        = "invoice_funded";

// Bids
pub const TOPIC_BID_PLACED: &str    = "bid_placed";
pub const TOPIC_BID_ACCEPTED: &str  = "bid_accepted";
pub const TOPIC_BID_WITHDRAWN: &str = "bid_withdrawn";
pub const TOPIC_BID_EXPIRED: &str   = "bid_expired";

// Escrow
pub const TOPIC_ESCROW_CREATED: &str  = "escrow_created";
pub const TOPIC_ESCROW_RELEASED: &str = "escrow_released";
pub const TOPIC_ESCROW_REFUNDED: &str = "escrow_refunded";

// Disputes
pub const TOPIC_DISPUTE_CREATED: &str      = "dispute_created";
pub const TOPIC_DISPUTE_UNDER_REVIEW: &str = "dispute_under_review";
pub const TOPIC_DISPUTE_RESOLVED: &str     = "dispute_resolved";
```

---

## Test Coverage

Event schema tests live in `src/test_events.rs`. Coverage includes:

| Test | What it verifies |
|------|-----------------|
| `test_topic_constants_are_stable` | All `TOPIC_*` constants match expected string values |
| `test_invoice_uploaded_field_order` | `InvoiceUploaded` field order and values |
| `test_invoice_verified_field_order` | `InvoiceVerified` field order and values |
| `test_invoice_cancelled_field_order` | `InvoiceCancelled` field order and values |
| `test_invoice_defaulted_field_order` | `InvoiceDefaulted` field order and values |
| `test_invoice_settled_field_order` | `InvoiceSettled` field order and values |
| `test_invoice_expired_field_order` | `InvoiceExpired` field order and values |
| `test_partial_payment_field_order` | `PartialPayment` field order and values |
| `test_bid_placed_field_order` | `BidPlaced` field order and values |
| `test_bid_accepted_field_order` | `BidAccepted` field order and values |
| `test_bid_withdrawn_field_order` | `BidWithdrawn` field order and values |
| `test_bid_expired_field_order` | `BidExpired` field order and values |
| `test_escrow_created_field_order` | `EscrowCreated` field order and values |
| `test_escrow_released_field_order` | `EscrowReleased` field order and values |
| `test_escrow_refunded_field_order_on_cancellation` | `EscrowRefunded` field order and values |
| `test_dispute_lifecycle_field_orders` | Full dispute lifecycle: Created → UnderReview → Resolved |
| `test_platform_fee_updated_field_order` | `PlatformFeeUpdated` emission |
| `test_funds_locked_event_schema` | `FundsLocked` alias validation |
| `test_loan_settled_event_schema` | `LoanSettled` alias validation |
| `test_dispute_opened_event_schema` | `DisputeOpened` alias validation |
| `test_no_events_emitted_for_reads` | Read-only calls emit zero events |
| `test_event_ordering_across_lifecycle` | Timestamps are strictly increasing across lifecycle |
| `test_no_events_on_failed_bid_placement` | Failed transactions emit zero events |
| `test_no_events_on_duplicate_dispute` | Duplicate disputes emit zero events |
| `test_no_events_on_cancel_funded_invoice` | Invalid state transitions emit zero events |

---

## Off-Chain Indexer Integration

To subscribe to QuickLendX events from a Stellar Horizon or RPC node:

```typescript
// Subscribe to all invoice lifecycle events
const INVOICE_TOPICS = [
  "invoice_uploaded",
  "invoice_verified",
  "invoice_cancelled",
  "invoice_settled",
  "invoice_defaulted",
  "invoice_expired",
  "invoice_funded",
];

// Subscribe to bid events
const BID_TOPICS = [
  "bid_placed",
  "bid_accepted",
  "bid_withdrawn",
  "bid_expired",
];

// Subscribe to escrow events
const ESCROW_TOPICS = [
  "escrow_created",   // == FundsLocked
  "escrow_released",
  "escrow_refunded",
];

// Subscribe to dispute events
const DISPUTE_TOPICS = [
  "dispute_created",       // == DisputeOpened
  "dispute_under_review",
  "dispute_resolved",
];
```

Import the `TOPIC_*` constants from `src/events.rs` directly in any Rust-based indexer to avoid string drift.

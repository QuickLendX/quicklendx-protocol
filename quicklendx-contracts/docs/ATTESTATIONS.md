# Attestation Events — Schema and Lifecycle for Off-Chain Indexers

**Audience: downstream integrators** building off-chain indexers, data pipelines, or analytics systems that consume QuickLendX on-chain events.

---

## Overview

"Attestation" in QuickLendX means a trusted on-chain assertion that a real-world entity or document has been reviewed and approved.  
There are two kinds:

| Kind | What gets attested | Emitted by |
|---|---|---|
| **KYC / identity verification** | A business or investor address has passed KYC | `verification.rs` |
| **Invoice verification** | An uploaded invoice is legitimate and eligible for funding | `invoice.rs` |

Both follow the same pattern: an admin calls a privileged entry-point, the contract validates state, transitions the entity's status, and publishes a structured `#[contractevent]` that the indexer stores verbatim.

All topic constants referenced below are defined in `src/events.rs` and can be imported directly — never hard-code the string literals, because any rename is a **breaking schema change**.

---

## 1. KYC Attestation Events

### 1.1 State machine

```
           submit_kyc()
  (none) ─────────────► Pending
                          │
              admin        │ admin
              approve()    │ reject()
                  │        │
                  ▼        ▼
              Verified   Rejected
                           │
                           │ resubmit_kyc()
                           ▼
                         Pending   (cycle repeats)
```

`Verified` is terminal for a given address — a verified address cannot be re-submitted.  
`Rejected → Pending` is allowed (resubmission after rejection).

### 1.2 `InvestorVerified`

Emitted when an admin approves a KYC submission for a business **or** investor address.

**Topic:** no `TOPIC_*` constant is exported for this event yet — subscribe by struct name `InvestorVerified`.  
**Source:** `src/events.rs` → `emit_investor_verified`

**Schema (Rust):**

```rust
#[contractevent]
pub struct InvestorVerified {
    pub investor: Address,        // verified address (business or investor)
    pub investment_limit: i128,   // approved investment cap in smallest unit; 0 for businesses
    pub verified_at: u64,         // ledger timestamp of approval
}
```

**JSON payload (as stored by the indexer):**

```json
{
  "type": "InvestorVerified",
  "ledger": 1042900,
  "txHash": "3d4f...a1b2",
  "eventIndex": 0,
  "payload": {
    "investor": "GBIZ...XYZW",
    "investment_limit": "5000000000",
    "verified_at": 1718000000
  }
}
```

> Note: `i128` values are serialised as decimal strings in JSON to avoid 64-bit integer overflow in standard JSON parsers.

**Indexer responsibilities:**
- Upsert a `verifications` record keyed on `investor` address, setting `status = "Verified"`, `verified_at`, and `investment_limit`.
- Use `(tx_hash, event_index)` as the idempotency key (see [Indexer — Transaction Semantics](../../backend/docs/indexer.md)).

---

## 2. Invoice Verification (Document Attestation) Events

### 2.1 State machine

Invoice status is the primary attestation signal for off-chain consumers:

```
  upload_invoice()      verify_invoice()       accept_bid()
  ────────────────►  Pending ────────────► Verified ────────────► Funded
                       │                                             │
                       │ cancel_invoice()            settle / default / expire
                       ▼                                             ▼
                    Cancelled                               Paid | Defaulted | Expired
```

An invoice can only receive bids and be funded after it reaches `Verified`.

### 2.2 `InvoiceUploaded` (alias: `InvoiceCreated`)

Emitted when a business submits a new invoice.

**Topic constant:** `TOPIC_INVOICE_UPLOADED = "invoice_uploaded"`  
**Source:** `src/events.rs` → `emit_invoice_uploaded`

```rust
#[contractevent]
pub struct InvoiceUploaded {
    pub invoice_id: BytesN<32>,   // 32-byte unique identifier (hex-encoded in JSON)
    pub business: Address,
    pub amount: i128,             // face value in smallest currency unit
    pub currency: Address,        // token contract address
    pub due_date: u64,            // Unix timestamp when invoice is due
    pub timestamp: u64,           // ledger timestamp at emission
}
```

**JSON payload example:**

```json
{
  "type": "InvoiceUploaded",
  "ledger": 1042857,
  "txHash": "a1b2c3...f9e8",
  "eventIndex": 0,
  "payload": {
    "invoice_id": "d4e5f6...0102",
    "business": "GBIZ...WXYZ",
    "amount": "100000000000",
    "currency": "CCURRENCY...CONTRACT",
    "due_date": 1720000000,
    "timestamp": 1718000000
  }
}
```

### 2.3 `InvoiceVerified`

Emitted when an admin marks an invoice as legitimate and eligible for bids.  
**This is the core attestation event** — it certifies that a human reviewer has confirmed the invoice's authenticity.

**Topic constant:** `TOPIC_INVOICE_VERIFIED = "invoice_verified"`  
**Source:** `src/events.rs` → `emit_invoice_verified`

```rust
#[contractevent]
pub struct InvoiceVerified {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub timestamp: u64,
}
```

**JSON payload example:**

```json
{
  "type": "InvoiceVerified",
  "ledger": 1042901,
  "txHash": "b2c3d4...e5f6",
  "eventIndex": 1,
  "payload": {
    "invoice_id": "d4e5f6...0102",
    "business": "GBIZ...WXYZ",
    "timestamp": 1718003600
  }
}
```

**Indexer responsibilities:**
- Transition the indexed invoice's `status` from `Pending` → `Verified`.
- Record the attestation timestamp for audit queries.

### 2.4 `InvoiceCancelled`

Emitted when the business owner cancels an invoice before funding.

**Topic constant:** `TOPIC_INVOICE_CANCELLED = "invoice_cancelled"`

```rust
#[contractevent]
pub struct InvoiceCancelled {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub timestamp: u64,
}
```

---

## 3. Full Attestation Lifecycle Example

Below is a realistic sequence from invoice creation to full settlement, showing only the attestation-relevant events. Ledger numbers are illustrative.

```
Ledger 1042857  →  InvoiceUploaded     { invoice_id: "d4e5...", amount: 100_000_000_000 }
Ledger 1042901  →  InvoiceVerified     { invoice_id: "d4e5...", business: "GBIZ..." }
                   ← invoice is now open for bids

Ledger 1043100  →  BidPlaced           { bid_id: "aa01...", invoice_id: "d4e5...", investor: "GINV..." }
Ledger 1043200  →  BidAccepted         { bid_id: "aa01...", invoice_id: "d4e5..." }
Ledger 1043201  →  EscrowCreated       { escrow_id: "ee01...", invoice_id: "d4e5..." }
                   ← invoice transitions to Funded

Ledger 1044500  →  PartialPayment      { invoice_id: "d4e5...", progress: 50 }
Ledger 1045000  →  InvoiceSettled      { invoice_id: "d4e5...", investor_return: ..., platform_fee: ... }
Ledger 1045000  →  InvoiceSettledFinal { invoice_id: "d4e5...", total_paid: 100_000_000_000 }
```

The `InvoiceVerified` event is the attestation checkpoint. Any indexer maintaining an allowlist of "fundable invoices" should only move an invoice into that set after observing this event.

---

## 4. `RawEvent` Schema (Indexer Wire Format)

Every on-chain event is stored by the backend indexer as a `RawEvent` record before being projected into domain tables.

```typescript
// src/types/replay.ts
interface RawEvent {
  id: string;           // "${txHash}:${eventIndex}"
  ledger: number;       // Soroban ledger sequence
  txHash: string;       // transaction hash (hex)
  eventIndex: number;   // position of event within the transaction (0-based)
  type: string;         // matches the TOPIC_* constant from events.rs
  payload: Record<string, unknown>; // decoded event fields
  timestamp: number;    // Unix ms, set by indexer at ingest time
  complianceHold: boolean;          // true when flagged for compliance review
  indexedAt: string;    // ISO 8601 UTC, set by indexer at ingest time
}
```

The `type` field is the canonical subscription key — use the `TOPIC_*` constants from `src/events.rs` to build your filter, for example:

```typescript
import {
  TOPIC_INVOICE_VERIFIED,
  TOPIC_INVOICE_UPLOADED,
} from "quicklendx-contracts/src/events";

const attestationEvents = rawEvents.filter(
  (e) =>
    e.type === TOPIC_INVOICE_VERIFIED ||
    e.type === "InvestorVerified"
);
```

---

## 5. Semantic Aliases

The contract exports type aliases so callers can use either the domain name or the protocol name. Both refer to identical schemas.

| Domain name | Protocol type | Topic constant |
|---|---|---|
| `InvoiceCreated` | `InvoiceUploaded` | `TOPIC_INVOICE_UPLOADED` |
| `FundsLocked` | `EscrowCreated` | `TOPIC_ESCROW_CREATED` |
| `LoanSettled` | `InvoiceSettled` | `TOPIC_INVOICE_SETTLED` |
| `DisputeOpened` | `DisputeCreated` | `TOPIC_DISPUTE_CREATED` |

Subscribe using the `TOPIC_*` constant, not the alias name.

---

## 6. Idempotency and Reorg Safety

Attestation events are ordinary Soroban events and subject to the same ingestion guarantees as all other events:

1. **Event-level idempotency**: the `(tx_hash, event_index)` pair is a unique key in the `raw_events` table. Duplicate deliveries are silently dropped (`ON CONFLICT DO NOTHING`).
2. **Batch-level cursor**: the ingestion cursor can only advance, never regress. Re-submitting an already-committed batch is a no-op.
3. **Reorg recovery**: `rollbackAndReingest()` deletes events above the target ledger, then re-ingests with `ON CONFLICT DO UPDATE` so canonical rows replace orphaned data.

For a detailed description of the unit-of-work contract see [Indexer — Transaction Semantics](../../backend/docs/indexer.md).

---

## 7. Compliance Holds

If `RawEvent.complianceHold` is `true`, the event has been flagged by the compliance layer and **must not** be projected into derived tables until cleared. Consumers should treat such records as pending and re-process them when the hold is lifted.

---

## 8. Topic Reference

All topics below are defined as `pub const TOPIC_*: &str` in `quicklendx-contracts/src/events.rs`.

| `TOPIC_*` constant | String value | Event struct | Emitter function |
|---|---|---|---|
| `TOPIC_INVOICE_UPLOADED` | `"invoice_uploaded"` | `InvoiceUploaded` | `emit_invoice_uploaded` |
| `TOPIC_INVOICE_VERIFIED` | `"invoice_verified"` | `InvoiceVerified` | `emit_invoice_verified` |
| `TOPIC_INVOICE_CANCELLED` | `"invoice_cancelled"` | `InvoiceCancelled` | `emit_invoice_cancelled` |
| `TOPIC_INVOICE_FUNDED` | `"invoice_funded"` | `InvoiceFunded` | `emit_invoice_funded` |
| `TOPIC_INVOICE_SETTLED` | `"invoice_settled"` | `InvoiceSettled` | `emit_invoice_settled` |
| `TOPIC_INVOICE_SETTLED_FINAL` | `"invoice_settled_final"` | `InvoiceSettledFinal` | `emit_invoice_settled_final` |
| `TOPIC_INVOICE_DEFAULTED` | `"invoice_defaulted"` | `InvoiceDefaulted` | `emit_invoice_defaulted` |
| `TOPIC_INVOICE_EXPIRED` | `"invoice_expired"` | `InvoiceExpired` | `emit_invoice_expired` |
| `TOPIC_PARTIAL_PAYMENT` | `"partial_payment"` | `PartialPayment` | `emit_partial_payment` |
| `TOPIC_PAYMENT_RECORDED` | `"payment_recorded"` | `PaymentRecorded` | `emit_payment_recorded` |
| `TOPIC_BID_PLACED` | `"bid_placed"` | `BidPlaced` | `emit_bid_placed` |
| `TOPIC_BID_ACCEPTED` | `"bid_accepted"` | `BidAccepted` | `emit_bid_accepted` |
| `TOPIC_BID_WITHDRAWN` | `"bid_withdrawn"` | `BidWithdrawn` | `emit_bid_withdrawn` |
| `TOPIC_BID_EXPIRED` | `"bid_expired"` | `BidExpired` | `emit_bid_expired` |
| `TOPIC_ESCROW_CREATED` | `"escrow_created"` | `EscrowCreated` | `emit_escrow_created` |
| `TOPIC_ESCROW_RELEASED` | `"escrow_released"` | `EscrowReleased` | `emit_escrow_released` |
| `TOPIC_ESCROW_REFUNDED` | `"escrow_refunded"` | `EscrowRefunded` | `emit_escrow_refunded` |
| `TOPIC_INVESTMENT_WITHDRAWN` | `"investment_withdrawn"` | `InvestmentWithdrawn` | `emit_investment_withdrawn` |
| `TOPIC_DISPUTE_CREATED` | `"dispute_created"` | `DisputeCreated` | `emit_dispute_created` |
| `TOPIC_DISPUTE_UNDER_REVIEW` | `"dispute_under_review"` | `DisputeUnderReview` | `emit_dispute_under_review` |
| `TOPIC_DISPUTE_RESOLVED` | `"dispute_resolved"` | `DisputeResolved` | `emit_dispute_resolved` |

KYC events (`InvestorVerified`) do not yet have a `TOPIC_*` constant; filter by type string `"InvestorVerified"` and plan to migrate once a constant is added.

---

## Related docs

- [Indexer — Transaction Semantics](../../backend/docs/indexer.md) — ingestion unit-of-work, idempotency, and reorg recovery
- [Bid lifecycle](bid-lifecycle.md) — full bid state machine
- [Settlement–dispute interaction](settlement-dispute-interaction.md) — how active disputes block settlement finalisation
- `src/events.rs` — all `TOPIC_*` constants and event struct definitions (single source of truth)
- `src/verification.rs` — KYC state machine implementation

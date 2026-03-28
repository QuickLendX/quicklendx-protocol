# Event Schema Stability — `events.rs`

This document is the canonical reference for every Soroban event emitted by QuickLendX. Off-chain indexers and analytics tools **must** use these exact topic strings and field positions.

---

## Stability Policy

> **Topics are frozen.** Once deployed on a network, no existing topic string will be renamed or removed. New events may be added in future contract upgrades. Payload fields follow **append-only ordering**: existing field positions are frozen; new fields are appended at the end.

---

## Security Notes

| Property | Guarantee |
|---|---|
| Timestamps | Always from `env.ledger().timestamp()` — tamper-proof Soroban clock |
| No PII | Only addresses, identifiers, and amounts are emitted |
| Auth context | All state-mutating events are gated by `require_auth()` upstream |
| Read-only purity | Getter functions emit zero events |

---

## Event Catalog

### Invoice Events

| Topic | Symbol | Description |
|---|---|---|
| `inv_up` | `TOPIC_INVOICE_UPLOADED` | Business uploads a new invoice |
| `inv_ver` | `TOPIC_INVOICE_VERIFIED` | Admin verifies an invoice |
| `inv_canc` | `TOPIC_INVOICE_CANCELLED` | Business cancels an invoice |
| `inv_set` | `TOPIC_INVOICE_SETTLED` | Invoice fully settled |
| `inv_def` | `TOPIC_INVOICE_DEFAULTED` | Invoice marked as defaulted |
| `inv_exp` | `TOPIC_INVOICE_EXPIRED` | Invoice bidding window expired |
| `inv_pp` | `TOPIC_PARTIAL_PAYMENT` | Partial payment recorded |
| `pay_rec` | `TOPIC_PAYMENT_RECORDED` | Atomic payment record stored |
| `inv_stlf` | `TOPIC_INVOICE_SETTLED_FINAL` | All payments done, invoice Settled |
| `inv_fnd` | _(inline)_ | Invoice transitions to Funded |
| `inv_meta` | _(inline)_ | Invoice metadata updated |
| `inv_mclr` | _(inline)_ | Invoice metadata cleared |

#### `inv_up` — Invoice Uploaded

```
(invoice_id: BytesN<32>, business: Address, amount: i128,
 currency: Address, due_date: u64, timestamp: u64)
```

| # | Field | Type | Description |
|---|---|---|---|
| 0 | `invoice_id` | `BytesN<32>` | SHA-256 derived unique identifier |
| 1 | `business` | `Address` | Authenticated uploading business |
| 2 | `amount` | `i128` | Invoice face value |
| 3 | `currency` | `Address` | Token contract address |
| 4 | `due_date` | `u64` | Payment deadline (Unix seconds) |
| 5 | `timestamp` | `u64` | Ledger time of upload |

#### `inv_ver` — Invoice Verified

```
(invoice_id: BytesN<32>, business: Address, timestamp: u64)
```

#### `inv_canc` — Invoice Cancelled

```
(invoice_id: BytesN<32>, business: Address, timestamp: u64)
```

#### `inv_set` — Invoice Settled

```
(invoice_id: BytesN<32>, business: Address, investor: Address,
 investor_return: i128, platform_fee: i128, timestamp: u64)
```

> `investor_return + platform_fee ≤ payment_amount` — enforced in `profits.rs`.

#### `inv_def` — Invoice Defaulted

```
(invoice_id: BytesN<32>, business: Address, investor: Address, timestamp: u64)
```

> Emitted only after the grace period has elapsed past `due_date`.

#### `inv_exp` — Invoice Expired

```
(invoice_id: BytesN<32>, business: Address, due_date: u64)
```

#### `inv_pp` — Partial Payment

```
(invoice_id: BytesN<32>, business: Address, payment_amount: i128,
 total_paid: i128, progress_bps: u32, transaction_id: String)
```

> `progress_bps` is in basis points (0–10 000). `total_paid` is monotonically increasing.

#### `pay_rec` — Payment Recorded

```
(invoice_id: BytesN<32>, payer: Address, amount: i128,
 transaction_id: String, timestamp: u64)
```

#### `inv_stlf` — Invoice Settled Final

```
(invoice_id: BytesN<32>, business: Address, investor: Address,
 total_paid: i128, timestamp: u64)
```

> Emitted exactly once per invoice when status transitions to `Settled`.

---

### Bid Events

| Topic | Symbol | Description |
|---|---|---|
| `bid_plc` | `TOPIC_BID_PLACED` | Investor places a bid |
| `bid_acc` | `TOPIC_BID_ACCEPTED` | Business accepts a bid |
| `bid_wdr` | `TOPIC_BID_WITHDRAWN` | Investor withdraws bid |
| `bid_exp` | `TOPIC_BID_EXPIRED` | Expired bid cleaned up |

#### `bid_plc` — Bid Placed

```
(bid_id: BytesN<32>, invoice_id: BytesN<32>, investor: Address,
 bid_amount: i128, expected_return: i128, timestamp: u64, expiration_timestamp: u64)
```

#### `bid_acc` — Bid Accepted

```
(bid_id: BytesN<32>, invoice_id: BytesN<32>, investor: Address,
 business: Address, bid_amount: i128, expected_return: i128, timestamp: u64)
```

#### `bid_wdr` — Bid Withdrawn

```
(bid_id: BytesN<32>, invoice_id: BytesN<32>, investor: Address,
 bid_amount: i128, timestamp: u64)
```

#### `bid_exp` — Bid Expired

```
(bid_id: BytesN<32>, invoice_id: BytesN<32>, investor: Address,
 bid_amount: i128, expiration_timestamp: u64)
```

---

### Escrow Events

| Topic | Symbol | Description |
|---|---|---|
| `esc_cr` | `TOPIC_ESCROW_CREATED` | Escrow created on bid acceptance |
| `esc_rel` | `TOPIC_ESCROW_RELEASED` | Funds released to business |
| `esc_ref` | `TOPIC_ESCROW_REFUNDED` | Funds returned to investor |

#### `esc_cr` — Escrow Created

```
(escrow_id: BytesN<32>, invoice_id: BytesN<32>, investor: Address,
 business: Address, amount: i128)
```

#### `esc_rel` — Escrow Released

```
(escrow_id: BytesN<32>, invoice_id: BytesN<32>, business: Address, amount: i128)
```

#### `esc_ref` — Escrow Refunded

```
(escrow_id: BytesN<32>, invoice_id: BytesN<32>, investor: Address, amount: i128)
```

---

### Dispute Events

| Topic | Symbol | Description |
|---|---|---|
| `dsp_cr` | _(inline)_ | Dispute opened |
| `dsp_ur` | _(inline)_ | Dispute escalated to UnderReview |
| `dsp_rs` | _(inline)_ | Dispute resolved |

#### `dsp_cr` — Dispute Created

```
(invoice_id: BytesN<32>, created_by: Address, reason: String, timestamp: u64)
```

#### `dsp_ur` — Dispute Under Review

```
(invoice_id: BytesN<32>, reviewed_by: Address, timestamp: u64)
```

#### `dsp_rs` — Dispute Resolved

```
(invoice_id: BytesN<32>, resolved_by: Address, resolution: String, timestamp: u64)
```

---

### Platform / Fee Events

| Topic | Symbol | Description |
|---|---|---|
| `fee_upd` | _(inline)_ | Platform fee configuration updated |
| `fee_rout` | _(inline)_ | Fee routed to treasury |
| `fee_cfg` | _(inline)_ | Fee bps reconfigured |
| `trs_cfg` | _(inline)_ | Treasury address configured |

#### `fee_upd` — Fee Updated

```
(fee_bps: i128, updated_at: u64, updated_by: Address)
```

---

### Audit Events

| Topic | Symbol | Description |
|---|---|---|
| `aud_val` | _(inline)_ | Audit integrity validated |
| `aud_qry` | _(inline)_ | Audit logs queried |

#### `aud_val`

```
(invoice_id: BytesN<32>, is_valid: bool, timestamp: u64)
```

#### `aud_qry`

```
(query_type: String, result_count: u32)
```

---

## Indexer Migration Guide

When upgrading to a new contract version:

1. **Check the changelog** for any new events (new topics will be documented here).
2. **Never rely on event position** in the transaction events array — filter by topic.
3. **Field positions are frozen** — you can safely deserialize by position index.
4. **New fields** will only ever appear at the end of the payload tuple.
5. **Dropped events** will never happen — topics are permanent once deployed.

---

## Running Schema Tests

```bash
cd quicklendx-contracts
cargo test test_events -- --nocapture
```

All tests in `src/test_events.rs` pin exact topic strings and payload field order.

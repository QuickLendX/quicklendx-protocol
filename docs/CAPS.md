# Protocol Capacity & Limits (CAPS)

**Audience: contributors** — this document is for people reading the contract
source and wanting to verify that enforcement behaviour matches the documented
intent. Operators who need the knobs-and-levers view should also read
[`docs/contracts/protocol-limits.md`](contracts/protocol-limits.md) and
[`docs/INVARIANTS.md`](INVARIANTS.md).

> **Hard limit** = enforced unconditionally in code; cannot be overridden by
> any runtime parameter or admin call.
>
> **Soft limit** = default that the admin can change within a documented range
> via a contract entrypoint.

---

## Table of Contents

1. [Investor Position Limits](#1-investor-position-limits)
2. [Business Supply Limits](#2-business-supply-limits)
3. [Bid Limits](#3-bid-limits)
4. [Per-Batch Limits](#4-per-batch-limits)
5. [Per-Window / TTL Limits](#5-per-window--ttl-limits)
6. [String / Metadata Field Lengths](#6-string--metadata-field-lengths)
7. [Fee Limits](#7-fee-limits)
8. [Pagination / Query Limits](#8-pagination--query-limits)
9. [Reading Limits On-Chain](#9-reading-limits-on-chain)
10. [Related Documents](#10-related-documents)

---

## 1. Investor Position Limits

Investor limits are enforced in
[`src/verification.rs`](../quicklendx-contracts/src/verification.rs) (function
`validate_investor_investment`) before every bid is accepted.

### 1.1 Aggregate exposure cap (soft)

Every verified investor has an `investment_limit` approved by the admin at KYC
time. The check is:

```
active_bid_exposure + total_invested + new_bid_amount  ≤  investment_limit
```

`investment_limit` is computed from a `base_limit` (set by the admin) that is
then scaled by a **tier multiplier** and a **risk multiplier**:

```
investment_limit = base_limit × tier_multiplier × risk_multiplier / 100
```

| Tier | Tier multiplier |
|---|---|
| `VIP` | 10× |
| `Platinum` | 5× |
| `Gold` | 3× |
| `Silver` | 2× |
| `Basic` | 1× |

| Risk level | Risk multiplier |
|---|---|
| `Low` (score 0–25) | 100 % |
| `Medium` (score 26–50) | 75 % |
| `High` (score 51–75) | 50 % |
| `VeryHigh` (score 76–100) | 25 % |

**Worked example** — admin sets `base_limit = 100_000`, investor earns `Gold`
tier with `Low` risk:

```
investment_limit = 100_000 × 3 × 100 / 100 = 300_000
```

Admin entrypoints:

- `verify_investor(admin, investor, base_limit)` — first approval
- `set_investment_limit(admin, investor, new_limit)` — update after approval
- `recompute_investor_tier(admin, investor)` — refresh after new history

Source: [`src/verification.rs` L1591–L1637](../quicklendx-contracts/src/verification.rs)

### 1.2 Per-bid caps for high-risk profiles (hard)

On top of the aggregate limit, high-risk investors face an additional per-bid
ceiling that **cannot be overridden**:

| Risk level | Per-bid hard cap |
|---|---|
| `VeryHigh` | 10 000 |
| `High` | 50 000 |
| `Low` / `Medium` | none |

These checks run after the aggregate check. Either check failing returns
`QuickLendXError::InvalidAmount`.

Source: [`src/verification.rs` L1616–L1631](../quicklendx-contracts/src/verification.rs)

### 1.3 Tier thresholds (hard constants)

An investor reaches a tier when **all four conditions** are met simultaneously
(evaluated highest-to-lowest; the first match wins):

| Tier | Max risk score | Min lifetime invested | Min successful | Max default rate |
|---|---|---|---|---|
| `VIP` | ≤ 10 | ≥ 5 000 000 | ≥ 50 | ≤ 5 % |
| `Platinum` | ≤ 20 | ≥ 1 000 000 | ≥ 20 | ≤ 10 % |
| `Gold` | ≤ 40 | ≥ 100 000 | ≥ 10 | ≤ 15 % |
| `Silver` | ≤ 60 | ≥ 10 000 | ≥ 3 | ≤ 25 % |
| `Basic` | fallback | — | — | — |

Full algorithm with worked examples: [`docs/INVESTOR_TIER.md`](INVESTOR_TIER.md)

---

## 2. Business Supply Limits

### 2.1 Maximum active invoices per business (soft, default 100)

| Parameter | Default | Disabled sentinel |
|---|---|---|
| `max_invoices_per_business` | 100 | 0 (no cap) |

Only invoices in status `Pending`, `Verified`, or `Funded` count toward the
limit. Terminal statuses (`Paid`, `Defaulted`, `Cancelled`, `Refunded`) are
excluded.

The check runs **before** the new invoice is written to storage.

```rust
// src/protocol_limits.rs
pub const MAX_ACTIVE_INVOICES_PER_BUSINESS: u32 = 100;
pub const DEFAULT_MAX_INVOICES_PER_BUSINESS: u32 = 100; // 0 = unlimited
```

Error: `QuickLendXError::MaxInvoicesPerBusinessExceeded`

Admin entrypoint: `set_protocol_limits(admin, ..., max_invoices_per_business)`

Source: [`src/protocol_limits.rs`](../quicklendx-contracts/src/protocol_limits.rs) L304, L385–L395

### 2.2 Minimum invoice amount (soft, default 1 000 000)

The minimum amount an invoice must carry, in the smallest currency unit.
For 6-decimal tokens `1_000_000` ≈ 1 whole token.

| Environment | Default |
|---|---|
| Production | 1 000 000 |
| Tests | 10 |

Validation: `amount >= min_invoice_amount` (inclusive).

Admin entrypoint: `set_protocol_limits(admin, min_invoice_amount, ...)`

Source: [`src/protocol_limits.rs`](../quicklendx-contracts/src/protocol_limits.rs) L31–L33

### 2.3 Maximum invoice due-date horizon (soft, default 365 days; hard ceiling 730)

| Parameter | Default | Hard ceiling |
|---|---|---|
| `max_due_date_days` | 365 days | 730 days (2 years) |

Validation: `due_date <= ledger_time + (max_due_date_days × 86 400)` (inclusive).

Setting `max_due_date_days = 0` or `> 730` is rejected with `InvoiceDueDateInvalid`.

Admin entrypoint: `set_protocol_limits(admin, ..., max_due_date_days, ...)`

Source: [`src/protocol_limits.rs`](../quicklendx-contracts/src/protocol_limits.rs) L109–L111,
[`src/init.rs`](../quicklendx-contracts/src/init.rs) L90

### 2.4 Invoice metadata counts (hard)

| Constraint | Hard cap | Constant |
|---|---|---|
| Tags per invoice | 10 | `MAX_INVOICE_TAG_COUNT` |
| Line items per invoice | 100 | `MAX_METADATA_LINE_ITEMS` |

Source: [`src/verification.rs`](../quicklendx-contracts/src/verification.rs) L14–L16

---

## 3. Bid Limits

### 3.1 Maximum active bids per invoice (hard)

```rust
// src/bid.rs
pub const MAX_BIDS_PER_INVOICE: u32 = 50;
```

Only `Placed` bids count. Expired or cancelled bids free a slot.
Error: `QuickLendXError::MaxBidsPerInvoiceExceeded`.

Source: [`src/bid.rs`](../quicklendx-contracts/src/bid.rs) L49

### 3.2 Maximum active bids per investor (soft, default 20)

| Parameter | Default | Disabled sentinel |
|---|---|---|
| `max_active_bids_per_investor` | 20 | 0 (no cap) |

Only `Placed` bids count toward the limit.

Source: [`src/bid.rs`](../quicklendx-contracts/src/bid.rs) L41

### 3.2.1 Per-invoice per-investor position cap (optional, hard when set)

| Parameter | Default | Disabled sentinel |
|---|---|---|
| `per_investor_position_cap` (per invoice) | unset / `None` | clear via `set_per_investor_position_cap(..., None)` |

When set, `validate_bid` / `place_bid` rejects any bid with `bid_amount > cap`
using `QuickLendXError::PerInvestorPositionCapExceeded` (1411 / `POS_CAP`).
The cap is an absolute amount in invoice currency units and must satisfy
`0 < cap ≤ invoice.amount` when configured by the business owner.

This is defence-in-depth against a whale cornering a single invoice even when
their KYC aggregate investment limit would otherwise allow a full-face bid.

Entrypoints:
- `set_per_investor_position_cap(business, invoice_id, cap)` — business only
- `get_per_investor_position_cap(invoice_id)` — public read

Source: [`src/verification.rs`](../quicklendx-contracts/src/verification.rs) (`validate_bid`),
[`src/storage.rs`](../quicklendx-contracts/src/storage.rs) (`DataKey::PerInvestorPositionCap`)

### 3.3 Minimum bid amount (soft, default 10; hard floor 1)

| Parameter | Default | Hard floor |
|---|---|---|
| `min_bid_amount` | 10 | 1 (`MIN_BID_FLOOR`) |

Effective minimum per bid: `max(min_bid_amount, invoice_amount × min_bid_bps / 10_000)`.

Admin entrypoints:
- `set_protocol_limits(admin, ..., min_bid_amount, ...)` — bulk update
- `update_minimum_bid(admin, amount)` — targeted update; enforces the floor of 1

Source: [`src/protocol_limits.rs`](../quicklendx-contracts/src/protocol_limits.rs) L36–L40, L292–L300

### 3.4 Minimum bid rate (soft, default 100 bps; hard ceiling 10 000 bps)

| Parameter | Default | Hard ceiling |
|---|---|---|
| `min_bid_bps` | 100 bps (1 %) | 10 000 bps (100 %) |

Setting `min_bid_bps > 10_000` is rejected with `InvalidAmount`.

Admin entrypoint: `set_protocol_limits(admin, ..., min_bid_bps, ...)`

Source: [`src/protocol_limits.rs`](../quicklendx-contracts/src/protocol_limits.rs) L38, L105–L107

---

## 4. Per-Batch Limits

### 4.1 Overdue-invoice scan batch (soft default 25; hard ceiling 100)

`scan_funded_invoice_expirations` uses a rotating cursor and processes at most
`limit` funded invoices per call. Re-invoke until `next_cursor == 0` to cover
the full set.

| Parameter | Default | Hard ceiling |
|---|---|---|
| Batch size | 25 | 100 |

```rust
// src/defaults.rs
pub const DEFAULT_OVERDUE_SCAN_BATCH_LIMIT: u32 = 25;
pub const MAX_OVERDUE_SCAN_BATCH_LIMIT: u32 = 100;
```

Values above 100 are **clamped** (not rejected); values below 1 are clamped to 1.

**Real call example** — drive a full sweep from off-chain code:

```rust
// Pseudocode; replace with your SDK's contract call pattern.
let grace_period_secs: u64 = 604_800; // 7 days
let batch: u32 = 50;

loop {
    let result = client.scan_funded_invoice_expirations(
        &grace_period_secs,
        &Some(batch),
    )?;
    println!(
        "scanned={} overdue={} next_cursor={}",
        result.scanned_count, result.overdue_count, result.next_cursor
    );
    if result.next_cursor == 0 {
        break;
    }
}
```

Source: [`src/defaults.rs`](../quicklendx-contracts/src/defaults.rs) L12–L14, L209–L213

---

## 5. Per-Window / TTL Limits

### 5.1 Bid TTL (soft, default 7 days; range 1–30 days)

| Parameter | Default | Hard min | Hard max |
|---|---|---|---|
| Bid TTL | 7 days | 1 day | 30 days |

```rust
// src/bid.rs
pub const DEFAULT_BID_TTL_DAYS: u64 = 7;
pub const MIN_BID_TTL_DAYS: u64 = 1;
pub const MAX_BID_TTL_DAYS: u64 = 30;
```

Setting `0` or `> 30` is rejected. Admin entrypoint: `set_bid_ttl(admin, days)`.

Source: [`src/bid.rs`](../quicklendx-contracts/src/bid.rs) L36–L38

### 5.2 Default grace period (soft, default 7 days; hard ceiling 30 days)

| Parameter | Default | Hard ceiling |
|---|---|---|
| Grace period | 7 days (604 800 s) | 30 days (2 592 000 s) |

Resolution order per call:
1. Caller-supplied override (if provided and ≤ 30 days)
2. `protocol_limits.grace_period_seconds`
3. Compile-time `DEFAULT_GRACE_PERIOD` (7 days)

Exceeding the 30-day hard cap returns `QuickLendXError::InvalidTimestamp`.

Additional cross-check: `grace_period_seconds` must not exceed
`max_due_date_days × 86 400` (the grace window must fit inside the due-date
horizon).

Admin entrypoint: `set_protocol_limits(admin, ..., grace_period_seconds, ...)`

Source: [`src/defaults.rs`](../quicklendx-contracts/src/defaults.rs) L10, L78;
[`src/protocol_limits.rs`](../quicklendx-contracts/src/protocol_limits.rs) L113–L121

### 5.3 Treasury rotation window (hard minimums)

| Parameter | Value | Constant |
|---|---|---|
| Minimum delay before confirmation | 1 day (86 400 s) | `MIN_ROTATION_DELAY_SECONDS` |
| Rotation request expiry | 7 days (604 800 s) | `ROTATION_TTL_SECONDS` |

The 1-day delay prevents same-block finalisation and gives the admin a window
to cancel an erroneous rotation.

Source: [`src/fees.rs`](../quicklendx-contracts/src/fees.rs) L21

---

## 6. String / Metadata Field Lengths

All lengths are in **bytes** (Soroban `String::len()` counts bytes). Exceeding
any cap returns `QuickLendXError::InvalidDescription`.

| Field | Hard cap (bytes) | Constant |
|---|---|---|
| Invoice description | 1 024 | `MAX_DESCRIPTION_LENGTH` |
| Customer name | 150 | `MAX_NAME_LENGTH` |
| Customer address | 300 | `MAX_ADDRESS_LENGTH` |
| Tax ID | 50 | `MAX_TAX_ID_LENGTH` |
| Notes | 2 000 | `MAX_NOTES_LENGTH` |
| Single tag | 50 | `MAX_TAG_LENGTH` |
| Transaction ID | 124 | `MAX_TRANSACTION_ID_LENGTH` |
| Dispute reason | 1 000 | `MAX_DISPUTE_REASON_LENGTH` |
| Dispute evidence | 2 000 | `MAX_DISPUTE_EVIDENCE_LENGTH` |
| Dispute resolution | 2 000 | `MAX_DISPUTE_RESOLUTION_LENGTH` |
| Notification title | 150 | `MAX_NOTIFICATION_TITLE_LENGTH` |
| Notification message | 1 000 | `MAX_NOTIFICATION_MESSAGE_LENGTH` |
| KYC data | 5 000 | `MAX_KYC_DATA_LENGTH` |
| Rejection reason | 500 | `MAX_REJECTION_REASON_LENGTH` |
| Invoice feedback | 1 000 | `MAX_FEEDBACK_LENGTH` |

All constants are declared in
[`src/protocol_limits.rs`](../quicklendx-contracts/src/protocol_limits.rs) L50–L79.

---

## 7. Fee Limits

### 7.1 Protocol fee (soft, default 2 %; hard ceiling 10 %)

| Parameter | Default | Min | Hard ceiling |
|---|---|---|---|
| `fee_bps` | 200 bps (2 %) | 0 bps | 1 000 bps (10 %) |

```rust
// src/init.rs
pub(crate) const MAX_FEE_BPS: u32 = 1000; // 10% maximum fee
const MIN_FEE_BPS: u32 = 0;
```

`fee_bps > 1000` is rejected. Admin entrypoint: `set_fee_config(admin, fee_bps)`.

Source: [`src/init.rs`](../quicklendx-contracts/src/init.rs) L88–L89

### 7.2 Platform fee (soft, default 2 %; hard ceiling 10 %)

Same ceiling as the protocol fee; enforced separately in the fee-management path.

```rust
// src/fees.rs
const MAX_PLATFORM_FEE_BPS: u32 = 1000; // 10%
```

Source: [`src/fees.rs`](../quicklendx-contracts/src/fees.rs) L17

---

## 8. Pagination / Query Limits

### 8.1 Items per paginated query (hard)

```rust
// src/pagination.rs
pub const MAX_QUERY_LIMIT: u32 = 50;
```

Every paginated read entrypoint silently **clamps** any `limit` argument to 50.
Requesting more than 50 returns at most 50 items — it never panics.

**Example** — page through all business invoices:

```rust
let mut offset: u32 = 0;
let page_size: u32 = 50; // MAX_QUERY_LIMIT

loop {
    let page = client.get_business_invoices(&business, &offset, &page_size);
    // process page …
    if (page.len() as u32) < page_size {
        break; // last or only page
    }
    offset = offset.saturating_add(page_size);
}
```

Source: [`src/pagination.rs`](../quicklendx-contracts/src/pagination.rs) L25

---

## 9. Reading Limits On-Chain

These query entrypoints expose the currently active limits in a single
round-trip so off-chain clients never have to hard-code constants.

### 9.1 `get_protocol_limits` — full limit snapshot

```rust
let limits: ProtocolLimits = client.get_protocol_limits();
// limits.min_invoice_amount       → 1_000_000 (production default)
// limits.min_bid_amount           → 10
// limits.min_bid_bps              → 100
// limits.max_due_date_days        → 365
// limits.grace_period_seconds     → 604_800  (7 days)
// limits.max_invoices_per_business→ 100
```

Returns compile-time defaults when no admin has called `set_protocol_limits`.

Source: [`src/protocol_limits.rs`](../quicklendx-contracts/src/protocol_limits.rs) L217–L229

### 9.2 `get_operational_limits` — operational ceiling snapshot

```rust
let op: OperationalLimits = client.get_operational_limits();
// op.max_batch  → MAX_OVERDUE_SCAN_BATCH_LIMIT (100)
// op.max_limit  → MAX_QUERY_LIMIT (50)
// op.max_fee    → MAX_FEE_BPS (1000)
```

Source: [`src/operational_limits.rs`](../quicklendx-contracts/src/operational_limits.rs)

### 9.3 `get_bid_ttl_config` — bid TTL snapshot

```rust
let ttl: BidTtlConfig = client.get_bid_ttl_config();
// ttl.current_days → admin-set value or DEFAULT_BID_TTL_DAYS (7)
// ttl.min_days     → MIN_BID_TTL_DAYS (1)
// ttl.max_days     → MAX_BID_TTL_DAYS (30)
// ttl.default_days → DEFAULT_BID_TTL_DAYS (7)
// ttl.is_custom    → true when admin has overridden the default
```

Source: [`src/bid.rs`](../quicklendx-contracts/src/bid.rs)

---

## 10. Related Documents

| Document | What it covers |
|---|---|
| [`docs/INVESTOR_TIER.md`](INVESTOR_TIER.md) | Full tier and risk-score algorithm with worked examples |
| [`docs/INVARIANTS.md`](INVARIANTS.md) | Protocol-wide invariants that must always hold |
| [`docs/contracts/protocol-limits.md`](contracts/protocol-limits.md) | Protocol limits API reference and admin operations |
| [`docs/contracts/bidding.md`](contracts/bidding.md) | Bid lifecycle, validation rules, and TTL semantics |
| [`docs/contracts/defaults.md`](contracts/defaults.md) | Grace-period resolution order and default flow |
| [`docs/contracts/fees.md`](contracts/fees.md) | Fee configuration, treasury rotation, and revenue split |
| [`docs/contracts/queries.md`](contracts/queries.md) | Paginated query reference with concrete examples |
| [`docs/ERROR_CODES.md`](ERROR_CODES.md) | Complete error-code catalog |

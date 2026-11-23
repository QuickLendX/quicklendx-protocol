# QuickLendX Risk Parameters Catalog

**Audience: contributors** — this document catalogs every risk-related parameter in the protocol with its minimum, maximum, and default values. It serves as a single reference for reviewers to verify that contract behavior matches documented risk intent, and for operators to understand the safety knobs available.

> **Hard parameter** = compile-time constant that cannot be changed without contract upgrade
>
> **Soft parameter** = admin-configurable value that can be updated within documented bounds via contract entrypoint

---

## Table of Contents

1. [Investor Risk Parameters](#1-investor-risk-parameters)
2. [Business Supply Risk Parameters](#2-business-supply-risk-parameters)
3. [Bid Risk Parameters](#3-bid-risk-parameters)
4. [Fee Risk Parameters](#4-fee-risk-parameters)
5. [Operational Risk Parameters](#5-operational-risk-parameters)
6. [Time-Based Risk Parameters](#6-time-based-risk-parameters)
7. [Reading Risk Parameters On-Chain](#7-reading-risk-parameters-on-chain)
8. [Related Documents](#8-related-documents)

---

## 1. Investor Risk Parameters

Investor risk parameters control exposure limits, tier qualifications, and per-bid caps to protect the protocol from concentrated positions and high-risk actors.

### 1.1 Investor Tier Thresholds (Hard)

These constants determine when an investor qualifies for each tier. All four conditions must be met simultaneously for a tier to be awarded.

| Tier | Max risk score | Min total invested | Min successful investments | Max default rate |
|------|----------------|---------------------|----------------------------|------------------|
| VIP | ≤ 10 | ≥ 5,000,000 | ≥ 50 | ≤ 5% |
| Platinum | ≤ 20 | ≥ 1,000,000 | ≥ 20 | ≤ 10% |
| Gold | ≤ 40 | ≥ 100,000 | ≥ 10 | ≤ 15% |
| Silver | ≤ 60 | ≥ 10,000 | ≥ 3 | ≤ 25% |
| Basic | fallback | — | — | — |

**Source constants:**
```rust
// src/verification.rs
const VIP_RISK_SCORE_MAX: u32 = 10;
const VIP_TOTAL_INVESTED_MIN: i128 = 5_000_000;
const VIP_SUCCESSFUL_INVESTMENTS_MIN: u32 = 50;
const VIP_DEFAULT_RATE_MAX_PCT: u32 = 5;

const PLATINUM_RISK_SCORE_MAX: u32 = 20;
const PLATINUM_TOTAL_INVESTED_MIN: i128 = 1_000_000;
const PLATINUM_SUCCESSFUL_INVESTMENTS_MIN: u32 = 20;
const PLATINUM_DEFAULT_RATE_MAX_PCT: u32 = 10;

const GOLD_RISK_SCORE_MAX: u32 = 40;
const GOLD_TOTAL_INVESTED_MIN: i128 = 100_000;
const GOLD_SUCCESSFUL_INVESTMENTS_MIN: u32 = 10;
const GOLD_DEFAULT_RATE_MAX_PCT: u32 = 15;

const SILVER_RISK_SCORE_MAX: u32 = 60;
const SILVER_TOTAL_INVESTED_MIN: i128 = 10_000;
const SILVER_SUCCESSFUL_INVESTMENTS_MIN: u32 = 3;
const SILVER_DEFAULT_RATE_MAX_PCT: u32 = 25;
```

**Source:** [`src/verification.rs`](../quicklendx-contracts/src/verification.rs) L439–L457

### 1.2 Risk Score Components (Hard)

Risk score is computed from three additive components, capped at 100.

| Component | Value range | Points added |
|-----------|-------------|--------------|
| KYC data completeness | < 100 chars | +30 |
| KYC data completeness | 100–499 chars | +20 |
| KYC data completeness | ≥ 500 chars | +10 |
| Historical default rate | 0–100% | default_rate_pct |
| Volume discount | > 1,000,000 invested | −20 |
| Volume discount | > 100,000 invested | −10 |
| Volume discount | ≤ 100,000 invested | 0 |

**Final cap:** `risk_score = min(computed_score, 100)`

**Source:** [`docs/INVESTOR_TIER.md`](INVESTOR_TIER.md) L23–L70

### 1.3 Tier Multipliers (Hard)

Multipliers applied to the admin-set `base_limit` to compute the effective `investment_limit`.

| Tier | Multiplier |
|------|------------|
| VIP | 10× |
| Platinum | 5× |
| Gold | 3× |
| Silver | 2× |
| Basic | 1× |

**Source:** [`docs/INVESTOR_TIER.md`](INVESTOR_TIER.md) L170–L178

### 1.4 Risk Level Multipliers (Hard)

Additional discount applied based on computed risk level.

| Risk level | Score range | Multiplier |
|------------|-------------|------------|
| Low | 0–25 | 100% (no reduction) |
| Medium | 26–50 | 75% |
| High | 51–75 | 50% |
| VeryHigh | 76–100 | 25% |

**Source:** [`docs/INVESTOR_TIER.md`](INVESTOR_TIER.md) L180–L188

### 1.5 Per-Bid Caps for High-Risk Investors (Hard)

On top of the aggregate investment limit, high-risk investors face per-bid ceilings that cannot be overridden.

| Risk level | Per-bid cap |
|------------|-------------|
| VeryHigh | 10,000 |
| High | 50,000 |
| Low / Medium | no additional cap |

**Source:** [`src/verification.rs`](../quicklendx-contracts/src/verification.rs) L1616–L1631

### 1.6 Maximum Active Bids Per Investor (Soft)

| Parameter | Default | Min | Max | Disabled sentinel |
|-----------|---------|-----|-----|-------------------|
| `max_active_bids_per_investor` | 20 | 0 | — | 0 |

Only `Placed` bids count toward the limit. Set to 0 to disable enforcement.

**Source constants:**
```rust
// src/bid.rs
const DEFAULT_MAX_ACTIVE_BIDS_PER_INVESTOR: u32 = 20;
pub const INVESTOR_BID_LIMIT_DISABLED: u32 = 0;
```

**Source:** [`src/bid.rs`](../quicklendx-contracts/src/bid.rs) L41–L42

---

## 2. Business Supply Risk Parameters

Business supply parameters control how many invoices a single business can have active and the minimum viable invoice size.

### 2.1 Maximum Active Invoices Per Business (Soft)

| Parameter | Default | Min | Max | Disabled sentinel |
|-----------|---------|-----|-----|-------------------|
| `max_invoices_per_business` | 100 | 0 | — | 0 |

Only invoices in status `Pending`, `Verified`, or `Funded` count toward the limit. Terminal statuses (`Paid`, `Defaulted`, `Cancelled`, `Refunded`) are excluded.

**Source constants:**
```rust
// src/protocol_limits.rs
pub const MAX_ACTIVE_INVOICES_PER_BUSINESS: u32 = 100;
pub const DEFAULT_MAX_INVOICES_PER_BUSINESS: u32 = 100; // 0 = unlimited
```

**Source:** [`src/protocol_limits.rs`](../quicklendx-contracts/src/protocol_limits.rs) L304, L47

### 2.2 Minimum Invoice Amount (Soft)

| Parameter | Default (prod) | Default (test) | Min |
|-----------|----------------|----------------|-----|
| `min_invoice_amount` | 1,000,000 | 10 | > 0 |

For 6-decimal tokens, `1_000_000` ≈ 1 whole token. Validation is inclusive: `amount >= min_invoice_amount`.

**Source constants:**
```rust
// src/protocol_limits.rs
#[cfg(not(test))]
const DEFAULT_MIN_AMOUNT: i128 = 1_000_000; // 1 token (6 decimals)
#[cfg(test)]
const DEFAULT_MIN_AMOUNT: i128 = 10;
```

**Source:** [`src/protocol_limits.rs`](../quicklendx-contracts/src/protocol_limits.rs) L31–L33

### 2.3 Maximum Invoice Due-Date Horizon (Soft with Hard Ceiling)

| Parameter | Default | Hard ceiling | Validation |
|-----------|---------|--------------|------------|
| `max_due_date_days` | 365 days | 730 days (2 years) | 0 < value ≤ 730 |

Validation: `due_date <= ledger_time + (max_due_date_days × 86_400)` (inclusive).

**Source constants:**
```rust
// src/protocol_limits.rs
const DEFAULT_MAX_DUE_DAYS: u64 = 365;
```

**Validation:** [`src/protocol_limits.rs`](../quicklendx-contracts/src/protocol_limits.rs) L111–L113

### 2.4 Invoice Metadata Counts (Hard)

| Constraint | Hard cap | Constant |
|------------|----------|----------|
| Tags per invoice | 10 | `MAX_INVOICE_TAG_COUNT` |
| Line items per invoice | 100 | `MAX_METADATA_LINE_ITEMS` |

**Source constants:**
```rust
// src/verification.rs
pub const MAX_INVOICE_TAG_COUNT: u32 = 10;
pub const MAX_METADATA_LINE_ITEMS: u32 = 100;
```

**Source:** [`src/verification.rs`](../quicklendx-contracts/src/verification.rs) L14–L16

---

## 3. Bid Risk Parameters

Bid risk parameters control bid sizing, TTL, and concentration limits to prevent excessive exposure and stale bids.

### 3.1 Maximum Active Bids Per Invoice (Hard)

| Parameter | Value | Constant |
|-----------|-------|----------|
| `MAX_BIDS_PER_INVOICE` | 50 | `MAX_BIDS_PER_INVOICE` |

Only `Placed` bids count. Expired or cancelled bids free a slot.

**Source constants:**
```rust
// src/bid.rs
pub const MAX_BIDS_PER_INVOICE: u32 = 50;
```

**Source:** [`src/bid.rs`](../quicklendx-contracts/src/bid.rs) L49

### 3.2 Minimum Bid Amount (Soft with Hard Floor)

| Parameter | Default | Hard floor | Validation |
|-----------|---------|------------|------------|
| `min_bid_amount` | 10 | 1 (`MIN_BID_FLOOR`) | amount ≥ floor |

Effective minimum per bid: `max(min_bid_amount, invoice_amount × min_bid_bps / 10_000)`.

**Source constants:**
```rust
// src/protocol_limits.rs
pub const DEFAULT_MIN_BID_AMOUNT: i128 = 10;
pub const MIN_BID_FLOOR: i128 = 1;
```

**Source:** [`src/protocol_limits.rs`](../quicklendx-contracts/src/protocol_limits.rs) L36–L40

### 3.3 Minimum Bid Rate (Soft with Hard Ceiling)

| Parameter | Default | Hard ceiling | Validation |
|-----------|---------|--------------|------------|
| `min_bid_bps` | 100 bps (1%) | 10,000 bps (100%) | 0 < value ≤ 10,000 |

**Source constants:**
```rust
// src/protocol_limits.rs
pub const DEFAULT_MIN_BID_BPS: u32 = 100; // 1%
```

**Validation:** [`src/protocol_limits.rs`](../quicklendx-contracts/src/protocol_limits.rs) L107–L109

### 3.4 Bid TTL (Soft with Hard Bounds)

| Parameter | Default | Hard min | Hard max | Validation |
|-----------|---------|----------|----------|------------|
| Bid TTL | 7 days | 1 day | 30 days | 1 ≤ value ≤ 30 |

**Source constants:**
```rust
// src/bid.rs
pub const DEFAULT_BID_TTL_DAYS: u64 = 7;
pub const MIN_BID_TTL_DAYS: u64 = 1;
pub const MAX_BID_TTL_DAYS: u64 = 30;
```

**Source:** [`src/bid.rs`](../quicklendx-contracts/src/bid.rs) L36–L38

---

## 4. Fee Risk Parameters

Fee parameters control the maximum fees the protocol can charge to prevent excessive rent extraction.

### 4.1 Protocol Fee (Soft with Hard Ceiling)

| Parameter | Default | Min | Hard ceiling | Validation |
|-----------|---------|-----|--------------|------------|
| `fee_bps` | 200 bps (2%) | 0 bps | 1,000 bps (10%) | 0 ≤ value ≤ 1,000 |

**Source constants:**
```rust
// src/init.rs
pub(crate) const MAX_FEE_BPS: u32 = 1000; // 10% maximum fee
const MIN_FEE_BPS: u32 = 0;
```

**Source:** [`src/init.rs`](../quicklendx-contracts/src/init.rs) L88–L89

### 4.2 Platform Fee (Soft with Hard Ceiling)

| Parameter | Default | Hard ceiling | Validation |
|-----------|---------|--------------|------------|
| Platform fee | 200 bps (2%) | 1,000 bps (10%) | value ≤ 1,000 |

**Source constants:**
```rust
// src/fees.rs
const DEFAULT_PLATFORM_FEE_BPS: u32 = 200; // 2%
const MAX_PLATFORM_FEE_BPS: u32 = 1000; // 10%
```

**Source:** [`src/fees.rs`](../quicklendx-contracts/src/fees.rs) L16–L17

### 4.3 Treasury Rotation Timing (Hard)

| Parameter | Value | Constant |
|-----------|-------|----------|
| Minimum delay before confirmation | 1 day (86,400 s) | `MIN_ROTATION_DELAY_SECONDS` |
| Rotation request expiry | 7 days (604,800 s) | `ROTATION_TTL_SECONDS` |

The 1-day delay prevents same-block finalisation and gives the admin a window to cancel an erroneous rotation.

**Source constants:**
```rust
// src/fees.rs
pub const MIN_ROTATION_DELAY_SECONDS: u64 = 86_400; // 1 day
const ROTATION_TTL_SECONDS: u64 = 604_800; // 7 days
```

**Source:** [`src/fees.rs`](../quicklendx-contracts/src/fees.rs) L18–L21

---

## 5. Operational Risk Parameters

Operational parameters control batch sizes and pagination limits to prevent gas exhaustion and denial-of-service.

### 5.1 Overdue Invoice Scan Batch (Soft with Hard Ceiling)

| Parameter | Default | Hard ceiling | Behavior |
|-----------|---------|--------------|----------|
| Batch size | 25 | 100 | Clamped to [1, 100] |

Values above 100 are clamped (not rejected); values below 1 are clamped to 1.

**Source constants:**
```rust
// src/defaults.rs
pub const DEFAULT_OVERDUE_SCAN_BATCH_LIMIT: u32 = 25;
pub const MAX_OVERDUE_SCAN_BATCH_LIMIT: u32 = 100;
```

**Source:** [`src/defaults.rs`](../quicklendx-contracts/src/defaults.rs) L12–L14

### 5.2 Pagination Query Limit (Hard)

| Parameter | Value | Constant |
|-----------|-------|----------|
| `MAX_QUERY_LIMIT` | 50 | `MAX_QUERY_LIMIT` |

Every paginated read entrypoint silently clamps any `limit` argument to 50. Requesting more than 50 returns at most 50 items — it never panics.

**Source constants:**
```rust
// src/pagination.rs
pub const MAX_QUERY_LIMIT: u32 = 50;
```

**Source:** [`src/pagination.rs`](../quicklendx-contracts/src/pagination.rs) L25

### 5.3 Batch Invoice Creation (Hard)

| Parameter | Value | Constant |
|-----------|-------|----------|
| `MAX_BATCH_INVOICES` | 10 | `MAX_BATCH_INVOICES` |

Caps the size of a `store_invoices_batch` submission to prevent a single transaction from consuming unbounded CPU and storage budget.

**Source constants:**
```rust
// src/protocol_limits.rs
pub const MAX_BATCH_INVOICES: u32 = 10;
```

**Source:** [`src/protocol_limits.rs`](../quicklendx-contracts/src/protocol_limits.rs) L314

---

## 6. Time-Based Risk Parameters

Time-based parameters control grace periods, expiry windows, and other time-sensitive risk controls.

### 6.1 Default Grace Period (Soft with Hard Ceiling)

| Parameter | Default | Hard ceiling | Validation |
|-----------|---------|--------------|------------|
| Grace period | 7 days (604,800 s) | 30 days (2,592,000 s) | value ≤ 2,592,000 |

Resolution order per call:
1. Caller-supplied override (if provided and ≤ 30 days)
2. `protocol_limits.grace_period_seconds`
3. Compile-time `DEFAULT_GRACE_PERIOD` (7 days)

Exceeding the 30-day hard cap returns `QuickLendXError::InvalidTimestamp`.

**Source constants:**
```rust
// src/protocol_limits.rs
const DEFAULT_GRACE_PERIOD: u64 = 7 * 24 * 60 * 60; // 7 days

// src/defaults.rs
pub const DEFAULT_GRACE_PERIOD: u64 = 7 * 24 * 60 * 60;
const MAX_GRACE_PERIOD: u64 = 30 * 24 * 60 * 60;
```

**Source:** [`src/protocol_limits.rs`](../quicklendx-contracts/src/protocol_limits.rs) L45; [`src/defaults.rs`](../quicklendx-contracts/src/defaults.rs) L10, L78

### 6.2 Grace Period Cross-Check (Soft)

The grace period must fit within the allowed due-date window:

```
grace_period_seconds ≤ max_due_date_days × 86,400
```

This prevents setting a grace period that extends beyond the maximum invoice horizon.

**Validation:** [`src/protocol_limits.rs`](../quicklendx-contracts/src/protocol_limits.rs) L119–L123

---

## 7. Reading Risk Parameters On-Chain

These query entrypoints expose the currently active risk parameters in a single round-trip so off-chain clients never have to hard-code constants.

### 7.1 `get_protocol_limits` — Full Limit Snapshot

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

**Source:** [`src/protocol_limits.rs`](../quicklendx-contracts/src/protocol_limits.rs) L217–L229

### 7.2 `get_bid_ttl_config` — Bid TTL Snapshot

```rust
let ttl: BidTtlConfig = client.get_bid_ttl_config();
// ttl.current_days → admin-set value or DEFAULT_BID_TTL_DAYS (7)
// ttl.min_days     → MIN_BID_TTL_DAYS (1)
// ttl.max_days     → MAX_BID_TTL_DAYS (30)
// ttl.default_days → DEFAULT_BID_TTL_DAYS (7)
// ttl.is_custom    → true when admin has overridden the default
```

**Source:** [`src/bid.rs`](../quicklendx-contracts/src/bid.rs)

### 7.3 `get_operational_limits` — Operational Ceiling Snapshot

```rust
let op: OperationalLimits = client.get_operational_limits();
// op.max_batch  → MAX_OVERDUE_SCAN_BATCH_LIMIT (100)
// op.max_limit  → MAX_QUERY_LIMIT (50)
// op.max_fee    → MAX_FEE_BPS (1000)
```

**Source:** [`src/operational_limits.rs`](../quicklendx-contracts/src/operational_limits.rs)

### 7.4 `get_investor_verification` — Investor Risk Snapshot

```rust
let verification: InvestorVerification = client.get_investor_verification(&investor)?;
// verification.tier        → InvestorTier (Basic/Silver/Gold/Platinum/VIP)
// verification.risk_level  → InvestorRiskLevel (Low/Medium/High/VeryHigh)
// verification.risk_score  → u32 (0–100)
// verification.investment_limit → i128 (effective limit after multipliers)
```

**Source:** [`src/verification.rs`](../quicklendx-contracts/src/verification.rs)

---

## 8. Related Documents

| Document | What it covers |
|----------|----------------|
| [`docs/CAPS.md`](CAPS.md) | Complete protocol capacity and limits catalog (investor, business, bid, batch, TTL, string lengths, fees, pagination) |
| [`docs/INVESTOR_TIER.md`](INVESTOR_TIER.md) | Full investor tier and risk-score algorithm with worked examples |
| [`docs/INVARIANTS.md`](INVARIANTS.md) | Protocol-wide invariants that must always hold |
| [`docs/contracts/protocol-limits.md`](contracts/protocol-limits.md) | Protocol limits API reference and admin operations |
| [`docs/ERROR_CODES.md`](ERROR_CODES.md) | Complete error-code catalog |

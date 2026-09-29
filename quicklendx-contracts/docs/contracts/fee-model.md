# QuickLendX — Fee & Revenue-Distribution Model

This guide explains how the QuickLendX protocol calculates fees, distributes profit, and routes revenue. It covers the two core modules:

- **`profits.rs`** – computes the investor/platform split at settlement time.
- **`fees.rs`** – manages fee structures, volume tiers, timing modifiers, treasury routing, and revenue distribution.

By the end, you will understand exactly where every unit of value goes when a borrower settles an invoice.

---

## Table of Contents

1. [Core Constants](#1-core-constants)
2. [The Settlement Flow (End-to-End)](#2-the-settlement-flow-end-to-end)
   - [Stage 1: Profit Splitting](#21-stage-1-profit-splitting-profitsrs)
   - [Stage 2: Fee Collection & Revenue Distribution](#22-stage-2-fee-collection--revenue-distribution-feesrs)
3. [Volume-Tier Discounts & Timing Modifiers](#3-volume-tier-discounts--timing-modifiers)
4. [Worked Numeric Example](#4-worked-numeric-example)
5. [Summary of Functions](#5-summary-of-functions--where-to-use-them)
6. [Integration Notes for Developers](#6-integration-notes-for-developers)
7. [Frequently Asked Questions](#7-frequently-asked-questions)

---

## 1. Core Constants

All percentages are expressed in **basis points (bps)**, where `10_000 bps = 100%`.

### From `profits.rs`

```rust
pub const DEFAULT_PLATFORM_FEE_BPS: i128 = 200;   // 2%
pub const MAX_PLATFORM_FEE_BPS: i128   = 1_000;   // 10%
pub const BPS_DENOMINATOR: i128        = 10_000;
```

### From `fees.rs`

```rust
const MAX_FEE_BPS: u32 = 1000;                   // 10% hard cap for all fees
const BPS_DENOMINATOR: i128 = 10_000;
const DEFAULT_PLATFORM_FEE_BPS: u32 = 200;       // 2%
const MAX_PLATFORM_FEE_BPS: u32 = 1000;          // 10%
const EARLY_PLATFORM_DISCOUNT_BPS: i128 = 1_000; // 10%
const LATE_FEE_SURCHARGE_BPS: i128 = 2_000;      // 20%
const ROTATION_TTL_SECONDS: u64 = 604_800;       // 7 days
```

---

## 2. The Settlement Flow (End-to-End)

When a borrower pays an invoice, the protocol processes the funds in two logical stages:

1. **Profit splitting** – determine how much of the payment returns to the investor and how much the platform keeps as a fee.
2. **Revenue distribution** – split the platform fee among treasury, developer, and protocol treasury according to the configured shares.

### 2.1 Stage 1: Profit Splitting (`profits.rs`)

The entry point is `PlatformFee::calculate(env, investment_amount, payment_amount)`.

**Inputs:**

- `investment_amount` – the principal the investor provided.
- `payment_amount` – the total payment received from the borrower.

**Algorithm** (pseudo-code from `calculate_with_fee_bps_checked`):

```
if payment_amount <= investment_amount:
    investor_return = payment_amount
    platform_fee = 0
else:
    gross_profit = payment_amount - investment_amount
    platform_fee = floor(gross_profit * fee_bps / 10_000)   // fee_bps from storage
    investor_return = payment_amount - platform_fee
```

The fee rate (`fee_bps`) is read from the platform fee configuration stored in `profits.rs` (key `plt_fee`), which defaults to `DEFAULT_PLATFORM_FEE_BPS = 200` (2%) unless updated by an admin.

> **Invariant:** `investor_return + platform_fee == payment_amount` — no dust is lost; rounding down of the fee leaves the remainder to the investor.

The result is a tuple `(investor_return, platform_fee)`.

> **Note:** `profits.rs` also provides `calculate_breakdown` for full transparency, returning a `ProfitFeeBreakdown` struct with all intermediate values.

---

### 2.2 Stage 2: Fee Collection & Revenue Distribution (`fees.rs`)

The `platform_fee` from Stage 1 is now treated as platform revenue. It is collected and eventually distributed according to the `RevenueConfig`.

#### 2.2.1 Fee Collection (`collect_fees`)

Fees are aggregated per time period (monthly). The `collect_fees` function accumulates the platform fee into a `RevenueData` struct for the current period, associating it with the user who paid it (for volume tracking).

#### 2.2.2 Revenue Distribution (`distribute_revenue`)

An admin (or automated process) calls `distribute_revenue(env, admin, period)` to split the accumulated pending fees for a specific period.

**`RevenueConfig`** (stored under `rev_cfg`) defines the shares:

```rust
pub struct RevenueConfig {
    pub treasury_address: Address,
    pub treasury_share_bps: u32,     // share going to treasury
    pub developer_share_bps: u32,    // share going to developer wallet
    pub platform_share_bps: u32,     // share going to protocol platform
    pub auto_distribution: bool,
    pub min_distribution_amount: i128,
}
```

> **Validation:** The three shares must sum to exactly `10_000 bps` (100%). For example:
>
> - `treasury_share_bps = 4000` (40%)
> - `developer_share_bps = 3000` (30%)
> - `platform_share_bps = 3000` (30%)

**Distribution algorithm** (from `distribute_revenue`):

```
amount = pending_distribution

treasury_amount  = floor(amount * treasury_share_bps / 10_000)
developer_amount = floor(amount * developer_share_bps / 10_000)
platform_amount  = amount - treasury_amount - developer_amount   // remainder
```

This guarantees that `treasury_amount + developer_amount + platform_amount == amount` (no dust loss; the platform receives any rounding remainder).

The amounts are then transferred to their respective addresses (the `treasury_address` from config, the developer address, and the platform address). The protocol also emits a `revenue_distributed` event for audit.

---

## 3. Volume-Tier Discounts & Timing Modifiers

The `fees.rs` module also provides a more detailed fee calculation that can be applied before the settlement profit split. It is used when the platform wants to offer discounts based on borrower volume or early/late payment behaviour.

### 3.1 Volume Tiers

**`VolumeTier` enum:**

```rust
pub enum VolumeTier {
    Standard,   // 0% discount
    Silver,     // 5% discount
    Gold,       // 10% discount
    Platinum,   // 15% discount
}
```

The tier is determined by a user's cumulative transaction volume (tracked in `UserVolumeData`).

**Thresholds** (from `update_user_volume`):

| Tier     | Minimum Volume (stroops) | Discount |
| -------- | ------------------------ | -------- |
| Platinum | ≥ 1,000,000,000,000      | 15%      |
| Gold     | ≥ 500,000,000,000        | 10%      |
| Silver   | ≥ 100,000,000,000        | 5%       |
| Standard | Otherwise                | 0%       |

The discount is applied as a percentage reduction (in bps) to the **fee amount** (not the rate). For example, if the fee is 2% and the user is Silver (5% discount), the effective fee rate becomes `2% × (1 − 0.05) = 1.9%`.

### 3.2 Early / Late Payment Modifiers

| Event         | Modifier                                                                      |
| ------------- | ----------------------------------------------------------------------------- |
| Early payment | 10% discount (`EARLY_PLATFORM_DISCOUNT_BPS = 1_000`) on the platform fee      |
| Late payment  | 20% surcharge (`LATE_FEE_SURCHARGE_BPS = 2_000`) on the late payment fee type |

These modifiers are applied in `calculate_total_fees`, which processes fees in this order:

1. Raw fee per active fee structure.
2. Clamp to min/max bounds.
3. Apply volume-tier discount to all fees except `LatePayment`.
4. Apply early discount to `Platform` fee if early payment.
5. Apply late surcharge to `LatePayment` fee if late payment.

> The resulting total fee can be used as the platform fee in the profit split. Note that `profits.rs` does **not** automatically incorporate these modifiers — integrators must call `calculate_total_fees` first.

---

## 4. Worked Numeric Example

### Assumptions

| Parameter         | Value                          |
| ----------------- | ------------------------------ |
| Investment        | 1,000,000 stroops              |
| Payment           | 1,100,000 stroops (10% return) |
| Platform fee rate | 200 bps (2%)                   |
| Treasury share    | 4,000 bps (40%)                |
| Developer share   | 3,000 bps (30%)                |
| Platform share    | 3,000 bps (30%)                |
| Volume tier       | Silver (5% discount)           |
| Payment timing    | Early (10% discount)           |

---

### Step 1: Profit Splitting (Base, without modifiers)

```
gross_profit       = 1,100,000 − 1,000,000 = 100,000
base_platform_fee  = floor(100,000 × 200 / 10,000) = 2,000
investor_return    = 1,100,000 − 2,000 = 1,098,000
```

---

### Step 2: Apply Volume-Tier and Early-Payment Discounts

**Volume discount (Silver = 5%):**

```
discount           = floor(2,000 × 500 / 10,000) = 100
fee_after_volume   = 2,000 − 100 = 1,900
```

**Early payment discount (10%):**

```
early_discount     = floor(1,900 × 1,000 / 10,000) = 190
final_platform_fee = 1,900 − 190 = 1,710
```

Investor gets: `1,100,000 − 1,710 = 1,098,290 stroops`

---

### Step 3: Revenue Distribution

```
treasury_amount  = floor(1,710 × 4,000 / 10,000) = 684
developer_amount = floor(1,710 × 3,000 / 10,000) = 513
platform_amount  = 1,710 − 684 − 513 = 513
```

**Verification:** `684 + 513 + 513 = 1,710` ✅

---

### Final Distribution

| Recipient | Amount (stroops) |
| --------- | ---------------- |
| Investor  | 1,098,290        |
| Treasury  | 684              |
| Developer | 513              |
| Platform  | 513              |
| **Total** | **1,100,000**    |

---

## 5. Summary of Functions & Where to Use Them

| Module       | Function                               | Purpose                                                                                                               |
| ------------ | -------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| `profits.rs` | `PlatformFee::calculate`               | Core settlement split: given investment and payment, returns `(investor_return, platform_fee)` using stored fee rate. |
| `profits.rs` | `PlatformFee::calculate_breakdown`     | Returns full breakdown struct for transparency.                                                                       |
| `fees.rs`    | `FeeManager::calculate_total_fees`     | Computes total fees after volume discounts and timing modifiers; useful for dynamic fee rates.                        |
| `fees.rs`    | `FeeManager::collect_fees`             | Adds platform fee to period revenue and updates user volume.                                                          |
| `fees.rs`    | `FeeManager::distribute_revenue`       | Splits pending fees according to `RevenueConfig` and transfers to recipients.                                         |
| `fees.rs`    | `FeeManager::get_revenue_split_config` | Retrieves current `RevenueConfig`.                                                                                    |
| `fees.rs`    | `FeeManager::route_platform_fee`       | Directly transfers a single fee to treasury (or contract if unset).                                                   |

---

## 6. Integration Notes for Developers

**Setting the platform fee rate:**
Use `PlatformFee::set_config` (requires admin auth) to update the fee bps stored in `profits.rs`. This affects all future settlement calculations.

**Configuring revenue split:**
Call `FeeManager::configure_revenue_distribution` with a `RevenueConfig` where shares sum to `10_000 bps`.

**Applying volume discounts:**
Before calling `PlatformFee::calculate`, compute the effective fee using `FeeManager::calculate_total_fees`, then either:

- Convert the resulting total fee to an effective bps rate:

  ```
  effective_bps = floor(total_fee * 10_000 / gross_profit)   // if gross_profit > 0
  ```

  Then call `PlatformFee::calculate_with_fee_bps` with that rate, **or**

- Simply subtract the discounts after the base fee calculation (as shown in the worked example).

**Treasury routing:**
The protocol supports a two-step rotation for the treasury address (`initiate_treasury_rotation` / `confirm_treasury_rotation`) to prevent misrouting.

**Audit:**
Always verify that `investor_return + platform_fee == payment_amount` after calculations; this invariant is guaranteed by the floor-division approach.

---

## 7. Frequently Asked Questions

**Q: Why are there two separate platform fee configurations (one in `profits.rs` and one in `fees.rs`)?**

A: The `profits.rs` config is the active fee rate used in settlement. The `fees.rs` config stores a separate copy that may be used for other purposes (e.g., treasury routing). They are intended to be kept in sync by the admin; future versions may unify them.

---

**Q: How is the volume tier determined?**

A: It is based on the total transaction volume (in stroops) of the borrower, updated each time `collect_fees` is called.

---

**Q: What happens if the platform fee is zero?**

A: No revenue is collected, and `distribute_revenue` will return an `OperationNotAllowed` error if called with zero pending.

---

**Q: Can I see the exact math behind the rounding?**

A: All divisions use integer **floor division** (truncation toward zero). The remainder is always kept by the investor or the platform (in the case of revenue split, the platform receives the remainder).

This guide should give you a complete understanding of how fees flow through the QuickLendX protocol. For further questions, join the Discord community.

# Investor Rating and Tier Algorithm

**Audience: contributors** — this document is for people reading the contract source and wanting to verify the implementation against the documented intent. Operators and integrators should start from [`docs/contracts/investor-kyc.md`](contracts/investor-kyc.md).

All logic described here lives in [`quicklendx-contracts/src/verification.rs`](../quicklendx-contracts/src/verification.rs).

---

## Overview

Every verified investor carries three derived values that are updated together whenever new information arrives:

| Field | Type | Purpose |
|---|---|---|
| `risk_score` | `u32` (0–100) | Composite score; lower is better |
| `risk_level` | `InvestorRiskLevel` | Coarse band derived from `risk_score` |
| `tier` | `InvestorTier` | Privilege level; governs the `investment_limit` multiplier |

These are computed deterministically from the investor's stored performance counters (`total_invested`, `successful_investments`, `defaulted_investments`) and their KYC data. The same counters always yield the same tier — the mapping is stable and idempotent.

---

## Step 1 — Risk Score (`calculate_investor_risk_score`)

The risk score is assembled from three additive components and then capped at 100.

### Component A: KYC data completeness

| KYC string length | Points added |
|---|---|
| < 100 characters | +30 |
| 100 – 499 characters | +20 |
| ≥ 500 characters | +10 |

Rationale: longer KYC submissions are treated as more thorough due diligence.

### Component B: Historical default rate

```
default_rate_pct = (defaulted_investments * 100) / total_investments
```

`default_rate_pct` points are added to the score. Only applied when `total_investments > 0`.

### Component C: Volume discount

| Lifetime invested | Adjustment |
|---|---|
| > 1,000,000 | −20 |
| > 100,000 | −10 |
| ≤ 100,000 | none |

The discount acknowledges that high-volume investors have demonstrated sustained commitment.

### Final cap

```
risk_score = min(risk_score, 100)
```

### Worked example

An investor with:
- Comprehensive KYC (600 chars) → +10
- 20 successful, 2 defaulted (default rate = 2/22 × 100 = 9%) → +9
- Total invested: 500,000 → −10

```
risk_score = 10 + 9 − 10 = 9
```

---

## Step 2 — Risk Level (`determine_risk_level`)

The risk level maps the numeric score to a coarse band:

| Band | Score range |
|---|---|
| `Low` | 0 – 25 |
| `Medium` | 26 – 50 |
| `High` | 51 – 75 |
| `VeryHigh` | 76 – 100 |

---

## Step 3 — Investor Tier (`compute_investor_tier`)

Tiers are evaluated from highest to lowest. The investor receives the **first tier whose every condition is satisfied**; if none match, they stay at `Basic`.

All four conditions must hold simultaneously for a tier to be awarded.

| Tier | Max risk score | Min total invested | Min successful investments | Max default rate |
|---|---|---|---|---|
| `VIP` | ≤ 10 | ≥ 5,000,000 | ≥ 50 | ≤ 5% |
| `Platinum` | ≤ 20 | ≥ 1,000,000 | ≥ 20 | ≤ 10% |
| `Gold` | ≤ 40 | ≥ 100,000 | ≥ 10 | ≤ 15% |
| `Silver` | ≤ 60 | ≥ 10,000 | ≥ 3 | ≤ 25% |
| `Basic` | — | — | — | — |

The threshold constants in source:

```rust
// src/verification.rs, lines 386-404
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

### Worked example — VIP

Investor counters:
- `total_invested` = 6,000,000
- `successful_investments` = 60
- `defaulted_investments` = 2
- `risk_score` = 9 (from Step 1 example above)

Check VIP:
- risk_score 9 ≤ 10 ✓
- 6,000,000 ≥ 5,000,000 ✓
- 60 ≥ 50 ✓
- default rate = 2/62 × 100 ≈ 3.2% ≤ 5% ✓

→ **VIP**

### Worked example — Gold (fails VIP and Platinum)

Investor counters:
- `total_invested` = 200,000
- `successful_investments` = 15
- `defaulted_investments` = 1
- `risk_score` = 35

Check VIP: risk_score 35 > 10 ✗ → skip  
Check Platinum: risk_score 35 > 20 ✗ → skip  
Check Gold:
- risk_score 35 ≤ 40 ✓
- 200,000 ≥ 100,000 ✓
- 15 ≥ 10 ✓
- default rate = 1/16 × 100 ≈ 6.25% ≤ 15% ✓

→ **Gold**

---

## Step 4 — Investment Limit (`calculate_investment_limit`)

The admin supplies a `base_limit` when approving a KYC record. The effective limit is derived by applying the tier multiplier and a risk discount:

```
investment_limit = floor( base_limit × tier_multiplier × risk_multiplier / 100 )
```

### Tier multipliers

| Tier | Multiplier |
|---|---|
| `VIP` | 10× |
| `Platinum` | 5× |
| `Gold` | 3× |
| `Silver` | 2× |
| `Basic` | 1× |

### Risk multipliers

| Risk level | Multiplier |
|---|---|
| `Low` | 100% (no reduction) |
| `Medium` | 75% |
| `High` | 50% |
| `VeryHigh` | 25% |

### Worked examples

| base_limit | Tier | Risk level | Effective limit |
|---|---|---|---|
| 100,000 | VIP | Low | 100,000 × 10 × 100/100 = **1,000,000** |
| 100,000 | VIP | Medium | 100,000 × 10 × 75/100 = **750,000** |
| 100,000 | Platinum | Low | 100,000 × 5 × 100/100 = **500,000** |
| 100,000 | Gold | High | 100,000 × 3 × 50/100 = **150,000** |
| 100,000 | Silver | VeryHigh | 100,000 × 2 × 25/100 = **50,000** |
| 100,000 | Basic | Low | 100,000 × 1 × 100/100 = **100,000** |

---

## Per-Bid Caps (risk-level gates)

In addition to the aggregate `investment_limit`, high-risk investors face hard per-bid caps enforced in `validate_investor_investment`:

| Risk level | Per-bid cap |
|---|---|
| `VeryHigh` | 10,000 |
| `High` | 50,000 |
| `Low` / `Medium` | no additional cap |

The aggregate check runs first:

```
total_exposure = active_bid_exposure + total_invested + new_bid_amount
assert total_exposure ≤ investment_limit
```

Then the per-bid cap check applies on top.

---

## When Tiers Are Recomputed

| Trigger | Function | Who calls it |
|---|---|---|
| KYC approval | `verify_investor` | Admin |
| Investment settled (success or default) | `update_investor_analytics` | Contract internally |
| Explicit admin refresh | `recompute_investor_tier` | Admin |

`recompute_investor_tier` is idempotent: calling it twice with the same stored counters produces the same outcome. It recovers the original `base_limit` from the current `investment_limit` before reapplying the updated multipliers, so the admin-approved baseline is never lost.

---

## Entrypoints

The public contract functions that read or mutate these values:

```rust
// Read
fn get_investor_verification(env: Env, investor: Address) -> Option<InvestorVerification>
fn get_investor_analytics(env: Env, investor: Address) -> Result<InvestorVerification, QuickLendXError>
fn get_investors_by_tier(env: Env, tier: InvestorTier) -> Vec<Address>

// Mutate
fn verify_investor(env: Env, admin: Address, investor: Address, investment_limit: i128)
fn recompute_investor_tier(env: Env, admin: Address, investor: Address)
fn set_investment_limit(env: Env, admin: Address, investor: Address, new_limit: i128)
```

---

## Quick decision tree

```
risk_score computed from KYC + history
        │
        ├─ ≤ 10 AND total_invested ≥ 5M AND successful ≥ 50 AND default_rate ≤ 5%
        │         → VIP  (10× limit)
        │
        ├─ ≤ 20 AND total_invested ≥ 1M AND successful ≥ 20 AND default_rate ≤ 10%
        │         → Platinum  (5× limit)
        │
        ├─ ≤ 40 AND total_invested ≥ 100K AND successful ≥ 10 AND default_rate ≤ 15%
        │         → Gold  (3× limit)
        │
        ├─ ≤ 60 AND total_invested ≥ 10K AND successful ≥ 3 AND default_rate ≤ 25%
        │         → Silver  (2× limit)
        │
        └─ otherwise
                  → Basic  (1× limit)

risk_level from risk_score
        0-25 → Low (100%)  |  26-50 → Medium (75%)
        51-75 → High (50%) |  76-100 → VeryHigh (25%)

investment_limit = base_limit × tier_multiplier × risk_multiplier / 100
```

---

## Related documents

- [`docs/contracts/investor-kyc.md`](contracts/investor-kyc.md) — KYC lifecycle, verification flow, error reference
- [`docs/contracts/verification.md`](contracts/verification.md) — Verification module reference
- [`docs/contracts/limits.md`](contracts/limits.md) — Protocol-wide limit configuration
- Source: [`quicklendx-contracts/src/verification.rs`](../quicklendx-contracts/src/verification.rs)
- Tests: [`quicklendx-contracts/src/test_risk_tier.rs`](../quicklendx-contracts/src/test_risk_tier.rs), [`quicklendx-contracts/src/test_investor_kyc.rs`](../quicklendx-contracts/src/test_investor_kyc.rs)

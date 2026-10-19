# Investor KYC Tiers, Requirements, and Gating Operations

**Audience: Operators and Support Teams** — This document is for platform operators and support staff who manage investor onboarding, risk ratings, and limit overrides. For the developer guide on verification contract logic, see [`docs/INVESTOR_TIER.md`](INVESTOR_TIER.md).

---

## Overview

QuickLendX implements an automated, multi-tiered Know Your Customer (KYC) and risk rating framework for investors. This framework dynamically regulates the maximum investment limits and enforces transactional gates at the smart contract level to mitigate default risk and maintain regulatory compliance.

Every investor profile holds three key derived classification values:
1. **Investor Tier** (`InvestorTier`): Governs the investment limit multiplier.
2. **Investor Risk Level** (`InvestorRiskLevel`): Adjusts the effective investment limit and enforces per-bid transactional caps.
3. **Risk Score** (`u32`): A composite score between 0 and 100 derived from KYC completeness and historical portfolio performance.

---

## 1. Investor Tiers & Performance Requirements

Tiers are recomputed dynamically when an investor's KYC is verified or when an investment settles. An investor is placed in the **highest tier** for which they satisfy **all four** of the following requirements simultaneously:

| Tier | Risk Score Threshold | Min. Lifetime Invested | Min. Successful Investments | Max. Historical Default Rate | Limit Multiplier |
|:---|:---|:---|:---|:---|:---|
| **VIP** | $\le 10$ | $\$5,000,000$ | $\ge 50$ | $\le 5\%$ | **10x** |
| **Platinum** | $\le 20$ | $\$1,000,000$ | $\ge 20$ | $\le 10\%$ | **5x** |
| **Gold** | $\le 40$ | $\$100,000$ | $\ge 10$ | $\le 15\%$ | **3x** |
| **Silver** | $\le 60$ | $\$10,000$ | $\ge 3$ | $\le 25\%$ | **2x** |
| **Basic** *(Default)*| — | — | — | — | **1x** |

> [!IMPORTANT]
> If an investor fails to satisfy even one condition of a tier, they are relegated to the next lower tier where all conditions are met. If none are met, they default to the **Basic** tier.

---

## 2. Risk Levels & Gating Operations

The investor's numeric **Risk Score** maps to a coarse **Risk Level**, which applies a risk discount multiplier to their limit and dictates additional transactional gates.

| Risk Score | Risk Level | Limit Multiplier | Per-Bid Transaction Cap | Gating Behavior |
|:---|:---|:---|:---|:---|
| **0 – 25** | `Low` | 100% | *No cap* | Standard investment flow. |
| **26 – 50** | `Medium` | 75% | *No cap* | Limit reduced to 75% of baseline. |
| **51 – 75** | `High` | 50% | **$\$50,000$** | Blocked if a single bid exceeds $\$50,000$. |
| **76 – 100** | `VeryHigh` | 25% | **$\$10,000$** | Blocked if a single bid exceeds $\$10,000$. |

---

## 3. Gating Enforcements in Bidding

Every time an investor places a bid on an invoice, the smart contract executes a series of validation gates inside the `validate_investor_investment` function.

### Validation Sequence

1. **Verification Status Gate**: The investor's KYC status must be explicitly `Verified`.
   - *Failure result*: `QuickLendXError::BusinessNotVerified` (equivalent to unauthorized).
2. **Protocol Minimum Tier Gate**: The investor's tier must meet or exceed the protocol's globally configured minimum investor tier (`min_investor_tier`).
   - *Failure result*: `QuickLendXError::InsufficientKYCTier`.
3. **Aggregate Exposure Gate**: The sum of the new bid, all active outstanding bids, and the total lifetime invested funds must not exceed the dynamic effective investment limit.
   - *Formula*: `active_bid_exposure + total_invested + new_bid_amount <= effective_limit`
   - *Failure result*: `QuickLendXError::InvalidAmount`.
4. **Per-Bid Transaction Gate**: If the investor is rated `High` or `VeryHigh` risk, individual bid amounts are capped.
   - *Failure result*: `QuickLendXError::InvalidAmount`.

---

## 4. Operational Examples

### Example A: Successful Onboarding & Limit Assignment

1. An operator approves a new investor with an admin-defined `base_limit` of **$\$100,000$**.
2. Because the investor has no historical performance, they default to the **Basic** tier (1x multiplier) and their risk score places them in the **Low** risk level (100% multiplier).
3. The dynamic investment limit is calculated as:
   $$\text{Limit} = \text{base\_limit} \times 1 \text{ (Tier)} \times 100\% \text{ (Risk)} = \$100,000$$

### Example B: Recomputed VIP Promotion

1. An investor with a `base_limit` of **$\$500,000$** reaches the following milestones:
   - Lifetime Invested: $\$5,200,000$
   - Successful Investments: $55$
   - Defaulted Investments: $1$ (Default Rate: $1/56 \approx 1.78\%$)
   - Calculated Risk Score: $9$
2. The recomputation evaluates the conditions for the **VIP** tier:
   - Risk score $9 \le 10$ ✓
   - Invested $\$5.2\text{M} \ge \$5.0\text{M}$ ✓
   - Successful investments $55 \ge 50$ ✓
   - Default rate $1.78\% \le 5\%$ ✓
3. The investor is promoted to **VIP** (10x multiplier) at **Low** risk level. Their new limit becomes:
   $$\text{Limit} = \$500,000 \times 10 \text{ (VIP)} \times 100\% \text{ (Risk)} = \$5,000,000$$

### Example C: Gated Over-limit Transaction

If a `VeryHigh` risk investor attempts to place a bid of **$\$15,000$**, the contract halts the transaction:

```rust
// Internally executes validate_investor_investment:
let investment_amount = 15000;
let risk_level = InvestorRiskLevel::VeryHigh;

match risk_level {
    InvestorRiskLevel::VeryHigh => {
        if investment_amount > 10000 {
            return Err(QuickLendXError::InvalidAmount); // Execution fails here
        }
    }
    // ...
}
```

---

## Related Documentation
- [`docs/README.md`](README.md) — Documentation index.
- [`docs/INVESTOR_TIER.md`](INVESTOR_TIER.md) — Under-the-hood risk calculation algorithms and code symbols for contract contributors.
- [`docs/contracts/investor-kyc.md`](contracts/investor-kyc.md) — KYC lifecycle and admin configuration entrypoints.

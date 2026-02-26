# Verification System

The QuickLendX verification module (`verification.rs`) provides KYC and compliance infrastructure for both businesses and investors. This document covers the investor verification flow, investment limits, risk assessment, and integration with the bidding system.

> For business KYC specifically, see [business-kyc.md](./business-kyc.md).  
> For detailed investor tier/limit mechanics, see [investor-kyc.md](./investor-kyc.md).

## Architecture Overview

```
┌─────────────┐     submit_investor_kyc     ┌────────────────────────────┐
│   Investor   │ ─────────────────────────▶  │  InvestorVerificationStorage│
└─────────────┘                              │   status: Pending           │
                                             └────────────┬───────────────┘
                                                          │
                          ┌───────────────────────────────┴───────────────────┐
                          ▼                                                   ▼
                   verify_investor                                   reject_investor
                   ┌──────────────┐                                ┌──────────────┐
                   │ risk_score   │                                │ status:      │
                   │ tier         │                                │  Rejected    │
                   │ risk_level   │                                │ reason: ...  │
                   │ limit calc   │                                └──────────────┘
                   │ status:      │
                   │  Verified    │
                   └──────┬───────┘
                          │
                          ▼
                     place_bid
                  ┌──────────────┐
                  │ check status │
                  │ check limit  │
                  │ validate_bid │
                  └──────────────┘
```

## Data Structures

### InvestorVerification

Complete on-chain record for each investor.

```rust
pub struct InvestorVerification {
    pub investor: Address,
    pub status: BusinessVerificationStatus,   // Pending | Verified | Rejected
    pub verified_at: Option<u64>,
    pub verified_by: Option<Address>,
    pub kyc_data: String,
    pub investment_limit: i128,
    pub submitted_at: u64,
    pub tier: InvestorTier,                    // Basic | Silver | Gold | Platinum | VIP
    pub risk_level: InvestorRiskLevel,         // Low | Medium | High | VeryHigh
    pub risk_score: u32,                       // 0-100
    pub total_invested: i128,
    pub total_returns: i128,
    pub successful_investments: u32,
    pub defaulted_investments: u32,
    pub last_activity: u64,
    pub rejection_reason: Option<String>,
    pub compliance_notes: Option<String>,
}
```

### Storage Keys

| Key Pattern | Description |
|---|---|
| `InvVer:{address}` | Individual investor verification record |
| `PendInv` | List of pending investor addresses |
| `VerInv` | List of verified investor addresses |
| `RejInv` | List of rejected investor addresses |
| `TierInv:{tier}` | List of investors by tier |
| `RiskInv:{level}` | List of investors by risk level |

## Investor KYC Lifecycle

### 1. Submission — `submit_investor_kyc`

```rust
pub fn submit_investor_kyc(env: Env, investor: Address, kyc_data: String)
    -> Result<(), QuickLendXError>
```

- Requires `investor.require_auth()`
- Validates `kyc_data` length ≤ `MAX_KYC_DATA_LENGTH`
- **Allowed transitions**: None → Pending, Rejected → Pending
- **Blocked if**: status is `Pending` or `Verified`
- Defaults: `tier = Basic`, `risk_level = High`, `risk_score = 100`

### 2. Verification — `verify_investor`

```rust
pub fn verify_investor(env: Env, investor: Address, investment_limit: i128)
    -> Result<InvestorVerification, QuickLendXError>
```

- Requires `admin.require_auth()` and admin identity check
- `investment_limit` must be > 0
- Computes `risk_score` → `tier` → `risk_level` → `investment_limit`
- Moves investor from pending list to verified list
- Adds investor to the appropriate tier and risk-level lists

### 3. Rejection — `reject_investor`

```rust
pub fn reject_investor(env: Env, admin: Address, investor: Address, reason: String)
    -> Result<(), QuickLendXError>
```

- Requires `admin.require_auth()`
- Sets status to `Rejected` with reason string
- Moves investor from pending list to rejected list
- Investor may resubmit KYC after rejection

### 4. Limit Update — `set_investment_limit`

```rust
pub fn set_investment_limit(env: Env, admin: Address, investor: Address, new_limit: i128)
    -> Result<(), QuickLendXError>
```

- Admin-only operation to adjust a verified investor's limit
- Recalculates using tier and risk multipliers
- Preserves admin-approved baseline and applies dynamic multipliers deterministically

## Risk Assessment

### Risk Score Calculation (`calculate_investor_risk_score`)

| Factor | Score Impact |
|---|---|
| KYC data < 100 chars | +30 (incomplete KYC) |
| KYC data 100-499 chars | +20 (moderate) |
| KYC data ≥ 500 chars | +10 (comprehensive) |
| Default rate (% of total) | + default_rate |
| Total invested > 1M | -20 |
| Total invested > 100K | -10 |

Score is capped at 100.

### Tier Determination (`determine_investor_tier`)

| Tier | Risk Score | Total Invested | Successful Investments |
|---|---|---|---|
| VIP | ≤ 10 | > $5M | > 50 |
| Platinum | ≤ 20 | > $1M | > 20 |
| Gold | ≤ 40 | > $100K | > 10 |
| Silver | ≤ 60 | > $10K | > 3 |
| Basic | Any | Any | Any |

### Risk Level Mapping (`determine_risk_level`)

| Risk Score | Risk Level |
|---|---|
| 0–25 | Low |
| 26–50 | Medium |
| 51–75 | High |
| 76–100 | VeryHigh |

### Limit Calculation (`calculate_investment_limit`)

```
final_limit = base_limit × tier_multiplier × risk_multiplier / 100
```

| Tier | Multiplier |   | Risk Level | Multiplier |
|---|---|---|---|---|
| VIP | 10× |   | Low | 100% |
| Platinum | 5× |   | Medium | 75% |
| Gold | 3× |   | High | 50% |
| Silver | 2× |   | VeryHigh | 25% |
| Basic | 1× |   | | |

`base_limit` is normalized to non-negative values before multiplier application.

### Analytics Recalculation Safety

When `update_investor_analytics` runs after settlement/default updates, it:

- updates totals and success/default counters,
- recalculates `risk_score`, `risk_level`, and `tier`,
- **recovers the previously approved baseline limit** from the current derived limit,
- reapplies multipliers using the updated profile.

This prevents unintended resets to hardcoded limits during analytics refresh.

## Bid Enforcement

### In `place_bid` (lib.rs)

The `place_bid` function enforces investor verification before any bid is accepted:

1. Retrieves `InvestorVerification` — fails with `BusinessNotVerified` if none
2. Checks `status`:
   - `Verified` → proceeds; enforces `validate_investor_investment` (limit + risk caps)
   - `Pending` → returns `KYCAlreadyPending`
   - `Rejected` → returns `BusinessNotVerified`
3. Calls `validate_bid` → `validate_investor_investment` for risk-level caps

### In `validate_investor_investment` (verification.rs)

Additional risk-level hard caps enforced independently of calculated limits:

| Risk Level | Maximum per Investment |
|---|---|
| VeryHigh | $10,000 |
| High | $50,000 |
| Medium / Low | Up to calculated limit |

## Query Functions

### Status Lists

```rust
fn get_verified_investors(env: Env) -> Vec<Address>
fn get_pending_investors(env: Env) -> Vec<Address>
fn get_rejected_investors(env: Env) -> Vec<Address>
```

### By Tier and Risk Level

```rust
fn get_investors_by_tier(env: Env, tier: InvestorTier) -> Vec<Address>
fn get_investors_by_risk_level(env: Env, risk_level: InvestorRiskLevel) -> Vec<Address>
```

### Individual Investor

```rust
fn get_investor_verification(env: Env, investor: Address) -> Option<InvestorVerification>
fn is_investor_verified(env: Env, investor: Address) -> bool
fn get_investor_analytics(env: Env, investor: Address) -> Result<InvestorVerification, QuickLendXError>
```

## Analytics Tracking

`update_investor_analytics` is called after investment settlement and default handling to update:
- `total_invested`, `total_returns`
- `successful_investments` / `defaulted_investments`
- `last_activity` timestamp

These fields feed back into risk scoring and tier determination on subsequent verifications.

## Error Codes

| Error | Code | Trigger |
|---|---|---|
| `KYCNotFound` | — | No verification record exists |
| `KYCAlreadyPending` | — | Submitting while status is Pending |
| `KYCAlreadyVerified` | — | Submitting or verifying while already Verified |
| `BusinessNotVerified` | — | Placing bid while unverified or rejected |
| `InvalidAmount` | — | Bid exceeds limit, or limit ≤ 0 |
| `NotAdmin` | — | Non-admin calling admin-only function |

## Security Notes

1. **Auth enforcement**: `investor.require_auth()` on submission, `admin.require_auth()` on verify/reject/limit-update
2. **Admin identity check**: Admin address is verified against stored admin before verification operations
3. **Input validation**: KYC data and rejection reason strings are length-checked against protocol maximums
4. **Risk cap**: Even if admin sets a high base limit, risk multipliers and hard caps constrain actual exposure
5. **Resubmission guard**: Verified investors cannot resubmit; only rejected investors can retry
6. **List consistency**: Investors are moved between pending/verified/rejected lists atomically during status transitions

## Test Coverage

| Test File | Tests | Coverage |
|---|---|---|
| `test_investor_kyc.rs` | 48 | KYC lifecycle, limit enforcement, bidding integration, status transitions, tier/risk queries, edge cases |
| `test_limit.rs` | 6 | Invoice/bid amount limits, due-date limits, admin authorization |

Run tests:
```bash
cargo test test_investor_kyc  # 48 tests
cargo test test_limit         # 6 tests
```

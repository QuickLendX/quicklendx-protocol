# Investor KYC and Investment Limits

## Overview

The QuickLendX protocol implements a comprehensive investor verification system that ensures only verified investors can participate in invoice financing. The system includes KYC (Know Your Customer) verification, risk assessment, tiered investor classification, and per-investor investment limits.

## Key Features

- **KYC Verification**: Investors must submit KYC data and be verified by admins before placing bids
- **Investment Limits**: Each verified investor has a maximum investment limit based on their tier and risk level
- **Risk Assessment**: Automated risk scoring based on KYC data and investment history
- **Tiered System**: Investors are classified into tiers (Basic, Silver, Gold, Platinum, VIP) with different privileges
- **Dynamic Limits**: Investment limits are calculated based on tier multipliers and risk adjustments

## Investor Verification Process

### 1. KYC Submission
Investors submit their KYC data using the `submit_investor_kyc` function:

```rust
pub fn submit_investor_kyc(
    env: Env,
    investor: Address,
    kyc_data: String,
) -> Result<(), QuickLendXError>
```

**Requirements:**
- Only the investor can submit their own KYC
- KYC data should contain comprehensive verification information
- Cannot resubmit if already pending or verified (can resubmit if previously rejected)

### 2. Admin Verification
Admins review and verify investors using the `verify_investor` function:

```rust
pub fn verify_investor(
    env: Env,
    investor: Address,
    investment_limit: i128,
) -> Result<InvestorVerification, QuickLendXError>
```

**Process:**
1. Admin reviews submitted KYC data
2. Sets a base investment limit
3. System calculates risk score based on KYC data
4. System determines investor tier based on risk and history
5. Final investment limit is calculated using tier and risk multipliers

### 3. Investment Limit Management
Admins can update investment limits for verified investors:

```rust
pub fn set_investment_limit(
    env: Env,
    investor: Address,
    new_limit: i128,
) -> Result<(), QuickLendXError>
```

## Investor Tiers and Risk Levels

### Investor Tiers
- **VIP**: Very low risk, high investment volume (>$5M), many successful investments (>50)
- **Platinum**: Low risk, high investment volume (>$1M), good track record (>20)
- **Gold**: Medium-low risk, moderate investment volume (>$100K), decent history (>10)
- **Silver**: Medium risk, some investment history (>$10K), few investments (>3)
- **Basic**: Default tier for new or low-volume investors

### Risk Levels
- **Low** (0-25): Minimal risk, full investment privileges
- **Medium** (26-50): Moderate risk, 75% of calculated limit
- **High** (51-75): High risk, 50% of calculated limit, max $50K per investment
- **Very High** (76-100): Very high risk, 25% of calculated limit, max $10K per investment

### Investment Limit Calculation

```rust
final_limit = base_limit × tier_multiplier × risk_multiplier / 100
```

**Tier Multipliers:**
- VIP: 10x
- Platinum: 5x
- Gold: 3x
- Silver: 2x
- Basic: 1x

**Risk Multipliers:**
- Low: 100% (no reduction)
- Medium: 75%
- High: 50%
- Very High: 25%

## Bid Placement Enforcement

When investors place bids, the system enforces verification and limits:

1. **Verification Check**: Investor must be verified (status = Verified)
2. **Investment Limit Check**: Bid amount must not exceed investor's limit
3. **Risk-Based Restrictions**: Additional limits based on risk level
4. **Duplicate Bid Prevention**: One active bid per investor per invoice

## Error Handling

The system uses specific error codes for investor verification:

- `KYCNotFound`: No KYC record exists for the investor
- `KYCAlreadyPending`: KYC is already under review
- `KYCAlreadyVerified`: Investor is already verified
- `InvalidKYCStatus`: Operation not allowed for current KYC status
- `BusinessNotVerified`: Investor is not verified (used for investor verification too)
- `InvalidAmount`: Investment amount exceeds limit or is invalid
- `NotAdmin`: Only admins can perform verification operations

## Query Functions

### Get Investor Information
```rust
// Get full verification record
pub fn get_investor_verification(env: Env, investor: Address) -> Option<InvestorVerification>

// Check verification status
pub fn is_investor_verified(env: Env, investor: Address) -> bool

// Get analytics and performance data
pub fn get_investor_analytics(env: Env, investor: Address) -> Result<InvestorVerification, QuickLendXError>
```

### List Investors by Status
```rust
// Get all verified investors
pub fn get_verified_investors(env: Env) -> Vec<Address>

// Get pending verifications
pub fn get_pending_investors(env: Env) -> Vec<Address>

// Get rejected applications
pub fn get_rejected_investors(env: Env) -> Vec<Address>
```

### Filter by Tier and Risk
```rust
// Get investors by tier
pub fn get_investors_by_tier(env: Env, tier: InvestorTier) -> Vec<Address>

// Get investors by risk level
pub fn get_investors_by_risk_level(env: Env, risk_level: InvestorRiskLevel) -> Vec<Address>
```

## Security Considerations

1. **Authorization**: All verification operations require proper authorization
2. **Admin Controls**: Only verified admins can approve/reject investors
3. **Limit Enforcement**: Investment limits are strictly enforced at bid placement
4. **Risk Assessment**: Automated risk scoring reduces manual bias
5. **Audit Trail**: All verification actions are logged and tracked

## Integration with Bidding System

The investor verification system is tightly integrated with the bidding process:

1. **Pre-Bid Validation**: `validate_investor_investment` checks verification and limits
2. **Bid Placement**: `place_bid` enforces all investor requirements
3. **Analytics Updates**: Investment outcomes update investor risk scores and tiers
4. **Dynamic Limits**: Limits are recalculated based on performance

## Example Usage

```rust
// 1. Investor submits KYC
contract.submit_investor_kyc(&investor_addr, &kyc_data);

// 2. Admin verifies investor with $100K base limit
contract.verify_investor(&investor_addr, &100_000);

// 3. Investor can now place bids up to their calculated limit
let bid_id = contract.place_bid(&investor_addr, &invoice_id, &50_000, &55_000);

// 4. Admin can update limits later
contract.set_investment_limit(&investor_addr, &200_000);
```

## Testing

The system includes comprehensive tests covering:
- KYC submission and verification flows
- Investment limit enforcement
- Risk assessment calculations
- Tier determination logic
- Error conditions and edge cases
- Integration with bidding system

See `test_bid.rs` and related test files for detailed test coverage.
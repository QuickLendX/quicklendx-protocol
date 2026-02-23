# Verification Module

The verification module implements the complete KYC (Know Your Customer) and verification system for the QuickLendX protocol. It covers both **business verification** (required for invoice uploads) and **investor verification** (required for bidding and investing).

## Overview

QuickLendX enforces a mandatory verification flow for all participants:

- **Businesses** must submit KYC data and be verified by an admin before uploading invoices.
- **Investors** must submit KYC data and be verified before placing bids or making investments.

All verification state transitions, timestamps, and rejection reasons are stored on-chain for full auditability.

## Data Structures

### BusinessVerificationStatus

```rust
enum BusinessVerificationStatus {
    Pending,
    Verified,
    Rejected,
}
```

### BusinessVerification

```rust
struct BusinessVerification {
    business: Address,
    status: BusinessVerificationStatus,
    verified_at: Option<u64>,
    verified_by: Option<Address>,
    kyc_data: String,          // Encrypted KYC data (JSON)
    submitted_at: u64,
    rejection_reason: Option<String>,
}
```

### InvestorVerification

```rust
struct InvestorVerification {
    investor: Address,
    status: BusinessVerificationStatus,
    verified_at: Option<u64>,
    verified_by: Option<Address>,
    kyc_data: String,
    investment_limit: i128,
    submitted_at: u64,
    tier: InvestorTier,
    risk_level: InvestorRiskLevel,
    risk_score: u32,
    total_invested: i128,
    total_returns: i128,
    successful_investments: u32,
    defaulted_investments: u32,
    last_activity: u64,
    rejection_reason: Option<String>,
    compliance_notes: Option<String>,
}
```

### InvestorTier

```rust
enum InvestorTier {
    Basic,
    Silver,
    Gold,
    Platinum,
    VIP,
}
```

### InvestorRiskLevel

```rust
enum InvestorRiskLevel {
    Low,
    Medium,
    High,
    VeryHigh,
}
```

## Storage

### BusinessVerificationStorage

Manages on-chain storage for business verification records. Maintains three indexed lists for efficient querying:

| Key | Description |
|-----|-------------|
| `verified_businesses` | List of all verified business addresses |
| `pending_businesses` | List of all businesses awaiting review |
| `rejected_businesses` | List of all rejected business addresses |

Individual verification records are keyed by the business `Address`.

**Key methods:**

- `store_verification(env, verification)` — Stores a new verification record and adds to the appropriate status list.
- `get_verification(env, business) -> Option<BusinessVerification>` — Retrieves a business's verification record.
- `update_verification(env, verification)` — Updates a record, moving it between status lists as needed.
- `is_business_verified(env, business) -> bool` — Returns `true` if the business has `Verified` status.
- `get_verified_businesses(env) -> Vec<Address>` — Returns all verified business addresses.
- `get_pending_businesses(env) -> Vec<Address>` — Returns all pending business addresses.
- `get_rejected_businesses(env) -> Vec<Address>` — Returns all rejected business addresses.

### InvestorVerificationStorage

Manages on-chain storage for investor verification records with the same indexed-list pattern.

**Key methods:**

- `submit(env, investor, kyc_data)` — Submits a new investor KYC application.
- `store(env, verification)` — Stores an investor verification record.
- `get(env, investor) -> Option<InvestorVerification>` — Retrieves an investor's verification record.
- `update(env, verification)` — Updates a record, moving it between status lists.
- `is_investor_verified(env, investor) -> bool` — Returns `true` if the investor has `Verified` status.
- `get_investors_by_tier(env, tier) -> Vec<Address>` — Returns investors filtered by tier.
- `get_investors_by_risk_level(env, risk_level) -> Vec<Address>` — Returns investors filtered by risk level.

## Business KYC Functions

### `submit_kyc_application(env, business, kyc_data)`

Submits a new KYC application for a business.

- **Authorization**: Only the business itself can submit its own KYC (`business.require_auth()`).
- **Behavior**:
  - If no prior record exists, creates a new `Pending` verification.
  - If status is `Pending`, returns `KYCAlreadyPending` error.
  - If status is `Verified`, returns `KYCAlreadyVerified` error.
  - If status is `Rejected`, allows resubmission with updated data.
- **Events**: Emits `kyc_sub` event.

### `verify_business(env, admin, business)`

Admin approves a pending business KYC application.

- **Authorization**: Only the contract admin can call this.
- **Requirements**: Business must have `Pending` status.
- **Effect**: Sets status to `Verified`, records `verified_at` timestamp and `verified_by` admin address.
- **Events**: Emits `bus_ver` event.

### `reject_business(env, admin, business, reason)`

Admin rejects a pending business KYC application.

- **Authorization**: Only the contract admin can call this.
- **Requirements**: Business must have `Pending` status.
- **Effect**: Sets status to `Rejected`, stores the rejection reason.
- **Events**: Emits `bus_rej` event.

### `get_business_verification_status(env, business)`

Returns the full `BusinessVerification` record for a business, or `None` if no KYC has been submitted.

### `require_business_verification(env, business)`

Helper that returns `BusinessNotVerified` error if the business is not verified. Used internally to gate invoice uploads.

## Investor KYC Functions

### `submit_investor_kyc(env, investor, kyc_data)`

Submits a new KYC application for an investor.

- **Authorization**: Only the investor itself can submit.
- **Behavior**: Same resubmission rules as business KYC.

### `verify_investor(env, admin, investor, investment_limit)`

Admin approves an investor with a base investment limit.

- Calculates risk score, tier, and risk level.
- Computes final investment limit based on tier and risk multipliers.

### `reject_investor(env, admin, investor, reason)`

Admin rejects an investor KYC application with a reason.

### Risk Assessment

- **`calculate_investor_risk_score`** — Scores 0–100 based on KYC data completeness and investment history.
- **`determine_investor_tier`** — Assigns tier (Basic → VIP) based on risk score, total invested, and successful investments.
- **`determine_risk_level`** — Maps risk score to Low/Medium/High/VeryHigh.
- **`calculate_investment_limit`** — Applies tier and risk multipliers to the base limit.

## State Transitions

```
                  ┌──────────┐
     submit_kyc   │          │  verify_business
  ───────────────►│ Pending  ├──────────────────► Verified
                  │          │
                  └────┬─────┘
                       │
                       │ reject_business
                       ▼
                  ┌──────────┐
                  │ Rejected │
                  └────┬─────┘
                       │
                       │ submit_kyc (resubmission)
                       ▼
                  ┌──────────┐
                  │ Pending  │  (cycle restarts)
                  └──────────┘
```

## Enforcement

The `upload_invoice` contract function checks business verification status before allowing invoice creation:

```rust
let verification = get_business_verification_status(&env, &business);
if verification.is_none()
    || !matches!(verification.unwrap().status, BusinessVerificationStatus::Verified)
{
    return Err(QuickLendXError::BusinessNotVerified);
}
```

Similarly, `validate_investor_investment` checks that an investor is verified and within their investment limits before allowing bids.

## Events

| Event | Symbol | Payload | Description |
|-------|--------|---------|-------------|
| KYC Submitted | `kyc_sub` | `(business, timestamp)` | Business submitted KYC data |
| Business Verified | `bus_ver` | `(business, admin, timestamp)` | Admin verified a business |
| Business Rejected | `bus_rej` | `(business, admin)` | Admin rejected a business |

## Error Codes

| Error | Code | Description |
|-------|------|-------------|
| `NotAdmin` | 1005 | Caller is not the contract admin |
| `BusinessNotVerified` | 1007 | Business has not been verified |
| `KYCAlreadyPending` | 1025 | KYC application is already pending review |
| `KYCAlreadyVerified` | 1026 | Business/investor is already verified |
| `KYCNotFound` | 1027 | No KYC record found for the address |
| `InvalidKYCStatus` | 1028 | Operation not valid for current KYC status |

## Security Considerations

- **Authorization**: All state-changing functions require `require_auth()` from the appropriate party (business for submission, admin for verify/reject).
- **Admin-only operations**: `verify_business` and `reject_business` check `is_admin()` before proceeding.
- **Status guards**: Only `Pending` applications can be verified or rejected. Only `Rejected` applications can be resubmitted.
- **On-chain auditability**: All timestamps, admin addresses, and rejection reasons are stored immutably.
- **Encrypted KYC data**: The `kyc_data` field stores encrypted JSON, keeping sensitive business information private while maintaining on-chain proof of submission.

## Test Coverage

Tests are located in `src/test_business_kyc.rs` and cover:

- **Submission**: Business self-submission, empty data, duplicate prevention
- **Authorization**: Admin-only verify/reject, non-admin rejection
- **Status transitions**: Pending → Verified, Pending → Rejected, Rejected → Pending (resubmission)
- **Enforcement**: Unverified businesses blocked from invoice upload, verified businesses allowed
- **Edge cases**: Non-existent business verify/reject, double verify/reject, concurrent multi-business flows
- **Data integrity**: KYC data preserved through transitions, timestamp accuracy
- **Integration**: Full KYC-to-invoice lifecycle, rejection-resubmission-verification cycle

Run tests with:

```bash
cargo test test_business_kyc -- --nocapture
```

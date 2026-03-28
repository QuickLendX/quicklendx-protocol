# verification.rs — Centralized Verification Guard System

## Overview

`verification.rs` provides the **single source of truth** for actor verification
enforcement across the QuickLendX protocol. Every restricted finance action
(invoice upload, bid placement, settlement initiation, escrow release) must pass
through a guard function in this module before execution.

The module is **pure Rust** with no blockchain dependencies, making it fully
testable and portable across environments.

### Design Philosophy

- **Deny-by-default** — every guard returns `Err` unless the actor is explicitly
  `Verified`. Pending, Rejected, and unknown (no KYC record) actors are all
  blocked.
- **Checked arithmetic** — investment limit calculations use `checked_*`
  operations; overflow returns `None` or `GuardError::ArithmeticOverflow`.
- **Typed errors** — callers receive a `GuardError` or `TransitionError`
  explaining *why* the action was denied, enabling precise audit trails.
- **Exhaustive state transitions** — a 3x3 transition matrix is fully validated;
  only 3 of 9 possible transitions are allowed.

## KYC Verification Status

Both business and investor actors share the same status enum:

```
VerificationStatus { Pending, Verified, Rejected }
```

| Status   | Meaning                                | Restricted Actions |
|----------|----------------------------------------|--------------------|
| Pending  | KYC submitted, awaiting admin review   | Blocked            |
| Verified | Admin-approved                         | Allowed            |
| Rejected | Admin-rejected, must resubmit          | Blocked            |
| *(none)* | No KYC record exists                   | Blocked            |

## Guard Functions

### Business Guards

| Guard                        | Action Protected       | Required Status |
|------------------------------|------------------------|-----------------|
| `guard_invoice_upload`       | Upload new invoice     | Verified        |
| `guard_settlement_initiation`| Initiate settlement    | Verified        |
| `guard_escrow_release`       | Release escrowed funds | Verified        |

All three delegate to `guard_business_action(status)` which enforces:

```
match status {
    None         => Err(NotSubmitted)
    Pending      => Err(VerificationPending)
    Rejected     => Err(VerificationRejected)
    Verified     => Ok(())
}
```

### Investor Guards

| Guard                     | Action Protected   | Required Status | Extra Checks          |
|---------------------------|--------------------|-----------------|-----------------------|
| `guard_bid_placement`     | Place a bid        | Verified        | amount <= limit + cap |
| `guard_investment_action` | Generic investment | Verified        | amount <= limit + cap |

Investor guards perform a 5-step check sequence:

1. **Verification status** — must be `Verified`
2. **Zero-amount** — amount must be > 0
3. **Effective limit** — `base_limit * tier_multiplier * risk_bps / 10_000`
4. **Limit check** — `amount <= effective_limit`
5. **Per-investment risk cap** — `amount <= cap` (if applicable)

Error priority follows this order: status errors are returned before amount errors.

## State Transition Rules

### Allowed Transitions

| From     | To       | Trigger              |
|----------|----------|----------------------|
| Pending  | Verified | Admin approves KYC   |
| Pending  | Rejected | Admin rejects KYC    |
| Rejected | Pending  | Actor resubmits KYC  |

### Blocked Transitions

| From     | To       | Error              | Reason                        |
|----------|----------|--------------------|-------------------------------|
| Verified | *any*    | `AlreadyVerified`  | Verified is a terminal state  |
| Pending  | Pending  | `AlreadyPending`   | Duplicate submission          |
| Rejected | Verified | `InvalidTransition` | Must go through Pending first |
| Rejected | Rejected | `InvalidTransition` | No-op is not allowed          |

### Rejection Workflow

`validate_rejection_reason(reason)`:
- Reason must be non-empty
- Reason must not exceed `MAX_REJECTION_REASON_LENGTH` (512 bytes)
- On resubmission (Rejected -> Pending), the reason is cleared

`validate_kyc_data(data)`:
- KYC payload must be non-empty
- Must not exceed `MAX_KYC_DATA_LENGTH` (4,096 bytes)

## Investor Tier System

Tiers are computed from the investor's track record via `compute_tier()`:

| Tier     | Multiplier | Required Invested | Required Successful |
|----------|-----------|-------------------|---------------------|
| Basic    | 1x        | —                 | —                   |
| Silver   | 2x        | > 10,000          | > 3                 |
| Gold     | 3x        | > 100,000         | > 10                |
| Platinum | 5x        | > 1,000,000       | > 20                |
| VIP      | 10x       | > 5,000,000       | > 50                |

Both thresholds (invested amount AND successful investment count) must be met.

## Risk Level System

Risk levels are derived from a 0-100 score via `risk_level_from_score()`:

| Risk Level | Score Range | Limit Multiplier | Per-Investment Cap |
|------------|-------------|-------------------|--------------------|
| Low        | 0–25        | 100%              | None               |
| Medium     | 26–50       | 75%               | None               |
| High       | 51–75       | 50%               | 50,000             |
| VeryHigh   | 76–100      | 25%               | 10,000             |

## Effective Limit Formula

```
effective_limit = base_limit * tier_multiplier * risk_multiplier_bps / BPS_DENOMINATOR
```

Example: Gold tier, Medium risk, base_limit = 100,000:
```
100,000 * 3 * 7,500 / 10,000 = 225,000
```

## Security Assumptions and Controls

1. **Deny-by-default**: Every non-Verified status is blocked. There is no
   implicit trust — actors must be explicitly approved by an admin.

2. **Terminal Verified state**: Once verified, an actor cannot be reverted to
   Pending or Rejected. This prevents social-engineering attacks where a
   verified actor's status is downgraded and then re-verified with different
   KYC data.

3. **Checked arithmetic**: All limit computations use `checked_mul` and
   `checked_div`. Overflow returns `ArithmeticOverflow` rather than wrapping
   or panicking.

4. **Input size limits**: Rejection reasons (512B) and KYC payloads (4,096B)
   are capped to prevent storage abuse.

5. **Error ordering**: Status checks execute before amount checks. This ensures
   unverified actors cannot probe investment limits.

6. **Dual-threshold tier qualification**: Both the invested amount and
   successful investment count must exceed the threshold. A single large
   investment does not grant a higher tier.

7. **Per-investment caps**: High and VeryHigh risk investors face hard caps per
   individual investment, independent of their total limit. This limits
   protocol exposure to high-risk actors.

## Related Tests

Guard coverage is implemented in:

- `src/test_business_kyc.rs` — Business actor guard tests
  - Negative tests for every guarded path (Pending, Rejected, NotSubmitted)
  - All three business guard functions (invoice, settlement, escrow)
  - State transition matrix (all 9 from/to combinations)
  - Rejection reason validation (empty, boundary, over-limit)
  - KYC data validation
  - Full lifecycle test (submit -> reject -> resubmit -> verify)
  - Deny-by-default property verification
  - Error variant discrimination

- `src/test_investor_kyc.rs` — Investor actor guard tests
  - Negative tests for every guarded path
  - Investment limit enforcement across all 20 tier x risk combinations
  - Per-investment risk cap enforcement (High, VeryHigh)
  - Bid placement guard (status + limit + cap)
  - Tier qualification with dual-threshold enforcement
  - Risk score boundary testing (all 101 valid scores)
  - Error priority verification (status before amount)
  - Arithmetic overflow protection
  - Full lifecycle test (submit -> reject -> resubmit -> verify -> bid)
  - Edge cases: zero truncation, minimum amounts, maximum base limits

- `src/verification.rs` (inline `#[cfg(test)] mod tests`) — Unit tests
  - Tier multiplier values
  - Risk multiplier and per-investment cap values
  - Effective limit computation
  - State transition validation
  - Input validation (reason, KYC data)
  - Tier computation logic

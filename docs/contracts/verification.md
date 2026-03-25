# verification.rs — KYC & Pending-State Restrictions

## Overview

`verification.rs` implements identity verification for both businesses and investors.
It enforces that partially-verified (pending) accounts cannot perform privileged
actions, preventing a window of abuse between KYC submission and admin approval.

---

## State Machine

Both `BusinessVerification` and `InvestorVerification` share the same status enum:

```
None ──► Pending ──► Verified
                └──► Rejected ──► Pending (resubmission)
```

| State    | Meaning                                |
| -------- | -------------------------------------- |
| None     | No KYC record exists                   |
| Pending  | KYC submitted, awaiting admin decision |
| Verified | Admin approved; full access granted    |
| Rejected | Admin rejected; resubmission allowed   |

---

## Pending-State Restrictions

### Security Assumption

A `Pending` account has self-reported identity data that has **not** been
validated by an admin. Allowing privileged actions before approval would let
an attacker submit fraudulent KYC, act immediately, then be rejected — with
no recourse.

### Enforced Call Sites

| Function         | Guard applied                  | Error on pending    |
| ---------------- | ------------------------------ | ------------------- |
| `upload_invoice` | `require_business_not_pending` | `KYCAlreadyPending` |
| `cancel_invoice` | `require_business_not_pending` | `KYCAlreadyPending` |
| `accept_bid`     | `require_business_not_pending` | `KYCAlreadyPending` |
| `place_bid`      | inline status check            | `KYCAlreadyPending` |
| `withdraw_bid`   | `require_investor_not_pending` | `KYCAlreadyPending` |

### Error Distinction

Callers receive distinct errors depending on KYC state:

| KYC State | Error returned        |
| --------- | --------------------- |
| None      | `BusinessNotVerified` |
| Pending   | `KYCAlreadyPending`   |
| Rejected  | `BusinessNotVerified` |
| Verified  | _(no error)_          |

This allows frontends and integrators to show actionable messages
("your KYC is under review" vs "you must submit KYC first").

---

## Key Functions

### `require_business_not_pending(env, business) → Result<(), QuickLendXError>`

Checks the business KYC record and returns:

- `Ok(())` if `Verified`
- `Err(KYCAlreadyPending)` if `Pending`
- `Err(BusinessNotVerified)` if `Rejected` or no record

### `require_investor_not_pending(env, investor) → Result<(), QuickLendXError>`

Same semantics as above, applied to investor records.

### `submit_kyc_application(env, business, kyc_data)`

- Requires auth from `business`
- Idempotent for `Rejected` state (allows resubmission)
- Fails with `KYCAlreadyPending` if already pending
- Fails with `KYCAlreadyVerified` if already verified

### `verify_business(env, admin, business)` / `verify_investor(env, admin, investor, limit)`

- Admin-only (requires auth + `is_admin` check)
- Transitions status from `Pending` → `Verified`
- For investors: calculates risk score, tier, and investment limit

### `reject_business(env, admin, business, reason)` / `reject_investor(env, admin, investor, reason)`

- Admin-only
- Transitions status from `Pending` → `Rejected`
- Stores rejection reason for auditability

---

## Risk & Tier System (Investors)

Investor verification computes a `risk_score` (0–100) from KYC data completeness
and historical default rate. The score maps to:

| Score  | Risk Level | Tier eligibility |
| ------ | ---------- | ---------------- |
| 0–25   | Low        | up to VIP        |
| 26–50  | Medium     | up to Platinum   |
| 51–75  | High       | up to Silver     |
| 76–100 | VeryHigh   | Basic only       |

The final `investment_limit` is `base_limit × tier_multiplier × risk_multiplier / 100`.
`VeryHigh` risk investors are additionally capped at 10 000 per bid regardless of limit.

---

## Security Notes

- KYC data is stored as an opaque string; encryption is the caller's responsibility.
- String lengths are validated against `MAX_KYC_DATA_LENGTH` and `MAX_REJECTION_REASON_LENGTH`.
- Admin address is managed by `admin::AdminStorage`; `BusinessVerificationStorage::set_admin`
  is kept only for backward compatibility with existing tests.
- All state-mutating functions require explicit `require_auth()` calls before any storage writes.

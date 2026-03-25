# verification.rs - KYC and Rejection Workflow

## Overview

`verification.rs` controls business and investor KYC lifecycle state, including:
- submission and resubmission,
- admin approval/rejection,
- enforcement of pending/rejected restrictions,
- queryable reason data for audit trails.

Both business and investor records use `BusinessVerificationStatus`:
- `Pending`
- `Verified`
- `Rejected`

## Rejection Workflow

### Business rejection

`reject_business(env, admin, business, reason)`:
- requires authenticated admin,
- requires existing KYC record,
- allows transition only from `Pending -> Rejected`,
- persists `rejection_reason` for query and audit,
- updates status indexes (`pending`, `verified`, `rejected`) atomically through storage update helpers.

### Investor rejection

`reject_investor(env, admin, investor, reason)`:
- requires authenticated admin,
- requires existing KYC record,
- allows transition only from `Pending -> Rejected`,
- persists `rejection_reason` and compliance metadata,
- updates status indexes (`pending`, `verified`, `rejected`) atomically through storage update helpers.

## Reason Data Lifecycle

- Rejection reason is validated against `MAX_REJECTION_REASON_LENGTH`.
- On successful rejection, reason is stored in `rejection_reason`.
- On valid resubmission from `Rejected -> Pending`, reason is cleared (`None`).
- Reason is always queryable via:
  - `get_business_verification_status`
  - `get_investor_verification`

This preserves historical rejection context while preventing stale reasons from being shown after a new pending submission.

## Transition Matrix

| Entity | From | To | Allowed | Error if disallowed |
| --- | --- | --- | --- | --- |
| Business | Pending | Verified | Yes | `InvalidKYCStatus` |
| Business | Pending | Rejected | Yes | `InvalidKYCStatus` |
| Business | Rejected | Pending (resubmit) | Yes | `KYCAlreadyVerified` / `KYCAlreadyPending` |
| Investor | Pending | Verified | Yes | `InvalidKYCStatus` |
| Investor | Pending | Rejected | Yes | `InvalidKYCStatus` |
| Investor | Rejected | Pending (resubmit) | Yes | `KYCAlreadyVerified` / `KYCAlreadyPending` |

## Index Update Guarantees

Verification storage keeps three query indexes for both businesses and investors:
- pending list,
- verified list,
- rejected list.

During status updates the contract:
1. removes the address from the old status list,
2. stores the updated verification record,
3. adds the address to the new status list.

Expected guarantees:
- no stale membership in old status lists,
- no duplicate presence across status lists for a single account,
- query functions return state-consistent buckets.

## Security Assumptions and Controls

- Only admin addresses can verify/reject KYC records.
- All state-changing endpoints require authentication before writes.
- String length limits prevent oversized reason/KYC payload abuse.
- Pending and rejected users are blocked from privileged operations.
- Rejection reason persistence supports compliance and forensic review.

## Related Tests

Rejection workflow coverage is implemented in:
- `src/test_business_kyc.rs`
- `src/test_investor_kyc.rs`

Focus areas:
- reason persistence and reset behavior,
- status-transition enforcement,
- status index integrity,
- authorization and boundary checks.

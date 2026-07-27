# Business KYC Status System

**Audience: contributors** — this document is for people reading the contract source and wanting to verify the implementation against the documented intent. Operators and integrators should start from [`docs/KYC.md`](KYC.md).

All logic described here lives in [`quicklendx-contracts/src/verification.rs`](../quicklendx-contracts/src/verification.rs).

---

## Overview

Every business on the platform carries a KYC verification status that determines what operations they can perform. Unlike investors, businesses use a **status-based system** rather than tier-based progression.

| Field | Type | Purpose |
|---|---|---|
| `status` | `BusinessVerificationStatus` | Current state: Pending, Verified, or Rejected |
| `kyc_data` | `String` | Encrypted KYC submission data |
| `submitted_at` | `u64` | Timestamp when KYC was submitted |
| `verified_at` | `Option<u64>` | Timestamp when verification completed (if verified) |
| `verified_by` | `Option<Address>` | Admin who performed verification (if verified) |
| `rejection_reason` | `Option<String>` | Admin-provided reason for rejection (if rejected) |

The status is updated deterministically through admin actions and business resubmissions. The same inputs always yield the same status — the state machine is stable and idempotent.

---

## Business KYC Statuses

### Pending

The business has submitted KYC data and is awaiting admin review.

**What it gates:**
- Creating new invoices
- Accepting bids on existing invoices
- Canceling invoices

**Error returned:** `KYCAlreadyPending`

**Concrete example:**
```rust
// Business submits KYC
client.submit_kyc_application(&business, &kyc_data);

// Attempting to upload invoice fails
let result = client.try_upload_invoice(&business, &amount, &currency, &due_date, &description, &category, &tags);
assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::KYCAlreadyPending);
```

### Verified

The business has passed KYC review and can operate normally on the platform.

**What it gates:**
- Nothing — full platform access

**Concrete example:**
```rust
// Admin verifies business
client.verify_business(&admin, &business);

// Invoice upload now succeeds
let invoice_id = client.upload_invoice(&business, &amount, &currency, &due_date, &description, &category, &tags);
```

### Rejected

The business has failed KYC review and must resubmit with corrected data.

**What it gates:**
- Creating new invoices
- Accepting bids
- Canceling invoices

**Error returned:** `BusinessNotVerified`

**Concrete example:**
```rust
// Admin rejects business with reason
client.reject_business(&admin, &business, &rejection_reason);

// Attempting to upload invoice fails
let result = client.try_upload_invoice(&business, &amount, &currency, &due_date, &description, &category, &tags);
assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::BusinessNotVerified);
```

---

## State Transitions

The KYC status follows a strict state machine. Transitions are validated in `BusinessVerificationStorage::validate_state_transition`.

### Valid Transitions

| From | To | Trigger | Who calls it |
|---|---|---|---|
| None | Pending | New KYC submission | Business |
| Pending | Verified | Admin approval | Admin |
| Pending | Rejected | Admin rejection | Admin |
| Rejected | Pending | Resubmission after rejection | Business |

### Invalid Transitions

| From | To | Error | Rationale |
|---|---|---|---|
| Verified | Any | `InvalidKYCStatus` | Verified is final |
| Pending | Pending | `KYCAlreadyPending` | Duplicate submission |
| Rejected | Rejected | `InvalidKYCStatus` | Duplicate rejection |
| Rejected | Verified | `InvalidKYCStatus` | Must go through Pending first |
| None | Verified | `InvalidKYCStatus` | Cannot verify without submission |
| None | Rejected | `InvalidKYCStatus` | Cannot reject without submission |

### State Transition Diagram

```
None ──submit──> Pending ──verify──> Verified (final)
                   │                    │
                   └─reject──> Rejected │
                                        │
                   └────resubmit────────┘
```

### Worked Example — Complete Flow

**1. Business submits KYC (None → Pending)**
```rust
client.submit_kyc_application(&business, &kyc_data);
let verification = client.get_business_verification_status(&business);
assert!(matches!(verification.unwrap().status, BusinessVerificationStatus::Pending));
```

**2. Admin rejects (Pending → Rejected)**
```rust
client.reject_business(&admin, &business, &rejection_reason);
let verification = client.get_business_verification_status(&business);
assert!(matches!(verification.unwrap().status, BusinessVerificationStatus::Rejected));
assert_eq!(verification.unwrap().rejection_reason, Some(rejection_reason));
```

**3. Business resubmits (Rejected → Pending)**
```rust
let updated_kyc = create_test_kyc_data(&env, "UpdatedBusiness");
client.submit_kyc_application(&business, &updated_kyc);
let verification = client.get_business_verification_status(&business);
assert!(matches!(verification.unwrap().status, BusinessVerificationStatus::Pending));
assert!(verification.unwrap().rejection_reason.is_none()); // Cleared on resubmission
```

**4. Admin verifies (Pending → Verified)**
```rust
client.verify_business(&admin, &business);
let verification = client.get_business_verification_status(&business);
assert!(matches!(verification.unwrap().status, BusinessVerificationStatus::Verified));
assert!(verification.unwrap().verified_at.is_some());
assert_eq!(verification.unwrap().verified_by, Some(admin));
```

---

## Rejection Reason Immutability

Once a business is rejected with a reason, that reason cannot be changed. This prevents admin confusion and ensures audit trail integrity.

**Validation logic** (in `validate_rejection_reason_immutability`):
- If an old rejection reason exists, any new reason must match exactly
- Cannot remove a rejection reason once set
- Only cleared when business resubmits (Rejected → Pending transition)

**Concrete example:**
```rust
// First rejection
client.reject_business(&admin, &business, &String::from_str(&env, "Missing docs"));

// Attempting to change reason fails
let result = client.try_reject_business(&admin, &business, &String::from_str(&env, "Different reason"));
assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidKYCStatus);
```

---

## Index Consistency

The contract maintains three status-specific indexes for efficient queries:
- `verified_businesses`: All verified businesses
- `pending_businesses`: All pending businesses
- `rejected_businesses`: All rejected businesses

**Invariant:** A business must appear in **exactly one** index at any time.

**Validation** (in `verify_index_consistency`):
```rust
let in_verified = verified.iter().any(|addr| addr == *business);
let in_pending = pending.iter().any(|addr| addr == *business);
let in_rejected = rejected.iter().any(|addr| addr == *business);

let count = [in_verified, in_pending, in_rejected].iter().filter(|&&x| x).count();
assert_eq!(count, 1); // Business must be in exactly one list
```

This check runs after every status update to prevent index corruption.

---

## Invoice Limits

Businesses are subject to a protocol-wide limit on the number of **active invoices** they can have simultaneously.

### Active Invoice Classification

Only non-terminal invoices count toward the limit:

| Status | Active? | Rationale |
|---|---|---|
| Pending | Yes | Still seeking funding |
| Verified | Yes | Available for bidding |
| Funded | Yes | Investment in progress |
| Paid | No | Terminal — settled successfully |
| Defaulted | No | Terminal — failed to pay |
| Cancelled | No | Terminal — cancelled by business |
| Refunded | No | Terminal — refunded to investors |

### Limit Enforcement

**Default limit:** 100 active invoices per business (configurable via `max_invoices_per_business` in protocol limits)

**Special value:** `0` means unlimited

**Check timing:** Performed BEFORE invoice creation to prevent race conditions

**Error:** `MaxInvoicesPerBusinessExceeded`

**Concrete example:**
```rust
// Protocol limit set to 5
client.set_protocol_limits(&admin, &min_amount, &min_bid, &min_bps, &max_days, &grace, &5);

// Business creates 5 active invoices (succeeds)
for i in 0..5 {
    client.upload_invoice(&business, &amount, &currency, &due_date, &description, &category, &tags);
}

// 6th invoice fails
let result = client.try_upload_invoice(&business, &amount, &currency, &due_date, &description, &category, &tags);
assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::MaxInvoicesPerBusinessExceeded);
```

### Limit vs. KYC Status

The invoice limit is **independent** of KYC status:
- Unverified businesses cannot create invoices at all (KYC gate)
- Verified businesses can create invoices up to the active limit (capacity gate)

Both checks must pass for invoice creation to succeed.

---

## Entrypoints

The public contract functions that read or mutate business KYC:

```rust
// Read
fn get_business_verification_status(env: Env, business: Address) -> Option<BusinessVerification>
fn get_verified_businesses(env: Env) -> Vec<Address>
fn get_pending_businesses(env: Env) -> Vec<Address>
fn get_rejected_businesses(env: Env) -> Vec<Address>

// Mutate
fn submit_kyc_application(env: Env, business: Address, kyc_data: String)
fn verify_business(env: Env, admin: Address, business: Address)
fn reject_business(env: Env, admin: Address, business: Address, rejection_reason: String)
```

---

## Quick Decision Tree

```
Business attempts operation
        │
        ├─ No KYC record?
        │         → BusinessNotVerified
        │
        ├─ KYC status = Pending?
        │         → KYCAlreadyPending
        │
        ├─ KYC status = Rejected?
        │         → BusinessNotVerified
        │
        ├─ KYC status = Verified?
        │         │
        │         ├─ Operation = upload_invoice?
        │         │         ├─ Active invoice count >= limit?
        │         │         │         → MaxInvoicesPerBusinessExceeded
        │         │         │         → Success
        │         │         │
        │         └─ Other operations?
        │                   → Success
        │
        └─ Unknown status?
                  → BusinessNotVerified
```

---

## Related Documents

- [`docs/KYC.md`](KYC.md) — High-level KYC guide for operators (business vs investor KYC)
- [`docs/INVESTOR_TIER.md`](INVESTOR_TIER.md) — Investor tier system (symmetric contributor reference)
- [`docs/contracts/invoice-lifecycle.md`](contracts/invoice-lifecycle.md) — Invoice state machine and business operations
- [`docs/CAPS.md`](CAPS.md) — Protocol-wide limits and capacity management
- Source: [`quicklendx-contracts/src/verification.rs`](../quicklendx-contracts/src/verification.rs)
- Tests: [`quicklendx-contracts/src/test_business_kyc.rs`](../quicklendx-contracts/src/test_business_kyc.rs)

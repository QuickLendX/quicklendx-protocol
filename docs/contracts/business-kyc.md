# Business KYC Verification

QuickLendX implements a mandatory Know Your Customer (KYC) flow for all businesses wishing to upload invoices. This ensures that only verified entities can seek funding on the platform.

## Overview

The Business KYC flow consists of three main stages with strict lifecycle enforcement:

1. **Submission**: A business submits their KYC data (encrypted string) to the contract.
2. **Verification**: The platform admin reviews the submission and verifies the business.
3. **Enforcement**: The contract prevents unverified businesses from uploading invoices.

### State Transition Enforcement

The KYC system enforces strict state transitions to ensure data integrity and auditability:

**Valid Transitions:**

- `None → Pending`: New KYC submission
- `Pending → Verified`: Admin approval
- `Pending → Rejected`: Admin rejection with immutable reason
- `Rejected → Pending`: Resubmission after rejection

**Invalid Transitions (Blocked):**

- `Verified → *any other state`: Verified status is final
- `Pending → Pending`: Duplicate submissions
- `Rejected → Rejected`: Duplicate rejections
- `Rejected → Verified`: Must go through Pending first
- `None → Verified/Rejected`: Cannot skip Pending state

## Data Structures

### BusinessVerificationStatus

The status of a business's KYC application.

```rust
enum BusinessVerificationStatus {
    Pending,
    Verified,
    Rejected,
}
```

### BusinessVerification

The stored record for a business's KYC data.

```rust
struct BusinessVerification {
    business: Address,
    status: BusinessVerificationStatus,
    verified_at: Option<u64>,
    verified_by: Option<Address>,
    kyc_data: String,
    submitted_at: u64,
    rejection_reason: Option<String>,
}
```

## Key Functions

### For Businesses

#### `submit_kyc_application`

Submits a new KYC application or re-submits a rejected one.

- **Arguments**:
  - `kyc_data`: String (encrypted JSON containing business details)
- **Requirements**: Sender must be the business address.

#### `get_business_verification_status`

Queries the current verification status.

- **Returns**: `Option<BusinessVerification>`

### For Admins

#### `verify_business`

Approves a pending KYC application.

- **Arguments**:
  - `business`: Address of the business to verify.
- **Requirements**: Sender must be the contract admin.
- **Effect**: Sets status to `Verified`, allowing invoice uploads.

#### `reject_business`

Rejects a pending KYC application with a reason.

- **Arguments**:
  - `business`: Address of the business to reject.
  - `reason`: String explaining the rejection.
- **Requirements**: Sender must be the contract admin.
- **Effect**: Sets status to `Rejected`. Business can re-submit.

## Events

- `kyc_sub`: Emitted when a business submits KYC data for the first time.
- `kyc_resub`: Emitted when a business resubmits KYC data after rejection.
- `bus_ver`: Emitted when a business is verified by admin.
- `bus_rej`: Emitted when a business is rejected by admin (includes rejection reason).

### Event Data Structure

All events include comprehensive audit information:

- Business address
- Admin address (for verification/rejection events)
- Timestamp
- Action description
- Rejection reason (for rejection events)

## Usage Example

### 1. Business Submits KYC

```rust
client.submit_kyc_application(&business, &String::from_str(&env, "encrypted_kyc_data"));
```

### 2. Admin Verifies

```rust
client.verify_business(&admin, &business);
```

### 3. Business Uploads Invoice

```rust
// This will succeed only after verification
client.upload_invoice(
    &business,
    &1000,
    &currency,
    &due_date,
    &description,
    &category,
    &tags
);
```

## Security Considerations

- **Authorization**: Only the contract admin can change a verification status to `Verified`.
- **Enforcement**: The `upload_invoice` function explicitly checks `BusinessVerificationStatus::Verified`.
- **Immutable History**: Rejection reasons and verification timestamps are stored on-chain for auditability.
- **State Transition Validation**: All state transitions are strictly validated to prevent invalid status changes.
- **Rejection Reason Immutability**: Once set, rejection reasons cannot be modified, ensuring audit trail integrity.
- **Index Consistency**: The contract maintains consistent status lists (verified/pending/rejected) during all transitions.
- **Comprehensive Audit Trail**: All state changes emit detailed events for complete auditability.

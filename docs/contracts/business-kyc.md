# Business KYC Verification

QuickLendX implements a mandatory Know Your Customer (KYC) flow for all businesses wishing to upload invoices. This ensures that only verified entities can seek funding on the platform.

## Overview

The Business KYC flow consists of three main stages:
1. **Submission**: A business submits their KYC data (encrypted string) to the contract.
2. **Verification**: The platform admin reviews the submission and verifies the business.
3. **Enforcement**: The contract prevents unverified businesses from uploading invoices.

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

- `kyc_sub`: Emitted when a business submits KYC data.
- `bus_ver`: Emitted when a business is verified by admin.
- `bus_rej`: Emitted when a business is rejected by admin.

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

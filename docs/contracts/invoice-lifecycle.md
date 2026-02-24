# Invoice Lifecycle Management

This document describes the invoice lifecycle management functionality in the QuickLendX protocol, including invoice upload, verification/approval, and cancellation.

## Overview

The invoice lifecycle consists of the following states:

1. **Pending** - Invoice uploaded by business, awaiting verification
2. **Verified** - Invoice verified by admin/oracle and available for bidding
3. **Funded** - Invoice has been funded by an investor
4. **Paid** - Invoice has been paid and settled
5. **Defaulted** - Invoice payment is overdue/defaulted
6. **Cancelled** - Invoice has been cancelled by the business owner
7. **Refunded** - Invoice funds have been returned to the investor and the invoice is closed

## Core Functions

### 1. `upload_invoice`

Allows a verified business to upload an invoice to the platform.

**Authorization**: Business owner only (requires authentication)

**Parameters**:

- `env: Env` - Contract environment
- `business: Address` - Address of the business uploading the invoice
- `amount: i128` - Total invoice amount (must be > 0)
- `currency: Address` - Currency token address
- `due_date: u64` - Due date timestamp (must be in the future)
- `description: String` - Invoice description (cannot be empty)
- `category: InvoiceCategory` - Invoice category
- `tags: Vec<String>` - Invoice tags for discoverability

**Returns**: `Result<BytesN<32>, QuickLendXError>` - Invoice ID on success

**Validations**:

- Business must be verified
- Amount must be greater than 0
- Due date must be in the future (after current timestamp)
- Description cannot be empty
- Category must be valid
- Tags must be valid (max 10 tags, 1-50 characters each)

**Events Emitted**:

- `inv_up` (invoice_uploaded) - Contains invoice ID, business address, amount, currency, and due date

**Failure Cases**:

- `BusinessNotVerified` - Business is not verified
- `InvalidAmount` - Amount is <= 0
- `InvoiceDueDateInvalid` - Due date is not in the future
- `InvalidDescription` - Description is empty
- `InvalidTag` - Invalid tag format or limit exceeded

---

### 2. `verify_invoice`

Allows an admin or oracle to verify an uploaded invoice, making it available for investors to bid on.

**Authorization**: Admin only (requires authentication)

**Parameters**:

- `env: Env` - Contract environment
- `invoice_id: BytesN<32>` - ID of the invoice to verify

**Returns**: `Result<(), QuickLendXError>` - Success or error

**Validations**:

- Caller must be an admin
- Invoice must exist
- Invoice status must be `Pending`

**State Transitions**:

- `Pending` → `Verified`

**Events Emitted**:

- `inv_ver` (invoice_verified) - Contains invoice ID and business address

**Failure Cases**:

- `NotAdmin` - Caller is not an admin
- `InvoiceNotFound` - Invoice does not exist
- `InvalidStatus` - Invoice is not in Pending status

---

### 3. `cancel_invoice`

Allows a business to cancel their own invoice before it has been funded by an investor.

**Authorization**: Business owner only (requires authentication)

**Parameters**:

- `env: Env` - Contract environment
- `invoice_id: BytesN<32>` - ID of the invoice to cancel

**Returns**: `Result<(), QuickLendXError>` - Success or error

**Validations**:

- Caller must be the business owner of the invoice
- Invoice must exist
- Invoice status must be either `Pending` or `Verified` (cannot cancel if already funded)

**State Transitions**:

- `Pending` → `Cancelled`
- `Verified` → `Cancelled`

**Events Emitted**:

- `inv_canc` (invoice_cancelled) - Contains invoice ID, business address, and timestamp

**Failure Cases**:

- `InvoiceNotFound` - Invoice does not exist
- `Unauthorized` - Caller is not the business owner
- `InvalidStatus` - Invoice is already funded, paid, defaulted, or cancelled

---

### 4. `refund_escrow_funds`

Allows an admin or the business owner to refund a funded invoice, returning funds to the investor.

**Authorization**: Admin or Business owner (requires authentication)

**Parameters**:

- `env: Env` - Contract environment
- `invoice_id: BytesN<32>` - ID of the invoice to refund
- `caller: Address` - Address of the party initiating the refund

**Returns**: `Result<(), QuickLendXError>` - Success or error

**Validations**:

- Caller must be an admin or the business owner
- Invoice must be in `Funded` status

**State Transitions**:

- `Funded` → `Refunded`

**Related Updates**:

- Bid status → `Cancelled`
- Investment status → `Refunded`
- Escrow status → `Refunded`

**Events Emitted**:

- `esc_ref` (escrow_refunded) - Transferred funds back to investor
- Audit logs for status change and refund

**Failure Cases**:

- `InvoiceNotFound` - Invoice does not exist
- `Unauthorized` - Caller is not authorized (Admin/Business)
- `InvalidStatus` - Invoice is not in Funded status

---

## Authorization Rules

### Business (Invoice Owner)

- Can upload invoices (if verified)
- Can cancel their own invoices (before funding)
- Can refund their own invoices (after funding, before release)
- Can update invoice metadata
- Can update invoice category and tags

### Admin/Oracle

- Can verify invoices
- Can reject verification
- Can set admin address

### Investor

- Cannot directly interact with invoice lifecycle (can only bid on verified invoices)

---

## State Transition Diagram

```
┌─────────┐
│ Pending │ ◄─── Business uploads invoice
└────┬────┘
     │
     │ Admin verifies
     ▼
┌──────────┐
│ Verified │ ◄─── Available for bidding
└────┬─────┘
     │
     │ Investor bids and business accepts
     ▼
┌─────────┐
│ Funded  │ ◄─── Investor has funded the invoice
└────┬────┘
     │
     │ Business pays back
     ▼
┌──────┐
│ Paid │ ◄─── Invoice settled successfully
└──────┘

Alternative paths:
- Pending/Verified → Cancelled (business cancels)
- Funded → Defaulted (payment overdue beyond grace period)
- Funded → Refunded (admin or business refunds)
```

---

## Complete Lifecycle Flow

### 1. Invoice Upload

```rust
// Business uploads an invoice
let invoice_id = upload_invoice(
    env,
    business_address,
    1000000, // amount in stroops
    xlm_address,
    due_date,
    String::from_str(&env, "Payment for services"),
    InvoiceCategory::Services,
    tags
)?;
```

### 2. Invoice Verification

```rust
// Admin verifies the invoice
verify_invoice(env, invoice_id)?;
// Invoice is now available for bidding
```

### 3. Invoice Cancellation (Optional)

```rust
// Business can cancel before funding
cancel_invoice(env, invoice_id)?;
// Invoice is now cancelled and unavailable for bidding
```

---

## Security Considerations

1. **Authentication**: All state-changing operations require proper authentication
   - `upload_invoice`: Business must authenticate
   - `verify_invoice`: Admin must authenticate
   - `cancel_invoice`: Business owner must authenticate

2. **Authorization**: Functions check that the caller has the appropriate role
   - Only verified businesses can upload invoices
   - Only admins can verify invoices
   - Only the business owner can cancel their own invoices

3. **State Validation**: All transitions validate the current state
   - Verification only works on Pending invoices
   - Cancellation only works on Pending or Verified invoices
   - Prevents invalid state transitions

4. **Input Validation**: All inputs are validated
   - Amount must be positive
   - Due date must be in the future
   - Description cannot be empty
   - Currency address must be valid

5. **Audit Logging**: All state changes are logged via the audit system
   - Invoice creation is logged
   - Status changes are logged with actor address
   - Audit trail enables accountability and dispute resolution

---

## Events Reference

All invoice lifecycle events can be monitored by off-chain systems:

| Event Symbol | Event Name        | Data                                               |
| ------------ | ----------------- | -------------------------------------------------- |
| `inv_up`     | invoice_uploaded  | (invoice_id, business, amount, currency, due_date) |
| `inv_ver`    | invoice_verified  | (invoice_id, business)                             |
| `inv_canc`   | invoice_cancelled | (invoice_id, business, timestamp)                  |

---

## Error Codes

| Error                   | Code | Description                             |
| ----------------------- | ---- | --------------------------------------- |
| `InvoiceNotFound`       | 1000 | Invoice does not exist                  |
| `InvalidAmount`         | 1200 | Amount is invalid (e.g., <= 0)          |
| `InvoiceDueDateInvalid` | 1005 | Due date is not in the future           |
| `InvalidDescription`    | 1204 | Description is empty                    |
| `InvalidStatus`         | 1401 | Operation not allowed in current status |
| `BusinessNotVerified`   | 1600 | Business is not verified                |
| `NotAdmin`              | 1103 | Caller is not an admin                  |
| `Unauthorized`          | 1100 | Caller is not authorized                |
| `InvalidTag`            | 1802 | Invalid tag format                      |
| `TagLimitExceeded`      | 1803 | Too many tags (max 10)                  |

---

## Testing Requirements

Comprehensive tests should cover:

1. **Happy Path**:
   - Upload invoice with valid parameters
   - Verify invoice by admin
   - Cancel invoice by business owner

2. **Authorization**:
   - Non-business cannot upload invoice
   - Non-admin cannot verify invoice
   - Non-owner cannot cancel invoice

3. **Validation**:
   - Negative amount rejected
   - Past due date rejected
   - Empty description rejected
   - Invalid category rejected

4. **State Transitions**:
   - Cannot verify non-pending invoice
   - Cannot cancel funded invoice
   - Cannot cancel already cancelled invoice

5. **Edge Cases**:
   - Cancel immediately after upload
   - Cancel after verification but before funding
   - Attempt to cancel after funding (should fail)

Minimum test coverage: **95%**

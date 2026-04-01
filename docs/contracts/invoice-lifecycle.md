# Invoice Lifecycle Management

This document describes the invoice lifecycle management functionality in the QuickLendX protocol, including invoice upload, verification/approval, and cancellation.

> Note
> `update_invoice_status` is an admin-only recovery/backfill pathway. It is not a
> replacement for the normal `accept_bid`, `settle_invoice`, or overdue-default
> flows, and it intentionally avoids escrow and payment side effects.

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
### 4. `update_invoice_status`

Allows the configured admin to move an invoice through a limited recovery path
when tests, migrations, or operational repair require a manual state correction.

**Authorization**: Admin only (requires authentication)

**Parameters**:
- `env: Env` - Contract environment
- `invoice_id: BytesN<32>` - ID of the invoice to update
- `new_status: InvoiceStatus` - Target lifecycle status

**Returns**: `Result<(), QuickLendXError>` - Success or error

**Supported transitions**:
- `Pending` → `Verified`
- `Verified` → `Funded`
- `Funded` → `Paid`
- `Funded` → `Defaulted`

**Unsupported transitions**:
- Any transition targeting `Pending`, `Cancelled`, or `Refunded`
- Any transition from terminal invoices (`Cancelled`, `Refunded`)
- Any transition that skips the supported recovery path, such as `Verified` → `Paid`

**Index updates**:
- Removes the invoice ID from the previous status bucket before persisting
- Adds the invoice ID to the new status bucket after persisting
- Keeps `get_invoices_by_status` and `get_invoice_count_by_status` aligned

**Events Emitted**:
- `inv_ver` when moving to `Verified`
- `inv_fnd` when moving to `Funded`
- `inv_set` when moving to `Paid` through the admin override path
- `inv_def` when moving to `Defaulted`

**Security Notes**:
- The function requires the stored admin address and fails with `NotAdmin` if none is configured
- Manual `Paid` updates emit the canonical settlement event with zeroed settlement values because no payment transfer is executed by this pathway
- Manual `Funded` updates are bookkeeping-only and do not create escrow or investment records
- Production flows should prefer `verify_invoice`, `accept_bid`, `settle_invoice`, and `mark_invoice_defaulted`

**Failure Cases**:
- `NotAdmin` - No admin configured
- `InvoiceNotFound` - Invoice does not exist
- `InvalidStatus` - Unsupported target status or invalid transition

---
### 5. `refund_escrow_funds`

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
- Can run the constrained `update_invoice_status` recovery path
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
2. **Recovery pathway isolation**: `update_invoice_status` is admin-only and does not move funds
3. **Index consistency**: Status-list removals/additions happen in the same override operation
4. **Canonical events**: Admin overrides emit the same lifecycle topics used by normal flows so indexers do not need a separate schema
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

| Event Symbol | Event Name | Data |
|-------------|------------|------|
| `inv_up` | invoice_uploaded | (invoice_id, business, amount, currency, due_date) |
| `inv_ver` | invoice_verified | (invoice_id, business) |
| `inv_canc` | invoice_cancelled | (invoice_id, business, timestamp) |

---

## Error Codes

| Error | Code | Description |
|-------|------|-------------|
| `InvoiceNotFound` | 1000 | Invoice does not exist |
| `InvalidAmount` | 1200 | Amount is invalid (e.g., <= 0) |
| `InvoiceDueDateInvalid` | 1005 | Due date is not in the future |
| `InvalidDescription` | 1204 | Description is empty |
| `InvalidStatus` | 1401 | Operation not allowed in current status |
| `BusinessNotVerified` | 1600 | Business is not verified |
| `NotAdmin` | 1103 | Caller is not an admin |
| `Unauthorized` | 1100 | Caller is not authorized |
| `InvalidTag` | 1802 | Invalid tag format |
| `TagLimitExceeded` | 1803 | Too many tags (max 10) |

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

---

## Investment Status Lifecycle (Issue #556)

When an invoice transitions through its lifecycle, the associated investment's status must be updated atomically. This section documents the investment status state machine and its integration with settlement and default events.

### Investment Status States

| Status | Description |
|--------|-------------|
| `Active` | Investment is live; funds are held in escrow |
| `Completed` | Invoice was fully settled; investor receives principal + yield |
| `Defaulted` | Invoice was not paid within the grace period |
| `Refunded` | Escrow was refunded before settlement |
| `Withdrawn` | Investor withdrew before the invoice was funded |

### Allowed Transitions

Only `Active` investments can transition. All other states are terminal.

```
Active ──► Completed   (full settlement via settle_invoice)
Active ──► Defaulted   (overdue via mark_invoice_defaulted)
Active ──► Refunded    (escrow refund via refund_escrow_funds)
Active ──► Withdrawn   (investor withdrawal before funding)

Completed ──► (terminal)
Defaulted ──► (terminal)
Refunded  ──► (terminal)
Withdrawn ──► (terminal)
```

Any attempt to transition from a terminal state panics with `QuickLendXError::InvalidStatus`, preventing double-settle, double-default, or any backward transition.

### Active Investment Index

A persistent `act_inv` index tracks all `Active` investment IDs:

- **Added** when `store_investment` is called (new investments always start `Active`)
- **Removed** atomically when `update_investment` transitions away from `Active`

This index enables O(n) orphan detection and off-chain monitoring without full storage scans.

### Orphan Prevention

`validate_no_orphan_investments` scans the active index and verifies every listed investment still has `status == Active`. Returns `false` if any entry has a terminal status — indicating a bug in the transition path.

```rust
// After any settlement or default event:
assert!(client.validate_no_orphan_investments());
```

### Integration Points

#### Settlement (`settle_invoice`)
```rust
// src/settlement.rs — full settlement path
updated_investment.status = InvestmentStatus::Completed;
InvestmentStorage::update_investment(env, &updated_investment);
// Active index entry removed automatically
```

#### Default (`mark_invoice_defaulted`)
```rust
// src/defaults.rs
investment.status = InvestmentStatus::Defaulted;
InvestmentStorage::update_investment(env, &investment);
// Active index entry removed automatically
```

#### Refund (`refund_escrow_funds`)
```rust
// src/escrow.rs
investment.status = InvestmentStatus::Refunded;
InvestmentStorage::update_investment(env, &investment);
// Active index entry removed automatically
```

### Security Assumptions

1. **Transition guard is mandatory** — `update_investment` always calls `validate_transition` before persisting. No code path can bypass it.
2. **Index consistency** — The active index is updated inside the same `update_investment` call as the status write; there is no window where the index and storage can diverge.
3. **Terminal states are irreversible** — Once an investment reaches `Completed`, `Defaulted`, `Refunded`, or `Withdrawn`, no further transitions are possible.
4. **No orphan active investments** — After every terminal lifecycle event, `validate_no_orphan_investments` must return `true`.

### Test Coverage (test_investment_lifecycle.rs)

| Test | Scenario |
|------|----------|
| `test_settlement_sets_investment_completed` | Full settlement → `Completed`, removed from active index |
| `test_settlement_invoice_status_paid` | Invoice status is `Paid` after settlement |
| `test_default_sets_investment_defaulted` | Default event → `Defaulted`, removed from active index |
| `test_default_invoice_status_defaulted` | Invoice status is `Defaulted` after default |
| `test_refund_sets_investment_refunded` | Refund → `Refunded`, no orphan |
| `test_completed_to_defaulted_rejected` | Terminal → terminal rejected |
| `test_defaulted_to_completed_rejected` | Terminal → terminal rejected |
| `test_refunded_to_active_rejected` | Terminal → Active rejected |
| `test_withdrawn_to_completed_rejected` | Terminal → terminal rejected |
| `test_active_valid_transitions_accepted` | All four Active transitions accepted |
| `test_double_settle_rejected` | Second settle fails |
| `test_double_default_rejected` | Second default fails with `InvoiceAlreadyDefaulted` |
| `test_partial_payment_keeps_investment_active` | Partial payment leaves investment `Active` |
| `test_multiple_investments_independent_transitions` | Two investments transition independently |
| `test_active_index_grows_and_shrinks` | Index size tracks lifecycle events |
| `test_validate_no_orphan_empty_state` | Returns `true` on empty state |
| `test_validate_no_orphan_after_funding` | Returns `true` when all active entries are genuinely Active |

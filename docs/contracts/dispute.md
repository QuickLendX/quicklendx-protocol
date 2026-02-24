# Dispute Resolution

## Overview

Complete dispute lifecycle management for invoice financing disputes. Enables business owners and investors to raise disputes on funded or settled invoices, with admin-controlled review and resolution process.

## Dispute Lifecycle

```
Open → UnderReview → Resolved
```

1. **Open**: Dispute created by business or investor
2. **UnderReview**: Admin has acknowledged and is investigating
3. **Resolved**: Admin has provided final resolution

## Data Structure

### Dispute

| Field         | Type             | Description                              |
| ------------- | ---------------- | ---------------------------------------- |
| `invoice_id`  | `u64`            | Associated invoice identifier            |
| `creator`     | `Address`        | Dispute initiator (business or investor) |
| `reason`      | `String`         | Dispute reason (1-500 chars)             |
| `evidence`    | `String`         | Supporting evidence (0-2000 chars)       |
| `status`      | `DisputeStatus`  | Current lifecycle stage                  |
| `resolution`  | `Option<String>` | Admin resolution text (0-1000 chars)     |
| `created_at`  | `u64`            | Creation timestamp                       |
| `resolved_at` | `Option<u64>`    | Resolution timestamp                     |

### DisputeStatus

```rust
pub enum DisputeStatus {
    Open,           // Initial state
    UnderReview,    // Admin reviewing
    Resolved,       // Final state
}
```

## Contract Interface

### User Functions

#### `create_dispute(invoice_id: u64, creator: Address, reason: String, evidence: String) -> Result<(), QuickLendXError>`

Creates a new dispute for a funded or settled invoice.

**Preconditions:**

- Invoice must be in Funded or Settled status
- Creator must be either business owner or investor on the invoice
- No existing dispute for this invoice
- Reason must be 1-500 characters
- Evidence must be ≤2000 characters

**Errors:**

- `DisputeAlreadyExists`: Dispute already exists for this invoice
- `InvoiceNotAvailableForFunding`: Invoice not in valid state
- `DisputeNotAuthorized`: Creator is not business or investor
- `InvoiceNotFound`: Invoice does not exist
- `InvalidDisputeReason`: Reason empty or exceeds 500 chars
- `InvalidDisputeEvidence`: Evidence exceeds 2000 chars

### Admin Functions

#### `put_dispute_under_review(admin: Address, invoice_id: u64) -> Result<(), QuickLendXError>`

Moves dispute from Open to UnderReview status.

**Preconditions:**

- Caller must be admin
- Dispute must exist
- Dispute status must be Open

**Errors:**

- `Unauthorized`: Caller not admin
- `NotAdmin`: Admin not configured
- `DisputeNotFound`: No dispute for this invoice
- `InvalidStatus`: Dispute not in Open status

#### `resolve_dispute(admin: Address, invoice_id: u64, resolution: String) -> Result<(), QuickLendXError>`

Finalizes dispute with resolution text.

**Preconditions:**

- Caller must be admin
- Dispute must exist
- Dispute status must be UnderReview
- Resolution must be 1-1000 characters

**Errors:**

- `Unauthorized`: Caller not admin
- `NotAdmin`: Admin not configured
- `DisputeNotFound`: No dispute for this invoice
- `DisputeNotUnderReview`: Dispute not in UnderReview status
- `DisputeAlreadyResolved`: Dispute already resolved
- `InvalidDisputeEvidence`: Resolution empty or exceeds 1000 chars

### Query Functions

#### `get_dispute_details(invoice_id: u64) -> Result<Dispute, QuickLendXError>`

Retrieves complete dispute information.

**Errors:**

- `DisputeNotFound`: No dispute for this invoice

#### `get_disputes_by_status(status: DisputeStatus, start: u64, limit: u32) -> Vec<Dispute>`

Paginated query of disputes by status. Maximum 50 results per query.

**Parameters:**

- `status`: Filter by dispute status
- `start`: Starting invoice ID for pagination
- `limit`: Maximum results (capped at 50)

#### `initialize(admin: Address) -> Result<(), QuickLendXError>`

One-time initialization with admin designation.

**Errors:**

- `OperationNotAllowed`: Already initialized

## Integration

### Invoice State Requirements

Disputes can only be created for invoices in specific states:

```rust
// Valid invoice states for dispute creation
match invoice_status {
    InvoiceStatus::Funded => Ok(()),
    InvoiceStatus::Settled => Ok(()),
    _ => Err(QuickLendXError::InvoiceNotAvailableForFunding),
}
```

### Authorization Model

**Create Dispute:**

- Business owner of the invoice
- Investor who funded the invoice

**Review/Resolve:**

- Platform admin only

### Usage Example

```rust
// Business creates dispute
create_dispute(
    env.clone(),
    invoice_id,
    business_address,
    String::from_str(&env, "Payment not received after due date"),
    String::from_str(&env, "Transaction ID: ABC123, Expected: 2025-01-15")
)?;

// Admin puts under review
put_dispute_under_review(
    env.clone(),
    admin_address,
    invoice_id
)?;

// Admin resolves
resolve_dispute(
    env.clone(),
    admin_address,
    invoice_id,
    String::from_str(&env, "Verified payment delay. Instructed business to release funds.")
)?;

// Query dispute
let dispute = get_dispute_details(env.clone(), invoice_id)?;
assert_eq!(dispute.status, DisputeStatus::Resolved);
```

## Validation Rules

### Field Length Constraints

| Field      | Minimum | Maximum    |
| ---------- | ------- | ---------- |
| Reason     | 1 char  | 500 chars  |
| Evidence   | 0 chars | 2000 chars |
| Resolution | 1 char  | 1000 chars |

### State Transition Rules

| Current Status | Allowed Transition | Required Role |
| -------------- | ------------------ | ------------- |
| Open           | UnderReview        | Admin         |
| UnderReview    | Resolved           | Admin         |
| Resolved       | None               | -             |

### Invoice State Requirements

| Invoice Status | Can Create Dispute |
| -------------- | ------------------ |
| Pending        | ❌                 |
| Funded         | ✅                 |
| Settled        | ✅                 |
| Defaulted      | ❌                 |

## Security Considerations

**Authorization:**

- Creator verification ensures only invoice participants can dispute
- Admin-only review and resolution prevents unauthorized modifications
- Authentication required for all state-changing operations

**Data Integrity:**

- One dispute per invoice prevents spam
- Immutable creator and creation timestamp
- Resolution can only be set once
- Status transitions follow strict lifecycle

**Input Validation:**

- Length limits prevent storage abuse
- Empty reason/resolution rejected
- Evidence optional but bounded

**Access Control:**

- Admin address stored in instance storage
- Admin verification on every privileged operation
- Separate user and admin function namespaces

## Error Handling

All operations return `Result<T, QuickLendXError>`:

| Error                    | Code | Condition                             |
| ------------------------ | ---- | ------------------------------------- |
| `DisputeNotFound`        | 1037 | Dispute does not exist                |
| `DisputeAlreadyExists`   | 1038 | Duplicate dispute creation            |
| `DisputeNotAuthorized`   | 1039 | Unauthorized creator                  |
| `DisputeAlreadyResolved` | 1040 | Dispute already finalized             |
| `DisputeNotUnderReview`  | 1041 | Invalid status for resolution         |
| `InvalidDisputeReason`   | 1042 | Reason validation failed              |
| `InvalidDisputeEvidence` | 1043 | Evidence/resolution validation failed |
| `Unauthorized`           | 1004 | Admin verification failed             |
| `NotAdmin`               | 1005 | Admin not configured                  |
| `InvoiceNotFound`        | 1000 | Invoice does not exist                |
| `InvalidStatus`          | 1003 | Invalid state transition              |

## Query Patterns

### Get Single Dispute

```rust
let dispute = get_dispute_details(env, invoice_id)?;
```

### Get All Open Disputes

```rust
let open_disputes = get_disputes_by_status(env, DisputeStatus::Open, 0, 50);
```

### Paginate Through Disputes

```rust
let page1 = get_disputes_by_status(env.clone(), DisputeStatus::Resolved, 0, 50);
let page2 = get_disputes_by_status(env.clone(), DisputeStatus::Resolved, 50, 50);
```

## Deployment Checklist

- [ ] Initialize contract with admin address
- [ ] Verify admin authorization works correctly
- [ ] Confirm dispute creation restricted to funded/settled invoices
- [ ] Test state transitions (Open → UnderReview → Resolved)
- [ ] Validate field length constraints
- [ ] Verify only invoice participants can create disputes
- [ ] Test pagination with get_disputes_by_status
- [ ] Document admin dispute resolution procedures
- [ ] Set up monitoring for open disputes

## Future Enhancements

- Dispute appeal mechanism
- Automated dispute categorization
- Multi-party disputes (beyond business/investor)
- Dispute metrics and analytics
- Integration with notification system
- Evidence file attachments support
- Dispute escalation timers

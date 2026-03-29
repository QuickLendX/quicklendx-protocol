# Dispute Resolution

## Overview

Complete dispute lifecycle management for invoice financing disputes. Enables business owners and investors to raise disputes on funded or settled invoices, with admin-controlled review and resolution process.

## Dispute Lifecycle

```
Disputed → UnderReview → Resolved
```

1. **Disputed**: Dispute created by business or investor (initial state)
2. **UnderReview**: Admin has acknowledged and is investigating
3. **Resolved**: Admin has provided final resolution (terminal state)

## Data Structure

### Dispute

| Field | Type | Description |
|-------|------|-------------|
| `created_by` | `Address` | Dispute initiator (business or investor) |
| `created_at` | `u64` | Creation timestamp |
| `reason` | `String` | Dispute reason (1-1000 chars) |
| `evidence` | `String` | Supporting evidence (1-2000 chars) |
| `resolution` | `String` | Admin resolution text (1-2000 chars, empty until resolved) |
| `resolved_by` | `Address` | Admin who resolved (placeholder until resolved) |
| `resolved_at` | `u64` | Resolution timestamp (0 until resolved) |

### DisputeStatus

```rust
pub enum DisputeStatus {
    Disputed,       // Initial state after creation
    UnderReview,    // Admin reviewing
    Resolved,       // Final terminal state
}
```

## Contract Interface

### User Functions

#### `create_dispute(invoice_id: BytesN<32>, creator: Address, reason: String, evidence: String) -> Result<(), QuickLendXError>`

Creates a new dispute for a funded or settled invoice.

**Preconditions**:
- Invoice must exist in storage
- Creator must be either business owner or investor on the invoice
- No existing dispute for this invoice (one dispute per invoice)
- Reason must be 1-1000 characters
- Evidence must be 1-2000 characters

**Errors:**
- `DisputeAlreadyExists`: Dispute already exists for this invoice
- `InvoiceNotAvailableForFunding`: Invoice not in valid state
- `DisputeNotAuthorized`: Creator is not business or investor
- `InvoiceNotFound`: Invoice does not exist
- `InvalidDisputeReason`: Reason empty or exceeds 1000 chars
- `InvalidDisputeEvidence`: Evidence empty or exceeds 2000 chars

### Admin Functions

#### `put_dispute_under_review(admin: Address, invoice_id: BytesN<32>) -> Result<(), QuickLendXError>`

Moves dispute from Disputed to UnderReview status.

**Preconditions:**
- Caller must be admin
- Dispute must exist
- Dispute status must be Disputed

**Errors:**
- `Unauthorized`: Caller not admin
- `NotAdmin`: Admin not configured
- `DisputeNotFound`: No dispute for this invoice
- `InvalidStatus`: Dispute not in Disputed status

#### `resolve_dispute(admin: Address, invoice_id: BytesN<32>, resolution: String) -> Result<(), QuickLendXError>`

Finalizes dispute with resolution text.

**Preconditions:**
- Caller must be admin
- Dispute must exist
- Dispute status must be UnderReview
- Resolution must be 1-2000 characters

**Errors:**
- `Unauthorized`: Caller not admin
- `NotAdmin`: Admin not configured
- `DisputeNotFound`: No dispute for this invoice
- `DisputeNotUnderReview`: Dispute not in UnderReview status
- `DisputeAlreadyResolved`: Dispute already resolved
- `InvalidDisputeEvidence`: Resolution empty or exceeds 2000 chars

### Query Functions

#### `get_dispute_details(invoice_id: BytesN<32>) -> Option<Dispute>`

Retrieves complete dispute information.

Returns `None` if no dispute exists, otherwise returns complete dispute information.

**Note**: This function does not return errors - use `Option` pattern instead.

#### `get_invoices_with_disputes() -> Vec<BytesN<32>>`

Returns all invoice IDs that have disputes in any state.

**Return Value**:
- Vector of invoice IDs with active disputes

#### `get_invoices_by_dispute_status(status: DisputeStatus) -> Vec<BytesN<32>>`

Returns invoice IDs filtered by specific dispute status.

**Parameters**:
- `status`: Filter by dispute status (Disputed, UnderReview, or Resolved)

**Return Value**:
- Vector of invoice IDs matching the status

## Integration

### Integration with Invoice Module

Disputes are stored as part of the `Invoice` struct in the invoice module. The dispute-related fields on `Invoice` include:

```rust
pub struct Invoice {
    // ... other fields ...
    pub dispute_status: DisputeStatus,  // Tracks lifecycle
    pub dispute: Option<Dispute>,       // Dispute details when present
}
```

When a dispute is created, the invoice's `dispute_status` is set to `DisputeStatus::Disputed`, preventing further funding operations on that invoice.

### Authorization Model

**Create Dispute:**
- Business owner of the invoice
- Investor who funded the invoice

**Review/Resolve:**
- Platform admin only

### Usage Example

```rust
// Business creates dispute
let invoice_id = /* 32-byte identifier */;
create_dispute(
    env.clone(),
    &invoice_id,
    &business_address,
    String::from_str(&env, "Payment not received after due date"),
    String::from_str(&env, "Transaction ID: ABC123, Expected: 2025-01-15")
)?;

// Admin puts under review
put_dispute_under_review(
    env.clone(),
    &admin_address,
    &invoice_id
)?;

// Admin resolves
resolve_dispute(
    env.clone(),
    &admin_address,
    &invoice_id,
    String::from_str(&env, "Verified payment delay. Instructed business to release funds.")
)?;

// Query dispute
let dispute = get_dispute_details(env.clone(), &invoice_id);
assert_eq!(dispute.unwrap().status, DisputeStatus::Resolved);

// Get all disputed invoices
let all_disputed = get_invoices_with_disputes(env.clone());

// Get disputes by status
let under_review = get_invoices_by_dispute_status(env.clone(), DisputeStatus::UnderReview);
```

## Validation Rules

### Field Length Constraints

| Field | Minimum | Maximum |
|-------|---------|--------|
| Reason | 1 char | 1000 chars |
| Evidence | 1 char | 2000 chars |
| Resolution | 1 char | 2000 chars |

### State Transition Rules

| Current Status | Allowed Transition | Required Role |
|----------------|-------------------|---------------|
| Disputed | UnderReview | Admin |
| UnderReview | Resolved | Admin |
| Resolved | None (terminal) | - |

### Invoice State Requirements

| Invoice Status | Can Create Dispute |
|----------------|-------------------|
| Pending | ❌ |
| Funded | ✅ |
| Settled | ✅ |
| Defaulted | ❌ |

## Security Considerations

**Authorization:**
- Creator verification via `require_auth()` ensures only invoice participants can dispute
- Admin-only review and resolution prevents unauthorized modifications
- Dual-check system: cryptographic signature + role verification against stored admin
- Forward-only state transitions prevent reverting to previous states

**Data Integrity:**
- One dispute per invoice prevents spam and storage bloat
- Immutable creator and creation timestamp once dispute is opened
- Resolution fields (`resolved_by`, `resolved_at`, `resolution`) set atomically on resolve
- Status transitions enforced: cannot skip `UnderReview` or revert from `Resolved`

**Input Validation:**
- Length limits on reason (1-1000), evidence (1-2000), resolution (1-2000) prevent storage abuse
- Empty strings rejected for all required fields
- Invoice existence verified before dispute creation

**Access Control:**
- Admin address stored in instance storage under `ADMIN_KEY` symbol
- Admin verification on every privileged operation (`put_dispute_under_review`, `resolve_dispute`)
- Separate user and admin function namespaces with clear role boundaries
- Business/investor can only create disputes, never advance or resolve them

## Error Handling

All operations return `Result<T, QuickLendXError>`:

| Error | Code | Condition |
|-------|------|-----------|
| `InvoiceNotFound` | 1000 | Invoice does not exist |
| `InvalidStatus` | 1003 | Invalid state transition |
| `Unauthorized` | 1004 | Admin verification failed |
| `NotAdmin` | 1005 | Admin not configured or caller mismatch |
| `DisputeNotFound` | 1037 | No dispute exists on this invoice |
| `DisputeAlreadyExists` | 1038 | Duplicate dispute creation attempt |
| `DisputeNotAuthorized` | 1039 | Caller is not business or investor |
| `DisputeAlreadyResolved` | 1040 | Attempting to resolve already-resolved dispute |
| `DisputeNotUnderReview` | 1041 | Attempting to resolve without reviewing first |
| `InvalidDisputeReason` | 1042 | Reason validation failed (empty or too long) |
| `InvalidDisputeEvidence` | 1043 | Evidence/resolution validation failed (empty or too long) |

## Query Patterns

### Get Single Dispute
```rust
let maybe_dispute = get_dispute_details(env, &invoice_id);
match maybe_dispute {
    Some(dispute) => {
        println!("Dispute status: {:?}", dispute.status);
        println!("Reason: {}", dispute.reason);
    },
    None => println!("No dispute on this invoice"),
}
```

### Get All Disputed Invoices
```rust
let disputed_invoices = get_invoices_with_disputes(env);
for invoice_id in disputed_invoices.iter() {
    println!("Invoice {:?} has a dispute", invoice_id);
}
```

### Get Disputes by Status
```rust
let under_review = get_invoices_by_dispute_status(env, DisputeStatus::UnderReview);
let resolved = get_invoices_by_dispute_status(env, DisputeStatus::Resolved);
let disputed = get_invoices_by_dispute_status(env, DisputeStatus::Disputed);

## Deployment Checklist

- [ ] Initialize contract with admin address via `set_admin`
- [ ] Verify admin authorization works correctly (test non-admin rejection)
- [ ] Confirm dispute creation restricted to business/investor only
- [ ] Test complete state machine: Disputed → UnderReview → Resolved
- [ ] Validate field length constraints (reason 1-1000, evidence 1-2000, resolution 1-2000)
- [ ] Verify one-dispute-per-invoice enforcement
- [ ] Test query functions return correct results for each status
- [ ] Verify multi-invoice isolation (disputes don't interfere)
- [ ] Document admin dispute resolution procedures
- [ ] Set up monitoring for disputes stuck in UnderReview status

## Future Enhancements

- Dispute appeal mechanism
- Automated dispute categorization
- Multi-party disputes (beyond business/investor)
- Dispute metrics and analytics
- Integration with notification system
- Evidence file attachments support
- Dispute escalation timers
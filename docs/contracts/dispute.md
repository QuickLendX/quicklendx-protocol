# Dispute Resolution

## Overview

Complete dispute lifecycle management for invoice financing disputes. Enables business owners and investors to raise disputes on funded or settled invoices, with admin-controlled review and resolution process.

Dispute data is embedded within the `Invoice` struct to keep dispute state co-located with the invoice it belongs to. All string fields are bounded by protocol-enforced limits to prevent abusive on-chain storage growth.

## Dispute Lifecycle

```
None → Disputed → UnderReview → Resolved
```

1. **None**: No dispute exists (default state)
2. **Disputed**: Dispute created by business or investor
3. **UnderReview**: Admin has acknowledged and is investigating
4. **Resolved**: Admin has provided final resolution

## Data Structure

### DisputeStatus

```rust
pub enum DisputeStatus {
    None,        // No dispute exists (default)
    Disputed,    // Dispute has been created
    UnderReview, // Admin reviewing
    Resolved,    // Final state
}
```

### Dispute

| Field | Type | Description |
|-------|------|-------------|
| `created_by` | `Address` | Dispute initiator (business or investor) |
| `created_at` | `u64` | Creation timestamp |
| `reason` | `String` | Dispute reason (1–1000 chars) |
| `evidence` | `String` | Supporting evidence (1–2000 chars) |
| `resolution` | `String` | Admin resolution text (1–2000 chars when set) |
| `resolved_by` | `Address` | Admin who resolved the dispute |
| `resolved_at` | `u64` | Resolution timestamp (0 if unresolved) |

## Input Validation — Storage Growth Prevention

All text fields are validated against protocol limits defined in `protocol_limits.rs` to prevent adversarial callers from inflating on-chain storage costs with oversized payloads.

### Field Length Constraints

| Field | Minimum | Maximum | Constant | Error Code |
|-------|---------|---------|----------|------------|
| Reason | 1 char | 1000 chars | `MAX_DISPUTE_REASON_LENGTH` | `InvalidDisputeReason` (1905) |
| Evidence | 1 char | 2000 chars | `MAX_DISPUTE_EVIDENCE_LENGTH` | `InvalidDisputeEvidence` (1906) |
| Resolution | 1 char | 2000 chars | `MAX_DISPUTE_RESOLUTION_LENGTH` | `InvalidDisputeReason` (1905) |

### Validation Functions (`verification.rs`)

| Function | Validates | Rejects |
|----------|-----------|---------|
| `validate_dispute_reason(reason)` | Non-empty, ≤ 1000 chars | Empty or oversized reason |
| `validate_dispute_evidence(evidence)` | Non-empty, ≤ 2000 chars | Empty or oversized evidence |
| `validate_dispute_resolution(resolution)` | Non-empty, ≤ 2000 chars | Empty or oversized resolution |
| `validate_dispute_eligibility(invoice, creator)` | Invoice status, authorization, no duplicate | Ineligible invoices |

### Security Assumptions

- **No empty payloads**: Empty reason or evidence is rejected to prevent frivolous disputes.
- **Bounded storage**: Maximum total dispute payload per invoice ≤ 5000 chars (reason + evidence + resolution).
- **One dispute per invoice**: Prevents spam by allowing only a single dispute per invoice.
- **Immutable once created**: Dispute creator and creation timestamp cannot be modified after creation.

## Contract Interface

### User Functions

#### `create_dispute(invoice_id: BytesN<32>, creator: Address, reason: String, evidence: String) -> Result<(), QuickLendXError>`

Creates a new dispute for an invoice.

**Preconditions:**
- `creator.require_auth()` passes
- Invoice exists and is in Pending, Verified, Funded, or Paid status
- Creator is either business owner or investor on the invoice
- No existing dispute for this invoice (`dispute_status == None`)
- Reason: 1–1000 characters (non-empty, bounded)
- Evidence: 1–2000 characters (non-empty, bounded)

**Errors:**
- `InvoiceNotFound`: Invoice does not exist
- `InvoiceNotAvailableForFunding`: Invoice not in valid state for disputes
- `DisputeNotAuthorized`: Creator is not business or investor
- `DisputeAlreadyExists`: Dispute already exists for this invoice
- `InvalidDisputeReason` (1905): Reason empty or exceeds 1000 chars
- `InvalidDisputeEvidence` (1906): Evidence empty or exceeds 2000 chars

### Admin Functions

#### `put_dispute_under_review(invoice_id: BytesN<32>, admin: Address) -> Result<(), QuickLendXError>`

Moves dispute from Disputed to UnderReview status.

**Preconditions:**
- Caller must be admin
- Invoice exists
- Dispute status must be Disputed

**Errors:**
- `Unauthorized`: Caller not admin
- `NotAdmin`: Admin not configured
- `InvoiceNotFound`: Invoice does not exist
- `DisputeNotFound`: No dispute exists (status is not Disputed)

#### `resolve_dispute(invoice_id: BytesN<32>, admin: Address, resolution: String) -> Result<(), QuickLendXError>`

Finalizes dispute with resolution text.

**Preconditions:**
- Caller must be admin
- Dispute must be in UnderReview status
- Resolution: 1–2000 characters (non-empty, bounded)

**Errors:**
- `Unauthorized`: Caller not admin
- `NotAdmin`: Admin not configured
- `InvoiceNotFound`: Invoice does not exist
- `DisputeNotUnderReview`: Dispute not in UnderReview status
- `InvalidDisputeReason` (1905): Resolution empty or exceeds 2000 chars

### Query Functions

#### `get_dispute_details(invoice_id: BytesN<32>) -> Option<Dispute>`

Returns dispute details if a dispute exists, `None` otherwise.

#### `get_invoice_dispute_status(invoice_id: BytesN<32>) -> DisputeStatus`

Returns the current dispute status for an invoice.

#### `get_invoices_with_disputes() -> Vec<BytesN<32>>`

Returns all invoice IDs that have an active or resolved dispute (status != None).

#### `get_invoices_by_dispute_status(status: DisputeStatus) -> Vec<BytesN<32>>`

Returns invoice IDs filtered by the given dispute status.

## Integration

### Invoice State Requirements

Disputes can only be created for invoices in specific states:

| Invoice Status | Can Create Dispute |
|----------------|-------------------|
| Pending | Yes |
| Verified | Yes |
| Funded | Yes |
| Paid | Yes |
| Defaulted | No |
| Cancelled | No |

### Authorization Model

**Create Dispute:**
- Business owner of the invoice
- Investor who funded the invoice

**Review/Resolve:**
- Platform admin only

### Usage Example

```rust
// Business creates dispute
client.create_dispute(
    &invoice_id,
    &business_address,
    &String::from_str(&env, "Payment not received after due date"),
    &String::from_str(&env, "Transaction ID: ABC123, Expected: 2025-01-15"),
);

// Admin puts under review
client.put_dispute_under_review(&invoice_id, &admin_address);

// Admin resolves
client.resolve_dispute(
    &invoice_id,
    &admin_address,
    &String::from_str(&env, "Verified payment delay. Instructed business to release funds."),
);

// Query dispute
let dispute = client.get_dispute_details(&invoice_id);
assert!(dispute.is_some());
```

## State Transition Rules

| Current Status | Allowed Transition | Required Role |
|----------------|-------------------|---------------|
| None | Disputed | Business / Investor |
| Disputed | UnderReview | Admin |
| UnderReview | Resolved | Admin |
| Resolved | None (terminal) | - |

## Error Handling

All operations return `Result<T, QuickLendXError>`:

| Error | Code | Symbol | Condition |
|-------|------|--------|-----------|
| `DisputeNotFound` | 1900 | `DSP_NF` | Dispute does not exist |
| `DisputeAlreadyExists` | 1901 | `DSP_EX` | Duplicate dispute creation |
| `DisputeNotAuthorized` | 1902 | `DSP_NA` | Unauthorized creator |
| `DisputeAlreadyResolved` | 1903 | `DSP_RS` | Dispute already finalized |
| `DisputeNotUnderReview` | 1904 | `DSP_UR` | Invalid status for resolution |
| `InvalidDisputeReason` | 1905 | `DSP_RN` | Reason/resolution validation failed |
| `InvalidDisputeEvidence` | 1906 | `DSP_EV` | Evidence validation failed |

## Test Coverage

Test suites: `test_dispute.rs`, `test_string_limits.rs`, and `test.rs`.

### Covered Scenarios

1. **Dispute Creation** (8 tests):
   - Business can create dispute
   - Unauthorized parties rejected
   - Duplicate disputes rejected
   - Reason validation: empty, too long, boundary (1 char, 1000 chars)
   - Evidence validation: empty, too long
   - Nonexistent invoice rejected

2. **Status Transitions** (6 tests):
   - Disputed → UnderReview (admin only)
   - UnderReview → Resolved (admin only)
   - Invalid transitions rejected
   - Cannot re-review resolved disputes
   - Cannot resolve non-reviewed disputes

3. **Resolution Validation** (2 tests):
   - Empty resolution rejected
   - Oversized resolution rejected

4. **Query Functions** (7 tests):
   - get_dispute_details returns correct data
   - get_invoices_with_disputes lists all disputed invoices
   - get_invoices_by_dispute_status filters by status (None, Disputed, UnderReview, Resolved)
   - Status lists update correctly during transitions
   - Multiple disputes on different invoices

5. **String Limits** (1 test in test_string_limits.rs):
   - Dispute reason and evidence at exact boundary

**Estimated Coverage: 95%+**

## Deployment Checklist

- [ ] Initialize contract with admin address
- [ ] Verify admin authorization works correctly
- [ ] Confirm dispute creation restricted to eligible invoice states
- [ ] Test state transitions (None → Disputed → UnderReview → Resolved)
- [ ] Validate field length constraints (reason ≤ 1000, evidence ≤ 2000, resolution ≤ 2000)
- [ ] Verify empty payloads are rejected
- [ ] Verify only invoice participants can create disputes
- [ ] Test query functions (get_dispute_details, get_invoices_with_disputes, get_invoices_by_dispute_status)
- [ ] Document admin dispute resolution procedures
- [ ] Set up monitoring for open disputes

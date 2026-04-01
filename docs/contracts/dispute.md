<<<<<<< feature/dispute-resolution-finality
# Dispute Resolution

## Overview

Complete dispute lifecycle management for invoice financing disputes. Enables
business owners and investors to raise disputes on invoices, with an
admin-controlled review and resolution process.

The central security property is **dispute locking**: once a dispute reaches
the `Resolved` state it is **terminal and immutable**. No further state
transitions are possible without an explicit policy-override path, preventing
accidental or malicious overwrites of finalized resolutions.

---

## Dispute Lifecycle

```
(none) ──create──▶ Disputed ──review──▶ UnderReview ──resolve──▶ Resolved
                                                                     │
                                                             TERMINAL / LOCKED
```

| Step | Transition | Actor | Notes |
|------|-----------|-------|-------|
| 1 | `None → Disputed` | Business or Investor | One dispute per invoice |
| 2 | `Disputed → UnderReview` | Admin only | Forward-only |
| 3 | `UnderReview → Resolved` | Admin only | **Terminal — locked** |

Any attempt to call `resolve_dispute` on an already-`Resolved` dispute returns
`DisputeNotUnderReview` because the status is no longer `UnderReview`. This is
the locking mechanism — no special flag is needed; the state machine itself
enforces immutability.

---

## Data Structures

### `DisputeStatus` (in `invoice.rs`)

```rust
pub enum DisputeStatus {
    None,        // No dispute exists
    Disputed,    // Dispute opened by business or investor
    UnderReview, // Admin is investigating
    Resolved,    // Admin has issued a final resolution (TERMINAL)
}
```

### `Dispute` (in `invoice.rs`)

| Field | Type | Description |
|-------|------|-------------|
| `created_by` | `Address` | Dispute initiator (business or investor) |
| `created_at` | `u64` | Creation timestamp (write-once) |
| `reason` | `String` | Dispute reason (1–1000 chars) |
| `evidence` | `String` | Supporting evidence (1–2000 chars) |
| `resolution` | `String` | Admin resolution text (empty until resolved) |
| `resolved_by` | `Address` | Admin who resolved (placeholder until resolved) |
| `resolved_at` | `u64` | Resolution timestamp (0 until resolved) |

---

## API Functions

### User Functions

#### `create_dispute(invoice_id, creator, reason, evidence) → Result<(), Error>`

Opens a dispute on an invoice.

**Preconditions:**
- `creator` must sign the transaction (`require_auth`)
- Invoice must exist
- No existing dispute on this invoice (`DisputeStatus::None`)
- `creator` must be the invoice's business owner or its investor
- `reason`: 1–1000 characters
- `evidence`: 1–2000 characters

**Errors:**

| Error | Condition |
|-------|-----------|
| `InvoiceNotFound` | Invoice does not exist |
| `DisputeAlreadyExists` | A dispute already exists on this invoice |
| `DisputeNotAuthorized` | Caller is neither business nor investor |
| `InvalidDisputeReason` | Reason is empty or exceeds 1000 chars |
| `InvalidDisputeEvidence` | Evidence is empty or exceeds 2000 chars |

---

### Admin Functions

#### `put_dispute_under_review(invoice_id, admin) → Result<(), Error>`

Advances a dispute from `Disputed` to `UnderReview`.

**Preconditions:**
- `admin` must sign the transaction (`require_auth`)
- `admin` must match the stored admin address
- A dispute must exist on the invoice
- Dispute must be in `Disputed` state

**Errors:**

| Error | Condition |
|-------|-----------|
| `NotAdmin` | Caller is not the stored admin |
| `InvoiceNotFound` | Invoice does not exist |
| `DisputeNotFound` | No dispute exists on this invoice |
| `InvalidStatus` | Dispute is not in `Disputed` state (includes `UnderReview` and `Resolved`) |

---

#### `resolve_dispute(invoice_id, admin, resolution) → Result<(), Error>`

Finalizes a dispute with a resolution text. **This is the locking operation.**

**Preconditions:**
- `admin` must sign the transaction (`require_auth`)
- `admin` must match the stored admin address
- A dispute must exist on the invoice
- Dispute must be in `UnderReview` state
- `resolution`: 1–2000 characters

**Locking invariant:** A second call on an already-`Resolved` dispute returns
`DisputeNotUnderReview` because the status is no longer `UnderReview`. The
`resolution`, `resolved_by`, and `resolved_at` fields are written atomically
and cannot be overwritten.

**Errors:**

| Error | Condition |
|-------|-----------|
| `NotAdmin` | Caller is not the stored admin |
| `InvoiceNotFound` | Invoice does not exist |
| `DisputeNotFound` | No dispute exists on this invoice |
| `DisputeNotUnderReview` | Dispute is not in `UnderReview` state (includes already-resolved disputes) |
| `InvalidDisputeReason` | Resolution is empty or exceeds 2000 chars |

---

### Query Functions

#### `get_dispute_details(invoice_id) → Result<Option<Dispute>, Error>`

Returns the full dispute record, or `None` if no dispute exists.

#### `get_invoice_dispute_status(invoice_id) → Result<DisputeStatus, Error>`

Returns the current `DisputeStatus` (including `None`).

#### `get_invoices_with_disputes() → Vec<BytesN<32>>`

Returns all invoice IDs that have any dispute (any status other than `None`).

#### `get_invoices_by_dispute_status(status) → Vec<BytesN<32>>`

Returns invoice IDs filtered by a specific `DisputeStatus`.
Passing `DisputeStatus::None` always returns an empty list.

---

## Security Model

### Dispute Locking

The `Resolved` state is **terminal**. The locking mechanism is the state
machine itself:

```
resolve_dispute checks: dispute_status == UnderReview
  → if Resolved: returns DisputeNotUnderReview  ← LOCK
  → if Disputed: returns DisputeNotUnderReview  ← LOCK
  → if None:     returns DisputeNotFound
  → if UnderReview: proceeds to write resolution
```

No additional flag or mutex is needed. The forward-only state machine
guarantees that once `Resolved` is written, no code path can overwrite it
without an explicit policy-override function (which does not currently exist).

### Authorization

| Operation | Required Role |
|-----------|--------------|
| `create_dispute` | Business owner or investor on the invoice |
| `put_dispute_under_review` | Platform admin |
| `resolve_dispute` | Platform admin |
| All queries | Anyone (read-only) |

Every mutating function calls `require_auth()` on the caller before any state
is read or written, preventing replay attacks.

### Input Validation

| Field | Min | Max | Error |
|-------|-----|-----|-------|
| `reason` | 1 char | 1000 chars | `InvalidDisputeReason` |
| `evidence` | 1 char | 2000 chars | `InvalidDisputeEvidence` |
| `resolution` | 1 char | 2000 chars | `InvalidDisputeReason` |

### One-Dispute-Per-Invoice

`create_dispute` checks `dispute_status == None` before writing. Any status
other than `None` returns `DisputeAlreadyExists`, preventing storage-bloat
attacks and ensuring a clean audit trail.

### Dual-Check Authorization

Admin operations perform two independent checks:
1. `admin.require_auth()` — cryptographic signature verification
2. `AdminStorage::require_admin(&env, &admin)` — role verification against
   the stored admin address

Both must pass. This prevents an attacker who knows the admin address from
calling admin functions without the private key.

---

## `dispute.rs` Module

The `dispute.rs` module provides shared types and helper logic:

```rust
// Validation helpers
pub fn validate_reason_len(len: u32) -> Result<(), QuickLendXError>
pub fn validate_evidence_len(len: u32) -> Result<(), QuickLendXError>
pub fn validate_resolution_len(len: u32) -> Result<(), QuickLendXError>

// State-machine helpers
pub fn require_disputed(status: &DisputeStatus) -> Result<(), QuickLendXError>
pub fn require_under_review(status: &DisputeStatus) -> Result<(), QuickLendXError>
pub fn is_locked(status: &DisputeStatus) -> bool
```

The `is_locked` predicate can be used by future policy-override logic to gate
any controlled exception path.

---

## Error Reference

| Error | Code | Condition |
|-------|------|-----------|
| `InvoiceNotFound` | 1000 | Invoice does not exist |
| `InvalidStatus` | 1401 | Invalid state transition (e.g. re-reviewing) |
| `NotAdmin` | 1103 | Caller is not the stored admin |
| `DisputeNotFound` | 1900 | No dispute exists on this invoice |
| `DisputeAlreadyExists` | 1901 | Duplicate dispute creation attempt |
| `DisputeNotAuthorized` | 1902 | Caller is not business or investor |
| `DisputeAlreadyResolved` | 1903 | (reserved for future use) |
| `DisputeNotUnderReview` | 1904 | Dispute is not in `UnderReview` state |
| `InvalidDisputeReason` | 1905 | Reason/resolution validation failed |
| `InvalidDisputeEvidence` | 1906 | Evidence validation failed |

---

## Usage Examples

```rust
// Business opens a dispute
create_dispute(
    env.clone(),
    &invoice_id,
    &business_address,
    String::from_str(&env, "Payment not received after due date"),
    String::from_str(&env, "Transaction ID: ABC123, Expected: 2025-01-15"),
)?;

// Admin puts under review
put_dispute_under_review(env.clone(), &invoice_id, &admin_address)?;

// Admin resolves (LOCKS the dispute)
resolve_dispute(
    env.clone(),
    &invoice_id,
    &admin_address,
    String::from_str(&env, "Verified payment delay. Instructed business to release funds."),
)?;

// Second resolve attempt — returns DisputeNotUnderReview (locked)
let err = resolve_dispute(env.clone(), &invoice_id, &admin_address, &new_text);
assert_eq!(err, Err(QuickLendXError::DisputeNotUnderReview));

// Query
let dispute = get_dispute_details(env.clone(), &invoice_id).unwrap();
assert_eq!(dispute.unwrap().resolved_by, admin_address);
```

---

## Test Coverage

`src/test_dispute.rs` contains 43 test cases (TC-01 through TC-43):

| Range | Area |
|-------|------|
| TC-01 – TC-10 | Dispute creation, authorization, boundary values |
| TC-11 – TC-14 | `put_dispute_under_review` state machine |
| TC-15 – TC-20 | `resolve_dispute` state machine and validation |
| TC-21 – TC-26 | Query functions |
| TC-27 – TC-29 | Multi-invoice isolation |
| TC-30 – TC-43 | **Regression tests — dispute locking** |

Key regression tests:
- **TC-30**: Resolved dispute cannot be overwritten (core locking test)
- **TC-31**: Resolved dispute cannot be re-opened via `put_dispute_under_review`
- **TC-32**: `resolved_at` is set exactly once and never zero after resolution
- **TC-33**: Cannot skip the `UnderReview` step
- **TC-34/35**: Non-admin cannot resolve or advance disputes
- **TC-38**: Double-resolution preserves original `resolved_by`/`resolved_at`
- **TC-39**: Invalid invoice ID returns `InvoiceNotFound` for all operations

---

## Deployment Checklist

- [ ] Initialize contract with admin address via `set_admin` / `initialize`
- [ ] Verify admin authorization works (test non-admin rejection)
- [ ] Confirm dispute creation restricted to business/investor only
- [ ] Test complete state machine: Disputed → UnderReview → Resolved
- [ ] Verify locking: second `resolve_dispute` returns `DisputeNotUnderReview`
- [ ] Validate field length constraints
- [ ] Verify one-dispute-per-invoice enforcement
- [ ] Test query functions return correct results for each status
- [ ] Verify multi-invoice isolation
- [ ] Run `cargo test test_dispute` — all 43 tests must pass

---

## Security Assumptions

1. The admin private key is kept secure. Compromise of the admin key allows
   dispute resolution but not dispute creation (which requires business/investor
   auth).
2. The Soroban `require_auth()` mechanism correctly enforces cryptographic
   signatures. This is a platform-level assumption.
3. The `AdminStorage::require_admin` check is the sole source of truth for
   admin identity. Admin key rotation via `transfer_admin` is atomic.
4. There is no policy-override path today. Any future override must be
   implemented as an explicit, separately audited function.
=======
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
>>>>>>> main

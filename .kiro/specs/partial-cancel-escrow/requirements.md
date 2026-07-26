# Requirements Document

## Introduction

This feature adds a `partial_cancel_escrow` function to the QuickLendX Soroban smart contract (crate `quicklendx-contracts`, Stellar blockchain, `#![no_std]` Rust). The function allows the business owner or a contract admin to refund a portion of an escrowed amount back to the investor while keeping the remainder locked in escrow under a `Funded` invoice. Unlike the existing `refund_escrow_funds` (full refund, terminates the lifecycle), a partial cancellation leaves the invoice and investment in their active states — only the escrow's `amount` field is reduced. No new error codes, no new escrow status variants, and no changes to existing functions are required.

## Glossary

- **Contract**: The `quicklendx-contracts` Soroban smart contract deployed on the Stellar blockchain.
- **Admin**: An address registered via `AdminStorage::is_admin` as a contract administrator.
- **Business**: The verified business address that created and owns the invoice.
- **Investor**: The address whose funds are currently held in escrow for a given invoice.
- **Invoice**: A financing request record stored on-chain; identified by a `BytesN<32>` invoice ID.
- **Escrow**: A payment record (`payments::Escrow`) that locks investor tokens inside the contract until release or refund. Fields relevant here: `amount: i128`, `status: EscrowStatus`, `investor: Address`, `currency: Address`.
- **EscrowStatus**: An enum with variants `Held`, `Released`, `Refunded`. The partial cancel function only operates on `Held` escrows.
- **InvoiceStatus**: An enum whose `Funded` variant indicates an active, fully-escrowed invoice.
- **InvestmentStatus**: An enum whose `Active` variant indicates an ongoing investment position.
- **cancel_amount**: The `i128` token amount the caller wishes to refund immediately from the escrow.
- **remaining_amount**: `escrow.amount - cancel_amount`; the token balance that stays locked in escrow after a partial cancellation.
- **Partial_Cancel_Escrow**: The new function (`partial_cancel_escrow`) introduced by this feature.
- **QuickLendXError**: The existing typed error enum. All 50 discriminant slots are occupied; no new variants may be added.
- **checked_sub**: Rust's `i128::checked_sub` method; returns `None` on arithmetic underflow instead of panicking.

---

## Requirements

### Requirement 1: Function Signature and Public Exposure

**User Story:** As a smart contract consumer, I want a single, clearly typed entry point for partial escrow cancellation, so that I can call it from off-chain clients and other contract functions without ambiguity.

#### Acceptance Criteria

1. THE Contract SHALL expose a public function with signature `partial_cancel_escrow(env: Env, invoice_id: BytesN<32>, caller: Address, cancel_amount: i128) -> Result<i128, QuickLendXError>`.
2. THE Contract SHALL return the new remaining escrowed amount (as `i128`) on success.
3. THE Contract SHALL compile without `std::` usage, maintaining `#![no_std]` compatibility.

---

### Requirement 2: Caller Authentication

**User Story:** As a security-conscious protocol designer, I want every state-mutating call to require on-chain authentication, so that no unauthorized party can drain escrow funds.

#### Acceptance Criteria

1. WHEN `partial_cancel_escrow` is invoked, THE Partial_Cancel_Escrow SHALL call `caller.require_auth()` as the first operation, before any parameter validation, state reads, or writes.

---

### Requirement 3: Caller Authorization

**User Story:** As the business owner of an invoice, I want to be able to partially cancel escrow on my own invoice, so that I can return a portion of investor funds when my financing needs change.

#### Acceptance Criteria

1. WHEN the caller is registered as a contract Admin via `AdminStorage::is_admin`, THE Partial_Cancel_Escrow SHALL permit the operation to proceed.
2. WHEN the caller's address equals `invoice.business`, THE Partial_Cancel_Escrow SHALL permit the operation to proceed.
3. WHEN the caller is neither an Admin nor the business owner of the invoice, THE Partial_Cancel_Escrow SHALL return `QuickLendXError::Unauthorized`.

---

### Requirement 4: Invoice State Preconditions

**User Story:** As a protocol designer, I want partial cancellation to be restricted to funded invoices, so that the escrow lifecycle remains consistent and deterministic.

#### Acceptance Criteria

1. WHEN the invoice identified by `invoice_id` does not exist in storage, THE Partial_Cancel_Escrow SHALL return `QuickLendXError::InvoiceNotFound`.
2. WHEN the invoice exists but its status is not `InvoiceStatus::Funded`, THE Partial_Cancel_Escrow SHALL return `QuickLendXError::InvalidStatus`.

---

### Requirement 5: Escrow State Preconditions

**User Story:** As a protocol designer, I want partial cancellation to be restricted to escrows that are currently held, so that already-released or already-refunded escrows cannot be modified.

#### Acceptance Criteria

1. WHEN no escrow record exists for the given `invoice_id`, THE Partial_Cancel_Escrow SHALL return `QuickLendXError::StorageKeyNotFound`.
2. WHEN an escrow record exists but its status is not `EscrowStatus::Held`, THE Partial_Cancel_Escrow SHALL return `QuickLendXError::InvalidStatus`.

---

### Requirement 6: Cancel Amount Validation

**User Story:** As a protocol designer, I want the cancel amount to be strictly between zero and the full escrow amount, so that zero-value no-ops and accidental full-refunds via this function are rejected.

#### Acceptance Criteria

1. WHEN `cancel_amount` is less than or equal to zero, THE Partial_Cancel_Escrow SHALL return `QuickLendXError::InvalidAmount`.
2. WHEN `cancel_amount` is greater than or equal to `escrow.amount`, THE Partial_Cancel_Escrow SHALL return `QuickLendXError::InvalidAmount` (the equal case — full refund — is covered here; the caller must use `refund_escrow_funds` instead).

---

### Requirement 7: Arithmetic Safety

**User Story:** As a protocol designer working under `overflow-checks = true` and `#![no_std]`, I want all arithmetic on token amounts to be checked, so that overflow or underflow can never silently corrupt balances.

#### Acceptance Criteria

1. THE Partial_Cancel_Escrow SHALL compute the remaining amount using `escrow.amount.checked_sub(cancel_amount)`.
2. IF `checked_sub` returns `None` (arithmetic underflow), THEN THE Partial_Cancel_Escrow SHALL return `QuickLendXError::InvalidAmount`.

---

### Requirement 8: Token Transfer

**User Story:** As an investor, I want the partial refund amount to be transferred to my address immediately, so that I have immediate access to the returned tokens without a secondary claim step.

#### Acceptance Criteria

1. WHEN all preconditions pass and arithmetic succeeds, THE Partial_Cancel_Escrow SHALL transfer exactly `cancel_amount` tokens from the contract address to `escrow.investor` using the escrow's `currency`.
2. IF the token transfer fails, THEN THE Partial_Cancel_Escrow SHALL propagate the underlying error. Because Soroban transactions are atomic, the entire transaction SHALL revert — including any state changes made prior to the failed transfer — so that on-chain state is never left in a partial-write condition.

---

### Requirement 9: Escrow State Update

**User Story:** As a protocol designer, I want the escrow record to reflect the reduced balance after a partial cancellation, so that subsequent operations (release, further partial cancels, or full refund) operate on the correct remaining amount.

#### Acceptance Criteria

1. WHEN the token transfer succeeds, THE Partial_Cancel_Escrow SHALL set `escrow.amount` to `remaining_amount` in the stored escrow record.
2. THE Partial_Cancel_Escrow SHALL leave `escrow.status` unchanged at `EscrowStatus::Held` after the update.
3. THE Partial_Cancel_Escrow SHALL NOT modify `EscrowStatus` enum — no new variants are introduced.

---

### Requirement 10: Invoice and Investment Status Preservation

**User Story:** As a protocol designer, I want the invoice and investment statuses to remain unchanged after a partial cancellation, so that the financing lifecycle is not prematurely terminated.

> **Design note:** Introducing a `PartiallyRefunded` invoice status was considered but rejected. The `QuickLendXError` enum has all 50 discriminant slots occupied and the `InvoiceStatus`, `EscrowStatus`, and `InvestmentStatus` enums are shared across many paths; adding new variants would be a cross-cutting breaking change. Partial cancellation is tracked via the reduced `escrow.amount` field and the `emit_partial_escrow_cancelled` event, which is sufficient for off-chain indexers.

#### Acceptance Criteria

1. THE Partial_Cancel_Escrow SHALL leave `invoice.status` as `InvoiceStatus::Funded` after a successful partial cancellation.
2. THE Partial_Cancel_Escrow SHALL leave the associated `Investment.status` as `InvestmentStatus::Active` after a successful partial cancellation.
3. THE Partial_Cancel_Escrow SHALL NOT call `invoice.mark_as_refunded` or transition the invoice to `InvoiceStatus::Refunded`.
4. THE Partial_Cancel_Escrow SHALL NOT introduce new variants into `InvoiceStatus`, `EscrowStatus`, or `InvestmentStatus` enums.

---

### Requirement 11: Event Emission

**User Story:** As an off-chain observer or indexer, I want a specific event emitted for each partial cancellation, so that I can track partial refunds separately from full refunds in analytics and audit trails.

#### Acceptance Criteria

1. WHEN a partial cancellation succeeds, THE Partial_Cancel_Escrow SHALL emit an event via `emit_partial_escrow_cancelled(env, invoice_id, investor, cancel_amount, remaining_amount)`.
2. THE event SHALL include the `invoice_id`, the `investor` address, the `cancel_amount` refunded, and the `remaining_amount` now held in escrow.

---

### Requirement 12: No Regression on Existing Functions

**User Story:** As a developer maintaining the QuickLendX protocol, I want the partial cancel feature to be additive only, so that existing behavior is not broken.

#### Acceptance Criteria

1. THE Contract SHALL leave `refund_escrow_funds` behavior unchanged after this feature is introduced.
2. THE Contract SHALL leave `cancel_invoice` behavior unchanged after this feature is introduced.
3. THE Contract SHALL NOT add new variants to `EscrowStatus`, `InvoiceStatus`, or `InvestmentStatus` enums.
4. THE Contract SHALL NOT add new variants to `QuickLendXError` (all 50 discriminant slots remain as-is).

---

### Requirement 13: Repeated Partial Cancellations

**User Story:** As a business owner, I want to be able to call `partial_cancel_escrow` multiple times on the same invoice, so that I can return investor funds incrementally as project milestones are adjusted.

#### Acceptance Criteria

1. WHEN `partial_cancel_escrow` is called a second time on the same invoice (after a prior successful partial cancel), THE Partial_Cancel_Escrow SHALL apply the same validation rules against the updated `escrow.amount`.
2. WHEN the cumulative sum of all `cancel_amount` values across multiple calls remains strictly less than the original escrowed amount, THE Partial_Cancel_Escrow SHALL succeed on each call and the invoice SHALL remain `Funded`.
3. WHEN a subsequent partial cancel would reduce `escrow.amount` to zero or below, THE Partial_Cancel_Escrow SHALL return `QuickLendXError::InvalidAmount` (the caller must use `refund_escrow_funds` for the final amount).

---

### Requirement 14: Test Coverage

**User Story:** As a developer maintaining the QuickLendX contract test suite, I want deterministic tests in `test_partial_cancel.rs` covering all happy paths and sad paths, so that regressions are caught by CI.

#### Acceptance Criteria

1. THE test module `test_partial_cancel.rs` SHALL contain a test `partial_cancel_reduces_remaining_balance_and_refunds_investor` that: places a bid, accepts it, calls `partial_cancel_escrow` with half the amount, and asserts that the escrow's stored amount equals half the original, the investor's token balance increased by the cancel amount, the invoice status is still `Funded`, the investment status is still `Active`, and the return value equals the remaining escrow amount.
2. THE test module SHALL contain a test `partial_cancel_multiple_times_accumulates_correctly` that calls `partial_cancel_escrow` twice in succession with amounts that sum to less than the original escrow amount, and asserts the invoice remains `Funded` after both calls.
3. THE test module SHALL contain a test `partial_cancel_fails_when_amount_exceeds_available_balance` that asserts `QuickLendXError::InvalidAmount` when `cancel_amount >= escrow.amount`.
4. THE test module SHALL contain a test `partial_cancel_fails_when_amount_is_zero` that asserts `QuickLendXError::InvalidAmount` when `cancel_amount == 0`.
5. THE test module SHALL contain a test `partial_cancel_fails_when_invoice_not_funded` that asserts `QuickLendXError::InvalidStatus` when the invoice is in `Pending` or `Verified` status.
6. THE test module SHALL contain a test `partial_cancel_fails_when_escrow_already_refunded` that asserts `QuickLendXError::InvalidStatus` when the escrow's status is `Refunded`.
7. THE test module SHALL contain a test `partial_cancel_fails_when_invoice_not_found` that asserts `QuickLendXError::InvoiceNotFound` for a non-existent invoice ID.
8. THE test module SHALL contain a test `partial_cancel_fails_when_caller_is_unauthorized` that asserts `QuickLendXError::Unauthorized` when the caller is neither admin nor business.
9. THE test module SHALL contain a test `partial_cancel_fails_when_amount_equals_full_escrow` that asserts `QuickLendXError::InvalidAmount` when `cancel_amount == escrow.amount`.
10. THE test module SHALL use only the Soroban mock `Env` (no external network calls) and SHALL be added to the `quicklendx-contracts` crate's test harness.
11. WHEN the full test suite is run via `cargo test -p quicklendx-contracts`, THE existing tests SHALL continue to pass without modification.

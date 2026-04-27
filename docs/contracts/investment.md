//! Investment lifecycle management and transition invariants.
//!
//! # Overview
//! This module manages the lifecycle of investments on the QuickLendX protocol.
//! Investments track funds committed by investors to invoice financing, including
//! their status transitions through settlement, default, refund, or withdrawal states.
//!
//! # Investment Lifecycle States
//!
//! An investment progresses through states following specific rules:
//!
//! ```text
//!     ┌──────────┐
//!     │  Active  │  ← New investments start in Active state
//!     └─────┬────┘
//!           │
//!      ┌────┴──────────┬──────────────┬──────────┐
//!      │               │              │          │
//!   Completed      Defaulted       Refunded    Withdrawn
//!   (Terminal)     (Terminal)      (Terminal)  (Terminal)
//! ```
//!
//! ## State Definitions
//!
//! ### Active
//! - **Meaning**: Investment funds are deployed and awaiting settlement or outcome
//! - **Entry**: When investor's bid is accepted and escrow is funded
//! - **Exit**: Via settlement, default, refund, or withdrawal
//! - **Properties**: Can appear in active-investment index; can transition
//!
//! ### Completed
//! - **Meaning**: Investment fully settled with all returns paid
//! - **Entry**: When invoice receives full payment (via `settle_invoice`)
//! - **Exit**: Terminal — cannot transition further
//! - **Properties**: Investor receives return + insurance claims (if applicable)
//!
//! ### Defaulted
//! - **Meaning**: Invoice failed to pay, investment marked for loss recovery
//! - **Entry**: When invoice remains unpaid past grace period (via `handle_overdue_invoices`)
//! - **Exit**: Terminal — cannot transition further
//! - **Properties**: Insurance providers are claimed if policies exist
//!
//! ### Refunded
//! - **Meaning**: Investment escrow returned due to invoice cancellation
//! - **Entry**: When invoice is cancelled before settlement (via `cancel_invoice`)
//! - **Exit**: Terminal — cannot transition further
//! - **Properties**: Funds returned to investor; insurance premiums returned
//!
//! ### Withdrawn
//! - **Meaning**: Investor voluntarily withdrew from the investment
//! - **Entry**: When investor chooses to withdraw before terminal outcome
//! - **Exit**: Terminal — cannot transition further
//! - **Properties**: Can occur from Active state only
//!
//! # Transition Rules (Invariants)
//!
//! ## Allowed Transitions
//!
//! | From | To | Trigger | Function |
//! |------|---|---------|----|
//! | Active | Completed | Invoice fully paid | `settle_invoice` |
//! | Active | Defaulted | Invoice_overdue + grace_period expired | `handle_overdue_invoices` |
//! | Active | Refunded | Invoice cancelled | `cancel_invoice` |
//! | Active | Withdrawn | Investor withdraws | `withdraw_investment` (if implemented) |
//!
//! ## Security Properties
//!
//! ### 1. Terminal Immutability
//! Once an investment reaches a terminal state (Completed, Defaulted, Refunded,
//! Withdrawn), it cannot transition to any other state:
//!
//! ```rust
//! // These transitions will always fail:
//! Completed → {Active, Defaulted, Refunded, Withdrawn}
//! Defaulted → {Active, Completed, Refunded, Withdrawn}
//! Refunded → {Active, Completed, Defaulted, Withdrawn}
//! Withdrawn → {Active, Completed, Defaulted, Refunded}
//! ```
//!
//! **Why**: Terminal states represent final settlement or loss. Allowing reversals
//! could enable:
//! - Double-payout exploits (settle twice)
//! - Escrow fund locks if state is reverted after payout
//! - Insurance claim duplication
//!
//! ### 2. No Orphan Active Investments
//! The active-investment index maintains a list of investment IDs in Active state.
//! Every investment transitioning to terminal state MUST be removed from this index:
//!
//! ```rust
//! // Enforcement: InvestmentStorage::update_investment
//! if old_status == Active && new_status != Active {
//!     remove_from_active_index(investment_id)
//! }
//! ```
//!
//! **Why**: Prevents off-chain monitoring from incorrectly tracking investments
//! as "still funding" when they've already settled.
//!
//! ### 3. Double-Payout Prevention (Idempotency)
//! State transitions are idempotent: repeating the same transition has no effect
//! beyond the first occurrence:
//!
//! - **Settlement**: `settle_invoice` fails if invoice status is already Paid
//! - **Default**: `handle_overdue_invoices` is safe to call multiple times
//! - **Refund**: `cancel_invoice` is safe to re-invoke
//!
//! **Why**: Protects against accidental double-settlement by Byzantine callers
//! or re-entrancy attacks. The contract never performs fund transfers twice.
//!
//! ### 4. Transition Guard Enforcement
//! Before any status change is persisted, `InvestmentStatus::validate_transition`
//! is called to verify the transition is allowed:
//!
//! ```rust
//! InvestmentStatus::validate_transition(&old_status, &new_status)
//!     .expect(\"invalid transition\");
//! ```
//!
//! **Why**: Centralizes transition logic so no code path can corrupt state directly.
//!
//! # Active-Investment Index
//!
//! ## Purpose
//! Maintains a list of investment IDs currently in Active state for:
//! - Efficient queries (`get_active_investment_ids`)
//! - Off-chain event monitoring
//! - Orphan detection (`validate_no_orphan_investments`)
//!
//! ## Storage
//! - **Key**: `symbol_short!(\"act_inv\")` (single global key)
//! - **Value**: `Vec<BytesN<32>>` of investment IDs
//! - **Scope**: Instance storage (fast, per-contract)
//!
//! ## Maintenance Rules
//!
//! ### Add to Active Index
//! When storing a new investment:
//! ```rust
//! if investment.status == Active {
//!     add_to_active_index(investment.investment_id)
//! }
//! ```
//!
//! ### Remove from Active Index
//! When transitioning from Active to terminal:
//! ```rust
//! if old_status == Active && new_status != Active {
//!     remove_from_active_index(investment.investment_id)
//! }
//! ```
//!
//! ### Deduplication
//! Operations are idempotent:
//! - Adding same ID twice = no duplicates (check before insert)
//! - Removing non-existent ID = no error (filtered remove)
//!
//! # Insurance Integration
//!
//! Investments can carry one or more active insurance policies. When transitioning
//! to certain terminal states, insurance claims are processed:
//!
//! ## Completion (Settlement)
//! - Insurance policies **remain active** (investor reclaimed them)
//! - No automatic claims
//!
//! ## Default
//! - Insurance policies **are claimed** via `process_all_insurance_claims`
//! - Providers receive coverage_amount; premium is not returned
//!
//! ## Refund
//! - Insurance **premiums are returned** to investor
//! - Policies are cancelled before refund
//!
//! # API Surface
//!
//! ## Core Operations
//!
//! ### `validate_transition(from, to) -> Result<(), Error>`
//! Validates whether a status transition is allowed. **Must be called before any state change.**
//!
//! **Returns**:
//! - `Ok(())` if transition is in the allowed set
//! - `Err(InvalidStatus)` if transition violates invariants
//!
//! ### `InvestmentStorage::store_investment(investment)`
//! Persists a new investment to storage.
//!
//! **Adds to**:
//! - Investment by ID (primary key)
//! - Invoice-to-investment index
//! - Investor-to-investment index
//! - Active-investment index (if status is Active)
//!
//! ### `InvestmentStorage::get_investment_by_invoice(invoice_id) -> Option<Investment>`
//! Retrieves investment for an invoice.
//!
//! **Safety**: Filters by invoice_id to prevent accidental invoice/investment confusion
//!
//! ### `InvestmentStorage::update_investment(investment)`
//! Atomically persists a status change and maintains the active index.
//!
//! **Validates**:
//! - Transition from old status to new status using `validate_transition`
//! - Removes from active index if leaving Active state
//!
//! **Panics** if transition is invalid (fail-fast for bug detection)
//!
//! ### `InvestmentStorage::get_active_investment_ids() -> Vec<BytesN<32>>`
//! Returns all investment IDs in Active state.
//!
//! **Use cases**:
//! - Off-chain monitoring
//! - Health checks
//! - Batch operations
//!
//! ### `InvestmentStorage::validate_no_orphan_investments() -> bool`
//! Verifies active index consistency.
//!
//! **Checks**:
//! - Every ID in active index is stored in contract
//! - Every stored investment has status == Active
//!
//! **Use cases**:
//! - Test assertions
//! - Off-chain validation
//! - Debug tooling
//!
//! # Example Transitions
//!
//! ## Scenario 1: Successful Settlement
//! ```text
//! Timeline:
//!   1. Investor places bid, bid accepted → Investment created (Active)
//!   2. Life span: t0 to t1
//!   3. Invoice paid at t1 → settle_invoice called
//!   4. Investment transitions: Active → Completed
//!   5. Removed from active-investment index
//!   6. Investor receives return
//! ```
//!
//! ## Scenario 2: Default Handling
//! ```text
//! Timeline:
//!   1. Investor places bid, bid accepted → Investment created (Active)
//!   2. Due date passes, grace period expires
//!   3. Maintenance job: handle_overdue_invoices
//!   4. Investment transitions: Active → Defaulted
//!   5. Removed from active-investment index
//!   6. Insurance claims processed
//! ```
//!
//! ## Scenario 3: Refund on Cancellation
//! ```text
//! Timeline:
//!   1. Investor places bid, bid accepted → Investment created (Active)
//!   2. Business decides to cancel invoice before payment → cancel_invoice
//!   3. Investment transitions: Active → Refunded
//!   4. Removed from active-investment index
//!   5. Escrow refunded to investor
//! ```
//!
//! # Testing
//!
//! Comprehensive test suites ensure:
//!
//! 1. **All allowed transitions succeed** (`test_transition_active_to_*`)
//! 2. **Terminal states are immutable** (`test_terminal_*_is_immutable`)
//! 3. **No orphan investments** (`test_no_orphan_after_*`)
//! 4. **Double-payout prevention** (`test_double_*_prevention`)
//! 5. **Concurrent investments** transition independently
//! 6. **Index integrity** under mutations
//!
//! Run with:
//! ```bash
//! cargo test --test investment_transitions --verbose
//! ```
//!
//! # Failure Modes & Recovery
//!
//! ## Invalid Transition Attempt
//! **Symptom**: `InvalidStatus` error when updating investment
//! **Cause**: Code path attempted illegal state change
//! **Recovery**:
//! - Check transition validation logic
//! - Ensure callers check invoice/investment status before operating
//! - Add pre-checks guard conditions
//!
//! ## Orphan Investment in Active Index
//! **Symptom**: `validate_no_orphan_investments` returns false
//! **Cause**: Bug in transition logic didn't remove from index
//! **Recovery**:
//! - Review update_investment implementation
//! - Check all terminal transitions remove from index
//! - Emergency cleanup via admin function (if available)
//!
//! ## Double-Settlement
//! **Symptom**: Investor received return twice
//! **Cause**: settle_invoice called on already-Paid invoice
//! **Recovery**:
//! - Settlement code should check InvoiceStatus::Paid first
//! - Ensure invoice transitions to Paid BEFORE investment transitions
//! - Add re-entrancy guards
//!
//! # Security Checklist
//!
//! - [ ] Validate every transition before persisting
//! - [ ] Remove from active index when leaving Active state
//! - [ ] Process insurance claims at appropriate transitions
//! - [ ] Prevent double-payout via status checks
//! - [ ] Test no-orphan invariant after each integration test
//! - [ ] Verify terminal states cannot revert in fuzz tests
//! - [ ] Monitor active index size in production
//!
//! # References
//!
//! - [Invoice Lifecycle](./invoice.md)
//! - [Settlement Flow](./settlement.md)
//! - [Default Handling](./default-handling.md)
//! - [Insurance Integration](./investment-insurance.md)
//! - [Storage Schema](./storage-schema.md)

// This module is self-contained; the implementation is in src/investment.rs

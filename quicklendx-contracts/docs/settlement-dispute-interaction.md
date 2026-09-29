# Settlement-Dispute Interaction & Logical Reorg Recovery

## Overview

While the Soroban/Stellar blockchain does not experience traditional reorgs (block reorganizations), the QuickLendX protocol faces **logical reorgs** — situations where an off-chain observer or indexer records a transaction (like a partial payment or settlement), but a subsequent administrative dispute operation conceptually rolls back or alters that outcome.

This document explains how the protocol handles these logical reorgs through strict settlement-dispute mutual exclusion, escrow safety guards, and deterministic resolution pathways.

## Definitions

### Logical Reorg
A **logical reorg** occurs when:
1. An off-chain system records a settlement or payment transaction as "final"
2. An on-chain dispute is opened or resolved that changes the expected outcome
3. The off-chain system must reconcile the new state with its previous understanding

Unlike blockchain reorgs (which rewrite history), logical reorgs happen **forward in time** but require state reconciliation.

### Settlement Finalization
The process of:
1. Recording full payment (`total_paid >= invoice.amount`)
2. Disbursing funds to investor (principal + profit)
3. Routing platform fees to treasury
4. Transitioning invoice status to `Paid`
5. Marking investment status as `Completed`
6. Releasing or consuming escrow records

Settlement finalization is **atomic and irreversible** once complete.

## Invariants

### INV-SD-1: Settlement Mutual Exclusion
**An invoice with `dispute_status != DisputeStatus::None` MUST NOT allow settlement finalization.**

**Rationale**: Disputes represent contested invoice states. Finalizing settlement while a dispute is active could:
- Release funds to a party later determined to be in breach
- Create irreversible state that contradicts dispute resolution
- Prevent proper refund pathways for the losing party

**Enforcement**: Settlement logic checks invoice status; disputes may transition invoice
status to a non-settleab

le state (e.g., `Disputed` status mapped to validation errors).

**Implementation**:
```rust
// settlement.rs: ensure_payable_status()
if invoice.status != InvoiceStatus::Funded {
    return Err(QuickLendXError::InvalidStatus);
}
```

### INV-SD-2: Partial Payment Recording During Disputes
**Partial payment recording remains ALLOWED during disputes to track business payment attempts.**

**Rationale**: Blocking partial payments during disputes would:
- Prevent businesses from demonstrating good-faith compliance
- Complicate dispute resolution (no payment history to reference)
- Create user-hostile workflows

However, **finalization is blocked** — partial payments accumulate but cannot trigger settlement completion.

**Implementation**:
```rust
// Partial payments update invoice.total_paid and payment history
// But settle_invoice_internal() checks status before finalizing
```

### INV-SD-3: Escrow Independence
**Escrow state transitions (`release_escrow`, `refund_escrow`) are independent of dispute status but respect invoice status guards.**

**Rationale**: Escrow follows its own state machine (`Held → Released/Refunded`). Disputes affect invoice status, which in turn gates escrow operations:
- **Release** requires `invoice.status == Paid` (settlement completed)
- **Refund** requires `invoice.status == Cancelled` or `Refunded`

Disputes influence these transitions indirectly by preventing status changes.

**Implementation**:
```rust
// payments.rs: release_escrow()
if invoice.status != InvoiceStatus::Paid {
    return Err(QuickLendXError::InvalidStatus);
}
```

### INV-ES-1: Escrow Lock During Disputes
**Escrow funds MUST NOT be released to business while `dispute_status == Disputed` or `UnderReview`.**

**Enforcement**: Escrow release requires invoice status `Paid`, which cannot be reached while dispute is active (per INV-SD-1).

### INV-ES-2: Escrow Single-Exit Property
**Only ONE of (`release_escrow`, `refund_escrow`) may succeed per escrow record.**

**Enforcement**: `EscrowStatus` state machine with terminal states:
```
Held → Released (terminal)
Held → Refunded (terminal)
```
Once terminal, all subsequent operations fail with `InvalidStatus`.

### INV-ES-3: Settlement Attempt Rejection
**Attempting to finalize settlement with active dispute returns `InvalidStatus`.**

**User Experience**: Clear error messaging that settlement is blocked pending dispute resolution.

## Dispute Resolution Outcomes

The protocol supports three distinct dispute resolution scenarios:

### 1. Resolution in Favor of Investor

**Scenario**: Investor proves business breach of contract (non-delivery, fraud, quality failure).

**State Transitions**:
1. `dispute_status: Disputed → UnderReview → Resolved`
2. Admin transitions invoice to `Cancelled` or `Refunded` status
3. Escrow refund becomes available: `refund_escrow()` succeeds
4. Settlement permanently blocked (invoice cannot return to `Funded` → `Paid`)

**Fund Routing**:
- Escrow funds (original investment amount) → Investor
- Any partial payments made by business → Handled per platform policy (might remain with business or return to investor)

**Guarantees**:
- Investor receives at least their principal back
- Business does not receive investor funds
- No double-spend possible (escrow single-exit)

### 2. Resolution in Favor of Business

**Scenario**: Admin determines dispute is frivolous or business fulfilled obligations.

**State Transitions**:
1. `dispute_status: Disputed → UnderReview → Resolved`
2. Invoice returns to `Funded` status (or equivalent settleable state)
3. Business completes any remaining payments
4. Settlement proceeds normally: `settle_invoice()` succeeds

**Fund Routing**:
- Escrow released to business (via settlement logic)
- Investor receives: `principal + profit - platform_fee`
- Platform receives: `platform_fee`

**Guarantees**:
- Business can complete settlement after resolution
- Investor receives agreed-upon returns if full payment occurs
- Standard settlement accounting applies

### 3. Neutral Resolution

**Scenario**: Both parties share responsibility; no clear winner.

**State Transitions**:
1. `dispute_status: Disputed → UnderReview → Resolved`
2. Invoice transitions per platform policy:
   - **Option A**: Return to `Funded`, settlement proceeds
   - **Option B**: Partial refund triggered (proportional to fault)
   - **Option C**: Mediation state, requires additional admin action

**Fund Routing** (Option A - Standard Terms):
- Settlement proceeds as if dispute never occurred
- Full payment → Normal distribution
- Partial payment → Refund difference or carry forward

**Guarantees**:
- No permanent fund freeze
- System provides deterministic resolution path
- Escrow safety maintained regardless of outcome

## Security Guarantees

### No Double-Spend

**Attack Vector**: Attacker attempts to:
1. Trigger escrow release during dispute
2. Simultaneously request escrow refund
3. Exploit race conditions in state machine

**Defense**:
- Escrow state machine enforces single-exit property
- Release requires `invoice.status == Paid`
- Refund requires `invoice.status == Cancelled/Refunded`
- These states are mutually exclusive

**Test Coverage**: `test_escrow_double_spend_protection_during_dispute`

### Refund Pathway Integrity

**Guarantee**: If dispute resolves against business, investor MUST have a path to recover funds.

**Mechanism**:
1. Admin resolution triggers invoice status change
2. Status change unlocks escrow refund
3. Refund operation returns funds to investor
4. Settlement permanently blocked (cannot override refund)

**Test Coverage**: `test_dispute_resolves_in_favor_of_investor`

### Settlement Unblock After Favorable Resolution

**Guarantee**: If dispute resolves in favor of business, settlement MUST become available.

**Mechanism**:
1. Admin resolution clears dispute status or returns invoice to settleable state
2. Business completes remaining payments
3. Standard settlement logic proceeds
4. Escrow released through normal settlement flow

**Test Coverage**: `test_dispute_resolves_in_favor_of_business`

## State Machine Diagram

```
Invoice Lifecycle with Dispute Interaction:

Pending → Verified → Funded → [Dispute Opens] → Disputed
                       ↓                              ↓
                       ↓                         UnderReview
                       ↓                              ↓
                       ↓                         [Resolution]
                       ↓                          ↙       ↘
                       ↓                    (Investor) (Business)
                       ↓                        ↓           ↓
                       ↓                   Cancelled   [Back to Funded]
                       ↓                        ↓           ↓
                       ↓                   Refunded      Paid
                       ↓___________________________________↑
                               [No Dispute Path]

Escrow State Machine:

Held → [Invoice reaches Paid] → Released (terminal)
  ↓
  ↓_→ [Invoice reaches Cancelled/Refunded] → Refunded (terminal)

Key:
- Solid arrows: Normal progression
- Dashed arrows: Conditional transitions
- (Investor)/(Business): Resolution outcomes
```

## Implementation Notes

### Dispute Status Checks in Settlement

The settlement logic **indirectly** enforces dispute blocking through invoice status checks:

```rust
// settlement.rs
fn ensure_payable_status(invoice: &Invoice) -> Result<(), QuickLendXError> {
    if invoice.status == InvoiceStatus::Paid
        || invoice.status == InvoiceStatus::Cancelled
        || invoice.status == InvoiceStatus::Defaulted
        || invoice.status == InvoiceStatus::Refunded
    {
        return Err(QuickLendXError::InvalidStatus);
    }

    if invoice.status != InvoiceStatus::Funded {
        return Err(QuickLendXError::InvalidStatus);
    }

    Ok(())
}
```

**Dispute Interaction**: When a dispute is opened/active, the invoice might:
1. Remain in `Funded` but have `dispute_status != None`
2. Transition to a dispute-specific status (implementation-dependent)

If approach (1), settlement attempts would need **explicit** dispute checks:
```rust
if invoice.dispute_status != DisputeStatus::None {
    return Err(QuickLendXError::DisputeActive);
}
```

### Partial Payment Persistence

Partial payments are recorded regardless of dispute status:
```rust
pub fn record_payment(
    env: &Env,
    invoice_id: &BytesN<32>,
    payer: &Address,
    amount: i128,
    payment_nonce: String,
) -> Result<Progress, QuickLendXError> {
    // No dispute status check here
    // Payments are tracked for transparency
    // But finalization (settle_invoice_internal) checks status
}
```

This design allows:
- Continuous payment tracking during disputes
- Dispute resolution to reference payment history
- Business to demonstrate good-faith compliance

## Testing Strategy

### Test Matrix

| Test Case | Dispute Status | Payment Status | Expected Outcome |
|-----------|----------------|----------------|------------------|
| Settlement during Disputed | Disputed | Partial (50%) | Settlement BLOCKED |
| Settlement during UnderReview | UnderReview | Partial (80%) | Settlement BLOCKED |
| Settlement after Resolved (Investor wins) | Resolved | Partial (60%) | Settlement BLOCKED, Refund AVAILABLE |
| Settlement after Resolved (Business wins) | Resolved | Full (100%) | Settlement SUCCEEDS |
| Escrow double-spend during dispute | UnderReview | Partial | Both release & refund FAIL |
| Partial payments during dispute | Disputed | Incremental | Payments RECORDED, finalization BLOCKED |

### Coverage Goals

**Target**: 95%+ coverage of settlement-dispute interaction logic

**Critical Paths**:
1. `settle_invoice_internal()` with dispute active
2. `record_payment()` during all dispute statuses
3. `release_escrow()` with dispute active
4. `refund_escrow()` after investor-favorable resolution
5. State transitions: `Disputed → UnderReview → Resolved → [Outcome]`

### Property-Based Testing

**Invariant**: `∀ invoice: invoice.dispute_status != None ⇒ settlement.finalize() = Err`

**Fuzz Targets**:
- Randomized payment sequences during dispute lifecycle
- Concurrent settlement + escrow operations during state transitions
- Dispute resolution outcomes with varying payment completeness

## Off-Chain Reconciliation

### Indexer Behavior

Off-chain indexers must handle logical reorgs:

**Scenario**: Indexer records settlement as complete, then dispute opens and resolves in favor of investor.

**Reconciliation Steps**:
1. Detect dispute event: `DisputeOpened(invoice_id, ...)`
2. Mark settlement as "pending" or "disputed" in local DB
3. Monitor dispute progression: `DisputeUnderReview`, `DisputeResolved`
4. On resolution:
   - If investor wins: Update settlement as "reversed", record refund
   - If business wins: Confirm settlement as "final"
   - If neutral: Follow platform policy

**API Guidance**:
```typescript
// Off-chain API should expose dispute status alongside settlement status
{
  "invoice_id": "0x...",
  "settlement_status": "completed",
  "dispute_status": "resolved",
  "final_outcome": "investor_refund" | "business_settled" | "neutral"
}
```

### Event Ordering

Indexers should respect this event sequence:
1. `InvoiceSettled` → Settlement recorded
2. `DisputeOpened` → Mark settlement as contested
3. `DisputeResolved` → Apply final outcome
4. `EscrowRefunded` OR `EscrowReleased` → Confirm fund routing

**Critical**: `DisputeResolved` event must include resolution outcome to guide reconciliation.

## Future Enhancements

### Automated Dispute Resolution

**Proposal**: Implement on-chain oracle or voting mechanism for dispute resolution.

**Benefits**:
- Reduces admin intervention
- Increases decentralization
- Faster dispute resolution

**Challenges**:
- Oracle reliability
- Voting security (Sybil resistance)
- Complexity vs. admin-mediated model

### Partial Refunds

**Proposal**: Support proportional refunds based on dispute outcome (e.g., 60% to investor, 40% to business).

**Implementation**:
```rust
pub struct DisputeResolution {
    pub investor_percentage: u32, // 0-100
    pub business_percentage: u32, // 0-100
    pub resolution_text: String,
}
```

**Challenges**:
- More complex fund routing
- Requires consensus on "fair" splits
- Testing complexity increases

### Time-Locked Disputes

**Proposal**: Auto-resolve disputes after timeout period if admin doesn't act.

**Benefits**:
- Prevents permanent fund freeze
- Provides guaranteed resolution timeline

**Risks**:
- Default resolution might be unfair
- Could be gamed by delaying admin action

## Conclusion

The QuickLendX settlement-dispute interaction model provides:
- **Strong safety guarantees**: No double-spend, no permanent fund freeze
- **Deterministic resolution**: Three clear outcomes with defined fund routing
- **Transparency**: Partial payments tracked throughout dispute lifecycle
- **Reversibility**: Refund pathways preserved for losing party
- **Test coverage**: Comprehensive integration tests validate all scenarios

By treating disputes as first-class state modifiers (not retroactive corrections), the protocol handles logical reorgs safely and predictably. Off-chain systems can reconcile state changes by monitoring dispute events and applying outcome-specific logic.

## References

- `src/settlement.rs`: Settlement finalization logic
- `src/dispute.rs`: Dispute lifecycle management
- `src/payments.rs`: Escrow state machine
- `src/test_settlement_dispute_interaction.rs`: Integration test suite
- `docs/escrow-invariants.md`: Escrow safety properties

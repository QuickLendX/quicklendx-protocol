# Contract Invariants

**Audience:** Smart-contract contributor / reviewer.

This document catalogs every invariant the QuickLendX Soroban contracts enforce,
where in the code the check lives, and what happens when it is violated.

---

## 1. Invoice Lifecycle State Machine

**Invariant:** An invoice follows the strict transition graph:

```
Pending → Verified → Funded → Paid
                          ↘ Defaulted
                          ↘ Cancelled
                          ↘ Refunded
```

Only transitions listed above are allowed. Terminal states (`Paid`, `Defaulted`,
`Cancelled`, `Refunded`) cannot transition further.

**Enforcement:**
- `Invoice::is_available_for_funding` — rejects any status other than `Verified`
  (`src/invoice.rs:119`)
- `ensure_payable_status` — rejects `Paid` / `Cancelled` / `Defaulted` /
  `Refunded` invoices; only `Funded` is payable (`src/settlement.rs:648`)
- `load_accept_bid_context` — rejects invoices that are already `Funded`
  (`src/escrow.rs:60`)
- `InvoiceStatus::is_terminal` — defines terminal set (`src/types.rs:28`)

**Violation:** `InvalidStatus` (1401) or `InvoiceAlreadyFunded` (1002).

---

## 2. One Escrow Per Invoice

**Invariant:** Each invoice may have **at most one** escrow record across its
entire lifetime.

**Two-layer guard:**
1. `load_accept_bid_context` checks `get_escrow_by_invoice` and
   `get_investment_by_invoice` before any mutation (`src/escrow.rs:76-80`).
2. `EscrowStorage::create_escrow` re-checks before token transfer
   (`src/payments.rs:...`).

**Violation:** `InvoiceAlreadyFunded` (1002) or `InvalidStatus` (1401).

**Test:** `test_escrow_uniqueness.rs`, `test_escrow_invariant_model.rs`

---

## 3. Settlement Accounting Identity

**Invariant:** For every invoice in status `Paid`:

```
investor_return + platform_fee == total_paid
```

**Enforcement:**
- `settle_invoice_internal` asserts the identity before disbursement
  (`src/settlement.rs:560-567`)
- `check_settlement_accounting_identity` re-verifies it during the invariant
  self-check (`src/invariants.rs:262-307`)

**Violation:** `InvalidAmount` (1200) — transaction is aborted before any funds
leave the contract.

---

## 4. Solvency: No Over-Funding

**Invariant:** For every `Funded` invoice:

```
0 < funded_amount <= amount
```

No active investment may carry non-positive principal.

**Enforcement:**
- `check_solvency` iterates active investments and funded invoices
  (`src/invariants.rs:105-134`)

**Violation:** The invariant self-check returns `passed == false`. The protocol
**must** be paused and investigated.

---

## 5. Global Solvency: Aggregate Borrowing ≤ Aggregate Lending

**Invariant:**

```
sum(active_investment_amounts) <= sum(all_invoice_amounts)
```

**Enforcement:**
- `check_sum_investments_le_sum_invoices` (`src/invariants.rs:186-227`)

**Violation:** The invariant self-check returns `passed == false`.

---

## 6. No Orphan Investments

**Invariant:** Every entry in the active-investment index carries
`InvestmentStatus::Active`. A terminal-status entry indicates a transition
path that failed to de-index the record.

**Enforcement:**
- `InvestmentStorage::validate_no_orphan_investments` called by
  `check_no_orphan_investments` (`src/invariants.rs:72-80`)

---

## 7. Storage Index Coherence

**Invariant:** Each invoice lives in **exactly one** status index whose status
equals the invoice's actual status. No duplicate indexing, no orphan index
entries.

**Enforcement:**
- `check_storage_index_coherence` (`src/invariants.rs:139-181`)
- Cross-module count checks (`src/test_cross_module_consistency.rs`)

---

## 8. Escrow Mapping Uniqueness & Bidirectional Integrity

**Invariant:** Every present escrow mapping is unique and correctly references
its invoice ID. If an escrow key exists, the escrow record's `invoice_id` must
equal the key's invoice ID.

**Enforcement:**
- `check_escrow_uniqueness` (`src/invariants.rs:233-256`)

---

## 9. Settlement Overpayment Guard

**Invariant:** `total_paid <= amount` is enforced at every payment recording
step. Overpayment attempts are silently capped to `remaining_due`.

**Enforcement:**
- `record_payment` caps `applied_amount` to `remaining_due`
  (`src/settlement.rs:275-279`)
- Explicit check: `new_total_paid > invoice.amount` returns `InvalidAmount`
  (`src/settlement.rs:291-293`)

---

## 10. Settlement Finalization Idempotency

**Invariant:** Once an invoice's status reaches `Paid`, further settlement
attempts are deterministically rejected.

**Enforcement:**
- `is_finalized` guard (`src/settlement.rs:519-521`)
- `ensure_payable_status` rejects `Paid` invoices (`src/settlement.rs:649`)

---

## 11. Payment Count Bound

**Invariant:** Payment records per invoice never exceed `MAX_PAYMENT_COUNT`
(1,000).

**Enforcement:**
- `record_payment` checks `payment_count >= MAX_PAYMENT_COUNT`
  (`src/settlement.rs:266-268`)

---

## 12. Settlement-Dispute Mutual Exclusion

**Invariant:** Settlement finalization is blocked while `dispute_status != None`.

**Enforcement:**
- The settlement path requires an explicit `dispute_status` check before
  finalization (`src/settlement.rs` — see doc comment lines 13-51).
  `ensure_payable_status` only checks `invoice.status`, so callers must also
  verify `dispute_status`.

**Violation:** If the explicit check is missing, the invoice could be settled
while disputed. This is a **critical safety property** — see
`docs/settlement-dispute-interaction.md` and
`test_settlement_dispute_interaction.rs`.

---

## 13. Dispute State Machine

**Invariant:** Disputes follow this strict state machine:

```
None → Disputed → UnderReview → Resolved
```

- `UnderReview` may appear at most once and only after `Disputed`.
- `Resolved` may appear at most once and only after `UnderReview`.
- `Resolved` is terminal.

**Enforcement:**
- `create_dispute`, `put_dispute_under_review`, `resolve_dispute` in
  `src/dispute.rs`
- Timeline ordering enforced in `src/dispute_timeline.rs`

**Violation:** `DisputeAlreadyExists` (1901), `DisputeNotUnderReview` (1904),
`DisputeAlreadyResolved` (1903).

---

## 14. Audit Trail Append-Only

**Invariant:** The audit trail never overwrites or reorders entries. Each
state-changing call produces exactly one entry. Monotonic IDs with embedded
timestamp + sequence + counter prevent tampering.

**Enforcement:**
- All `emit_*` and `AuditStorage::append_*` paths append, never mutate
  (`src/audit.rs`)
- `validate_invoice_audit_integrity` detects missing or tampered entries
  (`src/audit.rs`)

**Violation:** `AuditIntegrityError` (1701).

---

## 15. Bid Expiry Guarantee

**Invariant:** After `refresh_expired_bids` runs, no `Placed` bid on that
invoice has a deadline in the past, and every `Expired` bid has a deadline
strictly in the past.

**Enforcement:**
- `refresh_expired_bids` transitions over-due `Placed` → `Expired`
  (`src/bid.rs:559-634`)
- `assert_bid_invariants` validates the invariant for tests
  (`src/bid.rs:1076-1100`)

---

## 16. Terminal Bids Never Cleared

**Invariant:** Cleanup never removes `Accepted`, `Withdrawn`, or `Cancelled`
bids — only `Placed` (→ `Expired`) and already-`Expired` bids are pruned.

**Enforcement:**
- `refresh_expired_bids` explicitly checks terminal statuses
  (`src/bid.rs:579-581`)
- `refresh_investor_bids` same guard (`src/bid.rs:469-484`)

---

## 17. Bid TTL Bounds

**Invariant:** Bid TTL must be within `[MIN_BID_TTL_DAYS, MAX_BID_TTL_DAYS]`
= `[1, 30]` days. Zero is explicitly rejected.

**Enforcement:**
- `set_bid_ttl_days` (`src/bid.rs:307-323`)

**Violation:** `InvalidBidTtl` (1408).

---

## 18. Max Bids Per Invoice

**Invariant:** At most `MAX_BIDS_PER_INVOICE` (50) active (`Placed`) bids may
exist for a single invoice.

**Enforcement:**
- Checked during bid placement (`src/bid.rs`, contract entry point)

**Violation:** `MaxBidsPerInvoiceExceeded` (1406).

---

## 19. Invoice Amount ≥ Minimum

**Invariant:** Invoice `amount` must be ≥ `min_invoice_amount` and > 0.

**Enforcement:**
- `Invoice::new` checks `amount <= 0` → `InvalidAmount` (`src/invoice.rs:36`)
- Protocol limits enforced during creation (`src/contract.rs`)

---

## 20. KYC Gating for Invoice Creation

**Invariant:** Only businesses with `Verified` KYC status can create invoices.
`Pending`-KYC businesses are explicitly rejected.

**Enforcement:**
- `store_invoice` calls `require_business_not_pending` (`src/contract.rs:136`)

**Violation:** `BusinessNotVerified` (1600) or `KYCAlreadyPending` (1601).

---

## 21. Bid Ranking Determinism

**Invariant:** `compare_bids` produces a total ordering:
1. Profit (`expected_return - bid_amount`), higher = better
2. `expected_return`, higher = better
3. `bid_amount`, higher = better
4. `timestamp`, newer = better
5. `bid_id` as final tiebreaker

`get_best_bid` always returns the same bid as `rank_bids()[0]`.

**Enforcement:**
- `compare_bids` (`src/bid.rs:857-877`)
- `select_best_placed_bid` and `rank_bids` both use `compare_bids`
  (`src/bid.rs:885-907`, `944-975`)

---

## 22. Rating Constraints

**Invariant:**
- Rating score must be `[1, 5]`
- No duplicate rater per invoice
- Max `MAX_RATINGS_PER_INVOICE` (100) per invoice
- Ratings only on `Funded` or `Paid` invoices

**Enforcement:**
- `Invoice::add_rating` (`src/invoice.rs:300-331`)

**Violation:** `InvalidRating` (1500), `NotFunded` (1501), `AlreadyRated`
(1502), `OperationNotAllowed` (1402).

---

## 23. Tag Constraints

**Invariant:**
- Max `MAX_INVOICE_TAGS` (10) tags per invoice
- Tags are normalized (trimmed, case-folded) and deduplicated
- Each tag max `MAX_TAG_LENGTH` (50) bytes

**Enforcement:**
- `Invoice::new` and `add_tag` (`src/invoice.rs:40-56`, `270-278`)

**Violation:** `TagLimitExceeded` (1801), `InvalidTag` (1800).

---

## 24. Dispute Timeline Ordering

**Invariant:** Timeline entries appear in strictly chronological order:
`Opened` → (`UnderReview`?) → (`Resolved`?). `UnderReview` may appear at most
once and only after `Opened`. `Resolved` is terminal.

**Enforcement:**
- `src/dispute_timeline.rs` — all entry points reject invalid transitions

**Violation:** `DisputeNotUnderReview` (1904), `DisputeAlreadyResolved` (1903).

---

## 25. Payment Replay Protection

**Invariant:** Each `(invoice_id, nonce)` pair is unique. Duplicate nonces are
silently deduplicated (return current progress, no new record).

**Enforcement:**
- `record_payment` checks `seen` flag for non-empty nonces
  (`src/settlement.rs:254-261`)

---

## 26. Contract Initialization Is One-Time

**Invariant:** The contract can be initialized only once. Subsequent
`initialize` calls are rejected.

**Enforcement:**
- `ProtocolInitializer::initialize` checks for existing admin
  (`src/init.rs`)

---

## 27. Invariant Self-Check Is Read-Only

**Invariant:** `invariant_self_check` never mutates storage. It is admin-gated
and returns a diagnostic report with no side effects.

**Enforcement:**
- All checks use only `get_*` / `validate_*` methods
- Admin gate (`AdminStorage::require_admin_auth`) runs before any check
  (`src/invariants.rs:345-351`)

---

## 28. Escrow State Machine

**Invariant:** Escrow follows: `Held → Released` (on settlement) or
`Held → Refunded` (on refund/cancel). No other transitions allowed.

**Enforcement:**
- `EscrowStorage` guards (`src/payments.rs`)

---

## 29. Active Investment Index Discipline

**Invariant:** When an investment transitions to a terminal status
(`Completed`, `Defaulted`, `Refunded`, `Withdrawn`), it must be removed from
the active investment index.

**Enforcement:**
- `update_investment` and transitions in `investment.rs` de-index on terminal
  state

---

## 30. Investor Active-Bid Limit

**Invariant:** A single investor may have at most
`max_active_bids_per_investor` concurrently `Placed` bids across all invoices
(0 = disabled).

**Enforcement:**
- `investor_has_reached_bid_limit` (`src/bid.rs:394-404`)
- `count_active_placed_bids_for_investor` (`src/bid.rs:516-531`)

---

## Lifetime Integrity Summary

The invariant self-check (`invariant_self_check`) runs these checks in order:

| # | Check | Scope |
|---|-------|-------|
| 1 | `no_orphan_investments` | Active investment index |
| 2 | `audit_chain_integrity` | Per-invoice audit trails |
| 3 | `solvency` | Funding ≤ face, positive principals |
| 4 | `storage_index_coherence` | Status-index membership |
| 5 | `sum_investments_le_sum_invoices` | Aggregate solvency |
| 6 | `escrow_uniqueness` | Escrow map integrity |
| 7 | `settlement_accounting_identity` | Paid-invoice accounting |

See `src/invariants.rs:314-336` (`run_invariant_checks`).

## Related Documents

- `docs/invariant-checks.md` — Off-chain invariant monitoring (TypeScript)
- `docs/solvency-invariant.md` — P0 solvency invariant deep-dive
- `docs/dispute-timeline-invariants.md` — Dispute state machine specification
- `docs/invariants.md` — Backend invariant check suite

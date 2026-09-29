# Lifecycle Composition: Origination, Late, Default, and Dispute

> **Audience: contributors and reviewers.** Each of origination, lateness,
> default, and dispute already has its own detailed reference (linked below).
> This document does not repeat that detail — it answers a different
> question: **when two of these mechanisms are in play on the same invoice at
> the same time, which one wins?** That composition is not obvious from
> reading any single module, and getting it wrong silently is exactly the
> kind of thing a reviewer needs to be able to check against documented
> intent rather than re-derive from the diff.
>
> Every claim below is sourced from the actual contract entrypoints in
> [`quicklendx-contracts/src/lib.rs`](../quicklendx-contracts/src/lib.rs) and
> [`quicklendx-contracts/src/defaults.rs`](../quicklendx-contracts/src/defaults.rs),
> not from higher-level summaries, so you can jump straight to the guard in
> question and confirm it yourself.

---

## 1. The four phases, in one line each

| Phase | What it means | Full reference |
|-------|----------------|-----------------|
| **Origination** | An invoice moves from uploaded to funded: `Pending → Verified → Funded` | [`contracts/invoice-lifecycle.md`](contracts/invoice-lifecycle.md) |
| **Late** | The due date has passed but the grace window has not yet elapsed; a settlement surcharge accrues | [`FEES_GRACE_DEFAULT.md`](FEES_GRACE_DEFAULT.md) §3, §5 |
| **Default** | The grace window has strictly elapsed and the invoice is moved to the terminal `Defaulted` status | [`DEFAULT_FLOW_DIAGRAM.md`](DEFAULT_FLOW_DIAGRAM.md), [`default-finality-matrix.md`](default-finality-matrix.md) |
| **Dispute** | Either party contests the invoice; while open it blocks settlement | [`DISPUTE.md`](DISPUTE.md), [`dispute-timeline-invariants.md`](dispute-timeline-invariants.md) |

The rest of this document walks the entrypoints for each phase with real
signatures, then focuses on **how they interact** — the part that requires
reading three or four files together to piece together today.

---

## 2. Origination: the entrypoint sequence

```rust
// 1. Business uploads an invoice — status becomes Pending
contract.upload_invoice(
    &env, &business, amount, &currency, due_date,
    &description, &category, &tags, origination_fee_bps,
)?;

// 2. Admin verifies it — status becomes Verified, now open for bidding
contract.verify_invoice(&env, &invoice_id)?;   // admin.require_auth() internally

// 3. An investor places a bid
contract.place_bid(&env, &investor, &invoice_id, bid_amount, expected_return, &salt)?;

// 4. The business accepts a bid — status becomes Funded, escrow is Held
contract.accept_bid(&env, &invoice_id, &bid_id)?;
```

At this point the invoice is `Funded`, an `Investment` record exists with
`status = Active`, and an `Escrow` record exists with `status = Held`. This
is the state every scenario in the rest of this document starts from.

See [`contracts/invoice-lifecycle.md`](contracts/invoice-lifecycle.md) for
full validation rules, error codes, and the `update_invoice_status` admin
recovery path.

---

## 3. Late: the clock that starts at `due_date`

Two conditions are evaluated independently and only ever get **stricter** as
time passes — an invoice cannot become "less late":

```rust
// quicklendx-contracts/src/invoice.rs
pub fn is_overdue(&self, current_timestamp: u64) -> bool {
    current_timestamp > self.due_date
}
pub fn grace_deadline(&self, grace_period: u64) -> u64 {
    self.due_date.saturating_add(grace_period)
}
```

| Condition | Formula | Effect |
|-----------|---------|--------|
| **Overdue** | `now > due_date` | Overdue notification fires; if the invoice is later settled, a 20% surcharge (`LATE_FEE_SURCHARGE_BPS`) applies to the late-payment fee component |
| **Defaultable** | `now > due_date + grace_period` | The invoice becomes eligible for the `Defaulted` transition (§4) |

Being overdue does **not** move the invoice out of `Funded` — `settle_invoice`
still works normally for an overdue-but-not-yet-defaultable invoice, just
with the surcharge applied. The full fee arithmetic (early discount, late
surcharge, volume tiers) is in [`FEES_GRACE_DEFAULT.md`](FEES_GRACE_DEFAULT.md).

---

## 4. Default: two independent trigger paths

An invoice only becomes `Defaulted` once `now > grace_deadline`, but there
are two separate entrypoints that can perform the transition, with
**different authorization models**:

### 4.1 Explicit, admin-gated

```rust
// quicklendx-contracts/src/lib.rs
contract.mark_invoice_defaulted(&env, &invoice_id, grace_period_override)?;
```

Requires a configured admin and `admin.require_auth()`. Use this when you
know exactly which invoice needs to move and want it to happen in the same
transaction as an admin operation.

### 4.2 Permissionless batch scan

```rust
// quicklendx-contracts/src/lib.rs — no auth check
let overdue_count = contract.check_overdue_invoices_grace(&env, grace_period)?;
```

`check_overdue_invoices` / `check_overdue_invoices_grace` wrap
`defaults::scan_funded_invoice_expirations`, which walks a bounded window of
the `Funded` index (cursor persisted under `ovd_scan`, batch size clamped to
`[1, 100]`) and, for every invoice past its grace deadline, calls
`invoice.check_and_handle_expiration` → `defaults::handle_default` directly —
**no caller authorization is checked anywhere in this path**. This is the
mechanism a permissionless keeper/cron bot is expected to call repeatedly
(looping until the returned cursor wraps to `0`) to keep the funded set
current without relying on the admin.

Both paths converge on the same `handle_default` function, so the guards and
side effects are identical regardless of which one fired:

| Guard | Rejects with |
|-------|--------------|
| Already `Defaulted` | `InvoiceAlreadyDefaulted` |
| Not currently `Funded` | `InvalidStatus` / `InvoiceNotAvailableForFunding` |
| Settlement already finalized | `InvalidStatus` |
| Escrow not `Held` | `InvalidStatus` |
| `now <= grace_deadline` | `OperationNotAllowed` (strictly greater-than — calling exactly at the deadline fails) |
| Transition guard already set | `DuplicateDefaultTransition` |

On success: invoice → `Defaulted`, investment → `Defaulted`, any active
insurance is claimed, `invoice_expired` / `invoice_defaulted` events fire,
and business/investor are notified. See
[`DEFAULT_FLOW_DIAGRAM.md`](DEFAULT_FLOW_DIAGRAM.md) §4 for the full sequence
and [`DEFAULT_ACCOUNTING.md`](DEFAULT_ACCOUNTING.md) for downstream risk-score
and analytics effects.

**Notably absent from this guard list: dispute status.** That is the subject
of §6.

---

## 5. Dispute: open, review, resolve

```rust
// Business or investor opens a dispute on the invoice
contract.create_dispute(&env, &invoice_id, &creator, &reason, &evidence)?;
// dispute_status: None → Disputed

// Admin moves it into review
contract.put_dispute_under_review(&env, &invoice_id, &admin)?;
// dispute_status: Disputed → UnderReview

// Admin records a resolution
contract.resolve_dispute_structured(&env, &invoice_id, &admin, outcome, &note)?;
// dispute_status: UnderReview → Resolved
```

As implemented, `create_dispute` requires: the contract is not paused, the
invoice is not frozen, `dispute_status == None` (one open dispute per
invoice at a time), and a non-empty `reason`. `creator.require_auth()` only
proves the caller controls that address — it does not itself verify `creator`
is the invoice's business or its investor.

> **Cross-check note:** [`DISPUTE.md`](DISPUTE.md) additionally documents an
> eligibility rule restricting dispute creation to invoices in `Pending`,
> `Verified`, `Funded`, or `Paid` status (rejecting `Cancelled` /
> `Defaulted`). That check exists as `verification::validate_dispute_eligibility`,
> but it is only invoked from `dispute::create_dispute` — a function that is
> not wired into the `#[contractimpl]` block that ships as the contract's
> `create_dispute` entrypoint (that entrypoint is defined directly in
> `lib.rs` and does not call it). If this matters for your change, verify
> against your checked-out revision of `lib.rs` before relying on it, and
> consider filing a follow-up issue if the gap is still present.

`resolve_dispute_structured` only writes `resolution_outcome` onto the
invoice's `Dispute` record — it does **not** itself move `invoice.status`.
Realizing a `FavorInvestor` outcome (i.e., actually returning funds) still
requires a separate admin call to `refund_escrow_funds`, which itself
requires `invoice.status == Funded`. A `FavorBusiness` outcome requires no
further action — `dispute_status == Resolved` is sufficient to unblock
settlement (§6).

---

## 6. How they compose

This is the part that isn't visible from any single module.

### 6.1 Dispute vs. settlement — blocked

`settle_invoice` explicitly checks `dispute_status` before finalizing
(`quicklendx-contracts/src/settlement.rs`):

```rust
if invoice.dispute_status == DisputeStatus::Disputed
    || invoice.dispute_status == DisputeStatus::UnderReview
{
    return Err(QuickLendXError::DisputeActive); // error 1907
}
```

`Resolved` is deliberately excluded from this check — once the admin has
ruled, settlement resumes normally. This is a real, defence-in-depth guard;
see `test_settle_during_dispute.rs` for the regression tests that pinned it
down (it closed a gap where a business could open a dispute and then settle
before the admin ruled, permanently foreclosing the investor's refund path).

### 6.2 Dispute vs. default — **not blocked**

`defaults::handle_default` has no `dispute_status` check anywhere in its
guard list (§4). Practically:

- Opening a dispute on a `Funded` invoice does **not** pause the grace clock.
- If the grace deadline passes while a dispute is `Disputed` or
  `UnderReview`, `mark_invoice_defaulted` (or the permissionless batch scan)
  will still succeed and move the invoice to `Defaulted`.
- The dispute record (reason, evidence, timestamps) is untouched by the
  default transition and remains queryable via `get_dispute_details`.
- `resolve_dispute` / `resolve_dispute_structured` only require
  `dispute_status == UnderReview` — they do **not** check `invoice.status` —
  so an admin can still record a resolution after the invoice has already
  defaulted. At that point a `FavorInvestor` outcome is a paper record only:
  `refund_escrow_funds` will reject it (`InvalidStatus`, since the invoice is
  no longer `Funded`), so any investor recovery has to come from the
  insurance claims that `handle_default` already processed, not from escrow.

In short: **a dispute is a hold on settlement, not a hold on default.** If
you need a dispute to actually stop the clock, that has to be an operational
practice (admin resolves before the grace deadline, or extends the grace
period via the `grace_period` override on `mark_invoice_defaulted`) rather
than something the contract enforces automatically today.

### 6.3 Decision table

| Scenario (at the moment `now > grace_deadline`) | Result |
|---|---|
| No dispute ever opened | Default proceeds normally |
| Dispute opened and already `Resolved` (any outcome) before the deadline | Default proceeds normally; `Resolved` does not gate `handle_default` |
| Dispute still `Disputed` or `UnderReview` at the deadline | Default **still proceeds** — dispute record is preserved but the invoice becomes `Defaulted` |
| Dispute opened *after* the invoice is already `Defaulted` | Rejected by `create_dispute`'s eligibility check per `DISPUTE.md` — but confirm this against §5's cross-check note for your revision |
| Admin resolves a dispute (`FavorInvestor`) after the invoice defaulted | `resolution_outcome` is recorded; `refund_escrow_funds` will fail (`InvalidStatus`) since escrow disposition already happened via insurance claims at default time |

### 6.4 Origination vs. everything else

Origination only matters here in that it determines whether the other three
phases are even reachable: `create_dispute`, the late-fee surcharge, and
`mark_invoice_defaulted` / the batch scan all operate on invoices that have
already reached `Funded` (default and the late surcharge require it
explicitly; dispute creation is broader per §5). An invoice cancelled or
refunded before funding never enters the late/default/dispute state space at
all.

---

## 7. Worked composed timeline

```
t = 0            upload_invoice(business, amount=5_000_000, due_date=t+30d)
                 → Pending

t = 1_000        verify_invoice(admin, invoice_id)
                 → Verified

t = 2_000        place_bid(investor, invoice_id, ...) ; accept_bid(invoice_id, bid_id)
                 → Funded, escrow: Held

t = 2_600_000    due_date (2_592_000) has passed
                 → overdue; late surcharge would apply if settled now

t = 2_650_000    create_dispute(investor, invoice_id, "goods not delivered", evidence)
                 → dispute_status: Disputed
                 → settle_invoice would now fail with DisputeActive (1907)

t = 2_700_000    put_dispute_under_review(admin, invoice_id)
                 → dispute_status: UnderReview
                 (grace deadline = due_date + 604_800 = 3_196_800 — still running)

t = 3_196_801    grace deadline has strictly passed; dispute is still UnderReview
                 mark_invoice_defaulted(admin, invoice_id, None) — or the
                 permissionless check_overdue_invoices_grace(env, 0) batch scan
                 → SUCCEEDS: status → Defaulted (dispute_status untouched)
                 → investment → Defaulted, insurance claims processed
                 → events: invoice_expired, invoice_defaulted

t = 3_200_000    resolve_dispute_structured(admin, invoice_id, FavorInvestor, note)
                 → dispute_status: Resolved, resolution_outcome: FavorInvestor
                 → refund_escrow_funds(invoice_id, admin) now FAILS (InvalidStatus)
                   — the invoice is Defaulted, not Funded; the investor's
                   recovery already ran through insurance claims at t = 3_196_801,
                   not through this resolution.
```

---

## 8. Related documents

| Document | Relevance |
|----------|-----------|
| [`contracts/invoice-lifecycle.md`](contracts/invoice-lifecycle.md) | Full invoice state machine and investment status integration |
| [`FEES_GRACE_DEFAULT.md`](FEES_GRACE_DEFAULT.md) | Fee model, grace period resolution, default trigger preconditions |
| [`DEFAULT_FLOW_DIAGRAM.md`](DEFAULT_FLOW_DIAGRAM.md) | State-machine diagram for the default transition itself |
| [`DEFAULT_ACCOUNTING.md`](DEFAULT_ACCOUNTING.md) | How a default rolls into investor risk scores and business reports |
| [`default-finality-matrix.md`](default-finality-matrix.md) | Exhaustive decision table for default eligibility |
| [`DISPUTE.md`](DISPUTE.md) | Full dispute state machine and entrypoint reference |
| [`dispute-timeline-invariants.md`](dispute-timeline-invariants.md) | Timestamp invariants across dispute state changes |
| [`ESCROW.md`](ESCROW.md) | Escrow release/refund preconditions |
| [`ERROR_CODES.md`](ERROR_CODES.md) | Complete error code reference, including `DisputeActive` (1907) |
| [`quicklendx-contracts/src/defaults.rs`](../quicklendx-contracts/src/defaults.rs) | `mark_invoice_defaulted`, `handle_default`, `scan_funded_invoice_expirations` |
| [`quicklendx-contracts/src/lib.rs`](../quicklendx-contracts/src/lib.rs) | Contract entrypoints referenced throughout this document |
| [`quicklendx-contracts/src/test_settle_during_dispute.rs`](../quicklendx-contracts/src/test_settle_during_dispute.rs) | Regression tests for the dispute-blocks-settlement guard |

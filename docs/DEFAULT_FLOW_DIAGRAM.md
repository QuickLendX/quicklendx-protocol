# Default Flow Diagram: Invoice Past-Due → Default → Recovery

> **Audience: contributors** who need to trace exactly what happens when a
> funded invoice misses its repayment deadline — from the moment the due date
> passes, through the grace window, into `Defaulted`, and through the
> downstream accounting effects. Read this alongside
> [`docs/FEES_GRACE_DEFAULT.md`](FEES_GRACE_DEFAULT.md) (fee/grace reference)
> and [`docs/INVOICE_LIFECYCLE.md`](INVOICE_LIFECYCLE.md) (full lifecycle).

---

## 1. High-level state machine

```
                           accept_bid(business, invoice_id, bid_id)
   Verified ──────────────────────────────────────────────────────► Funded
                                                                       │
                               ┌───────────────────────────────────────┤
                               │   now ≤ due_date                      │
                               │   ─ normal repayment window ─         │
                               │                                       │
                  settle_invoice(business|admin, ...)                  │
                  total_paid ≥ invoice.amount                          │
                               │                                       │
                               ▼                                       │
                             Paid ◄── terminal                         │
                                                                       │
                               ┌───────────────────────────────────────┤
                               │   now > due_date                      │
                               │   ─ OVERDUE (not yet defaultable) ─   │
                               │   grace window running                │
                               │   late-payment surcharge applies      │
                               │   if invoice is ultimately settled    │
                               │                                       │
                               │   now > due_date + grace_period       │
                               │   ─ DEFAULTABLE ─                     │
                               │                                       │
            trigger_default(anyone, invoice_id)                        │
            OR scan_funded_invoice_expirations(grace, limit)           │
                               │                                       │
                               ▼                                       │
                          Defaulted ◄── terminal                       │
                                                                       │
                               ┌───────────────────────────────────────┤
                               │   at any point while Funded           │
                               │   (blocks settlement)                 │
                               │                                       │
                      create_dispute(business|investor, ...)           │
                               │                                       │
                               ▼                                       │
                        DisputeStatus: Disputed                        │
                        (invoice stays Funded)                         │
                               │                                       │
                  put_dispute_under_review(admin)                      │
                               │                                       │
                               ▼                                       │
                        DisputeStatus: UnderReview                     │
                               │                                       │
           ┌───────────────────┼───────────────────┐                   │
           │                   │                   │                   │
   FavorBusiness         FavorInvestor          Split/Dismissed        │
           │                   │                   │                   │
           ▼                   ▼                   ▼                   │
    Funded (resume      Cancelled or        Platform policy            │
    settlement)         Refunded ◄──────────determines path           │
                        (terminal)
```

Terminal states (`Paid`, `Defaulted`, `Cancelled`, `Refunded`) are
**irreversible** — no entrypoint may move an invoice out of a terminal state.

---

## 2. Overdue vs. defaultable

These two conditions are distinct and sequential:

| Condition | Formula | Effect |
|-----------|---------|--------|
| **Overdue** | `now > invoice.due_date` | Overdue notification emitted; late-payment surcharge accrues on final settlement fee |
| **Defaultable** | `now > invoice.due_date + grace_period` | `trigger_default` / batch scanner may execute the transition |

An invoice that is overdue but still within the grace window cannot be
defaulted. `trigger_default` called at `now == grace_deadline` is also
rejected — the check is strictly greater-than.

### Grace period resolution order

```
1. Explicit override supplied to trigger_default / scan_funded_invoice_expirations
   (must be ≤ MAX_GRACE_PERIOD = 30 days, else InvalidTimestamp)
2. Protocol config  ProtocolConfig.grace_period_seconds  (set by admin at init)
3. DEFAULT_GRACE_PERIOD = 7 × 24 × 3600 = 604 800 s  (7 days)
```

Source: `resolve_grace_period` in
[`quicklendx-contracts/src/defaults.rs`](../quicklendx-contracts/src/defaults.rs).

---

## 3. Pre-flight guards for `trigger_default`

All five checks must pass before the transition proceeds. They are evaluated
in this order inside `handle_default` (`defaults.rs`):

| # | Guard | Error if fails |
|---|-------|----------------|
| 1 | Invoice status is `Funded` | `InvoiceAlreadyDefaulted` (1006) if already `Defaulted`; `InvoiceNotAvailableForFunding` (1001) for any other status |
| 2 | Settlement not already finalized (`is_invoice_finalized == false`) | `InvalidStatus` |
| 3 | Escrow status is `Held` (not `Released` or `Refunded`) | `InvalidStatus` |
| 4 | `ledger.timestamp() > due_date + grace_period` (strictly greater-than) | `OperationNotAllowed` |
| 5 | Transition guard not already set (`check_and_set_default_guard`) | `DuplicateDefaultTransition` |

The transition guard (check 5) is written **only after** all earlier checks
pass — a failed attempt (e.g. invoice still within grace) does not poison the
guard or block a later legitimate call.

---

## 4. What happens during a default transition

When all guards pass, `handle_default` executes atomically:

```
mark_invoice_defaulted
  │
  ├─ InvoiceStorage: Funded index → Defaulted index
  ├─ invoice.status            = Defaulted
  ├─ InvestmentStatus          = Defaulted
  ├─ investment.process_all_insurance_claims(env)
  │    └─ for each provider with coverage > 0:
  │         emit InsuranceClaimed { investment_id, invoice_id, provider, coverage_amount }
  │
  ├─ emit InvoiceExpired  { invoice_id, business, due_date }
  ├─ emit InvoiceDefaulted { invoice_id, business, investor, timestamp }
  │
  ├─ audit_log: AuditOperation::InvoiceDefaulted
  │    (append-only, SHA-256 hash-chained per invoice trail)
  │
  └─ notify: NotificationType::InvoiceDefaulted → business + investor
```

**Escrow note**: `trigger_default` does not itself call `release_escrow` or
`refund_escrow`. Escrow disposition follows from the investment status
transition and insurance claims processed above; the escrow record is left
`Held` after the status flip. Downstream settlement recovery (partial
insurance payouts, write-offs) is handled by operator tooling described in
[`docs/DEFAULT_ACCOUNTING.md`](DEFAULT_ACCOUNTING.md).

---

## 5. Two ways to trigger a default

### 5.1 Single-invoice (explicit)

```rust
// Permissionless — any account may call after the grace deadline
contract.trigger_default(
    &env,
    &caller,        // Address — no auth requirement
    &invoice_id,    // BytesN<32>
)
```

Use this when you know exactly which invoice has expired.

### 5.2 Batch scanner

```rust
// Permissionless batch scan with a rotating cursor
let result: OverdueScanResult = contract.scan_funded_invoice_expirations(
    &env,
    grace_period,   // u64 — seconds; 0 uses protocol default
    Some(50),       // Option<u32> — batch size, clamped to [1, 100]; None = 25
);

// result fields:
// overdue_count  — invoices past due in this batch
// scanned_count  — invoices examined in this batch
// total_funded   — snapshot size of the funded index
// next_cursor    — 0 means the full cycle is complete; keep calling until 0
```

To scan every funded invoice, loop until `next_cursor == 0`:

```rust
loop {
    let r = contract.scan_funded_invoice_expirations(&env, 0, Some(50));
    // process r.overdue_count defaulted invoices …
    if r.next_cursor == 0 {
        break;
    }
}
```

The cursor is stored in instance storage under `ovd_scan` and advances by
`limit` positions on each call, wrapping to 0 when the full funded set has
been scanned.

---

## 6. Dispute interception before default

A dispute may be opened by the business owner or the investor on any `Funded`
invoice **before** the default transition fires. Once open, the dispute
**blocks settlement** but does not itself prevent the grace clock from
ticking.

```
Funded + dispute_status = Disputed/UnderReview
  │
  │  Grace deadline passes while dispute is open
  │  (trigger_default will succeed unless dispute is resolved first —
  │   the default guards do not check dispute_status)
  │
  ├─ Admin resolves → FavorBusiness
  │    invoice.dispute_status = Resolved
  │    invoice.status remains Funded → settlement can proceed normally
  │
  ├─ Admin resolves → FavorInvestor
  │    Admin transitions invoice → Cancelled or Refunded
  │    refund_escrow available; settlement permanently blocked
  │
  └─ No resolution before grace deadline expires
       trigger_default succeeds → Defaulted (terminal)
       (dispute record preserved; dispute_status is not checked by default guards)
```

For the full dispute state machine, see [`docs/DISPUTE.md`](DISPUTE.md).

---

## 7. Recovery path after default

`Defaulted` is a **terminal state**. There is no on-chain entrypoint to
reverse it. "Recovery" in this context refers to the downstream accounting
and off-chain steps:

| Recovery mechanism | Who | What happens |
|--------------------|-----|--------------|
| Insurance payout | Automatic (in `handle_default`) | Each opted-in provider's coverage amount is claimed and emitted via `InsuranceClaimed` event |
| Investor analytics update | Admin / cron | `update_investor_analytics(investor, amount, is_success=false)` — increments `defaulted_investments`, recalculates risk score, re-derives tier and investment limit |
| Business report | Query | `get_business_report` returns `default_rate` in basis points based on defaulted/total invoice ratio |
| Audit trail review | Operator | `AuditStorage::verify_audit_chain(env, &invoice_id)` checks hash-chain integrity; `first_audit_chain_divergence` pinpoints tampering |
| Off-chain notification | System | `NotificationType::InvoiceDefaulted` delivered to business and investor for email/dashboard alert |

For the full accounting chain (risk scores, tier demotion, business default
rate), see [`docs/DEFAULT_ACCOUNTING.md`](DEFAULT_ACCOUNTING.md).

---

## 8. Concrete timeline example

```
t = 0            store_invoice(business, amount=5_000_000, due_date=t+30d)
                 → status: Pending

t = 1 000        verify_invoice(admin, invoice_id)
                 → status: Verified

t = 2 000        accept_bid(business, invoice_id, bid_id)
                 → status: Funded, escrow: Held, bid: Accepted

                 ── repayment window open ──

t = 2_592_000    due_date reached
                 → status: Funded (overdue; late-payment surcharge accrues)
                 → grace window running (DEFAULT_GRACE_PERIOD = 604_800 s)

t = 2_592_000    trigger_default called at exactly the grace deadline
+ 604_800        → REJECTED (OperationNotAllowed; must be strictly greater-than)

t = 2_592_001    trigger_default called one second past the grace deadline
+ 604_800        → ALL GUARDS PASS
                 → status: Defaulted (terminal)
                 → investment.status: Defaulted
                 → events: InvoiceExpired, InvoiceDefaulted
                 → audit entry appended (hash-chained)
                 → notifications sent to business + investor

                 ── recovery phase (off-chain) ──

                 update_investor_analytics(investor, 5_000_000, false)
                 → defaulted_investments++, risk_score recalculated, tier re-derived
```

---

## 9. Related documents

| Document | Relevance |
|----------|-----------|
| [`docs/INVOICE_LIFECYCLE.md`](INVOICE_LIFECYCLE.md) | Full invoice state machine and all transition entrypoints |
| [`docs/FEES_GRACE_DEFAULT.md`](FEES_GRACE_DEFAULT.md) | Fee model, grace period resolution, default trigger rules, and finality guards — detailed reference |
| [`docs/DEFAULT_ACCOUNTING.md`](DEFAULT_ACCOUNTING.md) | How defaults roll into investor risk scores, business reports, and the audit trail |
| [`docs/default-finality-matrix.md`](default-finality-matrix.md) | Decision table: when default is allowed (status × finality × escrow) |
| [`docs/DISPUTE.md`](DISPUTE.md) | Dispute open/review/resolve flow and fund implications |
| [`docs/ESCROW.md`](ESCROW.md) | Escrow lifecycle and release conditions |
| [`docs/ERROR_CODES.md`](ERROR_CODES.md) | Complete error code reference |
| [`quicklendx-contracts/src/defaults.rs`](../quicklendx-contracts/src/defaults.rs) | Default transition implementation: `mark_invoice_defaulted`, `handle_default`, `resolve_grace_period` |
| [`quicklendx-contracts/src/types.rs`](../quicklendx-contracts/src/types.rs) | `InvoiceStatus`, `InvestmentStatus`, `DisputeStatus` enums |

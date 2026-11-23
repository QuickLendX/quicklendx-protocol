# Default Accounting: How Defaults Roll into Ratings and the Audit Trail

**Audience: contributors** — this document is for people reading the contract source and wanting to verify how defaulted invoices propagate into investor risk scores, business performance reports, and the append-only audit trail. Operators and integrators should start from [`docs/INVOICE_LIFECYCLE.md`](INVOICE_LIFECYCLE.md) and [`docs/default-finality-matrix.md`](default-finality-matrix.md).

---

## Overview

When a funded invoice passes its grace deadline without full repayment, the protocol marks it `Defaulted`. That single transition triggers a chain of accounting effects:

```
trigger_default / mark_invoice_defaulted
  │
  ├─ handle_default
  │    ├─ InvoiceStatus → Defaulted
  │    ├─ InvestmentStatus → Defaulted
  │    ├─ Insurance claims processed
  │    ├─ Events emitted (InvoiceExpired, InvoiceDefaulted)
  │    ├─ Notification sent (InvoiceDefaulted)
  │    └─ Audit entry (InvoiceDefaulted)
  │
  ├─ [via contract entrypoint] update_investor_analytics
  │    ├─ defaulted_investments++
  │    ├─ risk_score recalculated (includes default rate)
  │    ├─ risk_level re-derived
  │    ├─ tier re-derived
  │    └─ investment_limit re-derived
  │
  └─ [via analytics query] BusinessReport / InvestorReport
       └─ default_rate updated (in bps)
```

Sections below trace each link in this chain.

---

## 1. Default Transition

### Entrypoints

The default path has two entrypoints, both in [`quicklendx-contracts/src/defaults.rs`](../quicklendx-contracts/src/defaults.rs):

| Entrypoint | Caller | Notes |
|---|---|---|
| `trigger_default` | Permissionless (anyone after deadline) | Public contract function; calls `mark_invoice_defaulted` |
| `mark_invoice_defaulted` | Internal | Validates grace deadline, then calls `handle_default` |

### Guards (all in `handle_default` (`defaults.rs:299-354`))

Before the transition proceeds, five checks must pass:

1. **Status check** — invoice must be `Funded`; rejects `InvoiceAlreadyDefaulted` (1006) or `InvoiceNotAvailableForFunding` (1001) for any other status.
2. **Settlement finality guard** — `ensure_default_transition_open` (`defaults.rs:356`) rejects if `is_invoice_finalized` is true (invoice already settled).
3. **Escrow status guard** — the escrow must be `Held`. If it is `Released` or `Refunded`, the default is rejected with `InvalidStatus`.
4. **Grace deadline guard** — `ledger.timestamp() > due_date + resolved_grace_period`. Strictly greater-than; equal to the deadline fails.
5. **Transition guard** — `check_and_set_default_guard` (`defaults.rs:53`) prevents duplicate default transitions (returns `DuplicateDefaultTransition`).

The complete decision table is documented in [`docs/default-finality-matrix.md`](default-finality-matrix.md) and cross-checked by `test_default_finality_matrix.rs`.

### State mutations

```rust
// defaults.rs:318-347
InvoiceStorage::remove_from_status_invoices(env, Funded, invoice_id);
invoice.mark_as_defaulted();
InvoiceStorage::update_invoice(env, &invoice);
InvoiceStorage::add_to_status_invoices(env, Defaulted, invoice_id);

// Investment record
let mut investment = InvestmentStorage::get_investment_by_invoice(env, invoice_id);
investment.status = InvestmentStatus::Defaulted;
let claim_details = investment.process_all_insurance_claims(env);
InvestmentStorage::update_investment(env, &investment);
```

### Events and notifications

| Artifact | Emitted by | Structure |
|---|---|---|
| `InvoiceExpired` event | `defaults.rs:325`, `events.rs:837` | `{ invoice_id, business, due_date }` |
| `InvoiceDefaulted` event | `defaults.rs:347`, `events.rs:846` | `{ invoice_id, business, investor, timestamp }` |
| `InsuranceClaimed` events (per provider) | `defaults.rs:334-344`, `events.rs:847` | `{ investment_id, invoice_id, provider, coverage_amount }` |
| `NotificationType::InvoiceDefaulted` | `defaults.rs:351`, `notifications.rs:794` | Delivered to business and investor |

### Default-history counters

`handle_default` also bumps two lightweight, persistent per-address counters — independent of the risk-scoring path in [Section 2](#2-effect-on-investor-rating) below:

| Counter | Storage key | Incremented for | Read via |
|---|---|---|---|
| Business default history | `StorageKeys::business_default_history` (`biz_def_h`) | `invoice.business` | `get_business_default_history(business) -> u32` |
| Investor default history | `StorageKeys::investor_default_history` (`inv_def_h`) | `invoice.investor` (if `Some`, i.e. the invoice was funded) | `get_investor_default_history(investor) -> u32` |

Both counters saturate (never overflow/panic) and increment exactly once per successful default transition, in the same atomic `handle_default` call guarded by the transition guard described above — so they can't double-count on retries.

---

## 2. Effect on Investor Rating

### The analytics function

`update_investor_analytics` in [`quicklendx-contracts/src/verification.rs`](../quicklendx-contracts/src/verification.rs) `:1524-1579` performs the full rating recalculation:

```rust
pub fn update_investor_analytics(
    env: &Env,
    investor: &Address,
    investment_amount: i128,
    is_successful: bool, // false for defaults
) -> Result<(), QuickLendXError>
```

When called with `is_successful = false`:

1. **Increments `defaulted_investments`** (`verification.rs:1554`):
   ```rust
   verification.defaulted_investments =
       verification.defaulted_investments.saturating_add(1);
   ```

2. **Recalculates `risk_score`** via `calculate_investor_risk_score` (see [`docs/INVESTOR_TIER.md`](INVESTOR_TIER.md) for the full formula). The default-rate component is:
   ```
   default_rate = (defaulted_investments * 100) / total_investments
   risk_score += default_rate  // added point-for-point
   ```

3. **Re-derives `risk_level`**: `0-25 → Low`, `26-50 → Medium`, `51-75 → High`, `76-100 → VeryHigh`

4. **Re-derives `tier`** via `compute_investor_tier_from_stats` (evaluated top-down):
   | Tier | Max risk score | Min total invested | Min successful | Max default rate |
   |---|---|---|---|---|
   | VIP | ≤ 10 | ≥ 5,000,000 | ≥ 50 | ≤ 5% |
   | Platinum | ≤ 20 | ≥ 1,000,000 | ≥ 20 | ≤ 10% |
   | Gold | ≤ 40 | ≥ 100,000 | ≥ 10 | ≤ 15% |
   | Silver | ≤ 60 | ≥ 10,000 | ≥ 3 | ≤ 25% |
   | Basic | — | — | — | — |

5. **Recalculates `investment_limit`**:
   ```
   investment_limit = floor(base_limit × tier_multiplier × risk_multiplier / 100)
   ```

### Worked example

An investor with:
- KYC length 600 chars → +10
- 20 successful, **2 defaulted** investments → total 22, default rate = 2/22 × 100 = 9 → +9
- Total invested 500,000 → −10 (volume discount)
- Risk score = 10 + 9 − 10 = **9**
- Risk level = Low (0–25)

Before the second default, the investor was:
- Successful: 20, Defaulted: 1, Total: 21, default rate: 4.76% (4 points)
- Risk score = 10 + 4 − 10 = **4**

After the second default:
- Risk score rises to 9
- Tier check: Gold threshold (risk ≤ 40 ✔, total ≥ 100K ✔, successful ≥ 10 ✔, default rate ≤ 15% ✔) → still Gold
- Investment limit re-derived from base with Gold multiplier (3×) and Low risk multiplier (100%)

If defaults push the score past a tier threshold (e.g., risk score > 40 loses Gold → Silver), the investor's lending capacity decreases immediately.

### Current wiring

As of this writing, `update_investor_analytics` is exposed as a **public contract entrypoint** in `lib.rs:2283-2289`:

```rust
pub fn update_investor_analytics(
    env: Env, investor: Address, amount: i128, is_success: bool,
) -> Result<(), QuickLendXError>
```

It is **not automatically called** by `handle_default` or `settle_invoice_internal`. This means the investor's risk score and tier are not auto-updated on default — the update requires an explicit call (by an admin, a cron job, or a future wiring change). The `InvestmentStatus` is correctly set to `Defaulted`, which downstream analytics queries can read, but the `InvestorVerification` counters are not automatically incremented.

---

## 3. Effect on Business Rating

Defaulted invoices affect the business's `BusinessReport`, generated by `AnalyticsStorage::generate_business_report` (`analytics.rs:917-997`):

```rust
let default_rate = if invoices_uploaded > 0 {
    (defaulted_invoices.saturating_mul(10000)).saturating_div(invoices_uploaded) as i128
} else {
    0
};
```

The rate is expressed in **basis points** (0–10000, where 10000 = 100%). This feeds into the `BusinessReport` returned by `get_business_report`.

---

## 4. Audit Trail

Every default produces audit artifacts in three systems:

### 4.1 Events (on-chain, indexed)

| Event | Topic (`Symbol`) | Fields |
|---|---|---|
| `InvoiceExpired` | `"inv_exp"` | `invoice_id, business, due_date` |
| `InvoiceDefaulted` | `"inv_def"` | `invoice_id, business, investor, timestamp` |

### 4.2 Audit log (append-only, hash-chained)

The audit module (`audit.rs`) records an entry with `AuditOperation::InvoiceDefaulted` (tag `5`, symbol `inv_def`). Each entry includes:

| Field | Description |
|---|---|
| `audit_id` | Unique 32-byte ID with embedded timestamp + counter |
| `invoice_id` | The defaulted invoice |
| `operation` | `AuditOperation::InvoiceDefaulted` |
| `actor` | Address that triggered the default |
| `timestamp` | Ledger timestamp |
| `amount` | Invoice amount (the economic impact) |
| `block_height` | Ledger sequence number |
| `prev_hash` | SHA-256 of the previous entry in this invoice trail |

The hash chain guarantees **tamper evidence**: changing any field in any past entry changes the expected `prev_hash` of all subsequent entries. Verify a trail with:

```rust
AuditStorage::verify_audit_chain(env, &invoice_id);    // returns bool
AuditStorage::first_audit_chain_divergence(env, &invoice_id); // Option<u32>
```

### 4.3 Notifications (off-chain delivery)

`NotificationSystem::notify_invoice_defaulted` (`notifications.rs:794`) publishes a `NotificationType::InvoiceDefaulted` record to both the business and the investor for off-chain delivery (email, dashboard alert, etc.).

### 4.4 Traceability diagram

```
Trigger default
  │
  ├─ emit_invoice_expired       ──►  Event: InvoiceExpired
  ├─ emit_invoice_defaulted     ──►  Event: InvoiceDefaulted
  ├─ emit_insurance_claimed     ──►  Event: InsuranceClaimed  (per provider)
  ├─ log_operation(InvoiceDefaulted) ──►  Audit trail entry (append-only, hash-chained)
  └─ notify_invoice_defaulted   ──►  Notification (off-chain)
```

---

## 5. Related Documents

| Document | Relevance |
|---|---|
| [`docs/INVESTOR_TIER.md`](INVESTOR_TIER.md) | Full risk score formula, tier thresholds, and investment limit math |
| [`docs/SETTLEMENT_ACCOUNTING.md`](SETTLEMENT_ACCOUNTING.md) | Partial payment accounting before default |
| [`docs/default-finality-matrix.md`](default-finality-matrix.md) | Decision table for when default is allowed (status × finality × escrow) |
| [`docs/INVOICE_LIFECYCLE.md`](INVOICE_LIFECYCLE.md) | State diagram: Funded → Defaulted transition |
| [`docs/ERROR_CODES.md`](ERROR_CODES.md) | Error codes: `InvoiceAlreadyDefaulted` (1006), `DuplicateDefaultTransition` (1010) |
| [`quicklendx-contracts/src/defaults.rs`](../quicklendx-contracts/src/defaults.rs) | Default transition implementation |
| [`quicklendx-contracts/src/verification.rs`](../quicklendx-contracts/src/verification.rs) | Investor risk score, tier, and limit derivation |
| [`quicklendx-contracts/src/audit.rs`](../quicklendx-contracts/src/audit.rs) | Append-only audit log with hash-chain integrity |
| [`quicklendx-contracts/src/analytics.rs`](../quicklendx-contracts/src/analytics.rs) | Business and investor report generation |

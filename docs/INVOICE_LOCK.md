# Invoice Lock Duration and Auto-Release

> Audience: **contributors** who need to understand how long an invoice can be
> locked and what mechanisms exist (or do not exist) for automatic release.
> For the escrow state machine see [`docs/ESCROW.md`](ESCROW.md); for invoice
> lifecycle transitions see [`docs/INVOICE_LIFECYCLE.md`](INVOICE_LIFECYCLE.md).

## What "locked" means

An invoice is "locked" when a contract-enforced restriction prevents financial
operations on it. QuickLendX has two independent locking mechanisms:

| Mechanism | Who applies it | What it blocks | Auto-release? |
|-----------|---------------|----------------|---------------|
| **Admin freeze** (`FrozenInvoice`) | Admin | `place_bid`, `accept_bid`, `record_payment`, `settle_invoice` | **No** |
| **Escrow hold** (`EscrowStatus::Held`) | Automatically on `accept_bid_and_fund` | Release/refund/withdraw of escrowed funds | **No** (time-based) |

---

## 1. Admin freeze (`FrozenInvoice`)

Admin freezes are a permanent kill switch for an invoice. There is **no
auto-release** and **no time-based expiry** — once frozen, the invoice stays
frozen indefinitely.

### Entrypoint

```rust
fn freeze_invoice(env: Env, admin: Address, invoice_id: BytesN<32>)
    -> Result<(), QuickLendXError>
```

- **Caller**: Admin (`AdminStorage::require_admin` enforced).
- **Effect**: `InvoiceFrozen` flag set in persistent storage.

### Enforcement points

| Operation | File | Line |
|-----------|------|------|
| `place_bid` | `contract.rs:207` | `if InvoiceStorage::is_frozen(…)` → `InvoiceFrozen` |
| `accept_bid` | `contract.rs:228` | `if InvoiceStorage::is_frozen(…)` → `InvoiceFrozen` |
| `record_payment` | `settlement.rs:249` | `if InvoiceStorage::is_frozen(…)` → `InvoiceFrozen` |
| `settle_invoice` | `settlement.rs:393` | `if InvoiceStorage::is_frozen(…)` → `InvoiceFrozen` |

There is **no public `unfreeze_invoice` entrypoint**. The underlying storage
function `set_frozen(env, id, false)` does exist in `storage.rs:273` but is
not exposed through the contract interface.

### Why no auto-release

Admin freeze is designed as a manual safety valve for incident response.
Releasing it always requires an explicit on-chain action (a contract upgrade
or storage migration). See the [incident response
runbook](RUNBOOK_INCIDENT_RESPONSE.md) for operational guidance.

---

## 2. Escrow hold (funds locked after bid acceptance)

When `accept_bid_and_fund` succeeds the investor's tokens move into the
contract and the escrow status becomes `Held`. The funds stay locked until
**exactly one** terminal action is explicitly called:

| Action | Who can call | Result |
|--------|-------------|--------|
| `release_escrow_funds` | Admin (via invoice verification) | Funds → business; escrow → `Released` |
| `refund_escrow_funds` | Admin or business owner | Funds → investor; escrow → `Refunded` |
| `withdraw_investment` | Investor themself | Funds → investor; invoice → `Verified` |

There is **no time-based auto-expiry** on the escrow hold. An escrow stays
`Held` until one of the above is explicitly invoked. Detailed state-machine
coverage is in [`docs/ESCROW.md`](ESCROW.md).

---

## 3. Default path — the closest thing to a time-based release

The only mechanism that references a clock is the **grace period + default**
path. After the invoice's `due_date` plus a grace period, anyone can trigger a
default, which refunds escrowed funds to the investor. This is not an automatic
on-chain timer — it still requires an explicit call — but the eligibility
window is determined by ledger time.

### Grace period resolution

The grace period is resolved in `defaults.rs:104` with this fallback order:

1. Explicit override passed to `mark_invoice_defaulted`
2. Protocol config (`grace_period_seconds` from contract initialization)
3. Hardcoded `DEFAULT_GRACE_PERIOD` (7 days)

```rust
// defaults.rs:10
pub const DEFAULT_GRACE_PERIOD: u64 = 7 * 24 * 60 * 60; // 604_800 seconds

// defaults.rs:78
const MAX_GRACE_PERIOD: u64 = 30 * 24 * 60 * 60;        // 2_592_000 seconds
```

The grace deadline is computed in `invoice.rs:353`:

```rust
pub fn grace_deadline(&self, grace_period: u64) -> u64 {
    self.due_date.saturating_add(grace_period)
}
```

### Default trigger

```rust
fn mark_invoice_defaulted(
    env: &Env,
    invoice_id: &BytesN<32>,
    grace_period: Option<u64>,
) -> Result<(), QuickLendXError>
```

- **Caller**: Anyone (permissionless).
- **Precondition**: `now > due_date + grace_period` AND escrow is `Held`.
- **Effect**: Escrow refunded to investor; invoice → `Defaulted`; insurance
  claims processed.

A bounded scan entrypoint (`scan_funded_invoice_expirations` in `defaults.rs:228`)
iterates funded invoices in configurable batches and triggers defaults for any
whose grace deadline has passed. Repeated calls advance a rotating cursor until
every funded invoice has been visited.

### Timeline summary

```
funded_at              due_date               due_date + grace_period
    │                      │                          │
    ▼                      ▼                          ▼
┌────────┐            ┌─────────┐              ┌──────────┐
│ Funded │  ──window──│ Overdue │  ──grace───  │ Default  │
│ escrow │            │         │    period     │ eligible │
│ Held   │            │         │              │ (trigger │
└────────┘            └─────────┘              │  needed) │
                                               └──────────┘
```

---

## 4. Dispute lock

Opening a dispute on an invoice does **not** directly move funds, but it
blocks settlement finalization while `dispute_status != None`. There are
**no on-chain timeouts** on dispute states — a dispute stays `Disputed` or
`UnderReview` indefinitely until an authorised party transitions it. See
[`docs/DISPUTE.md`](DISPUTE.md).

---

## Error codes

| Error | Code | Symbol | When raised |
|-------|------|--------|-------------|
| `InvoiceFrozen` | 1007 | `INV_FR` | Operation blocked by admin freeze |

Full error catalog: [`docs/ERROR_CODES.md`](ERROR_CODES.md).

## Related documentation

- [`docs/INVOICE_LIFECYCLE.md`](INVOICE_LIFECYCLE.md) — full state diagram and entrypoint reference.
- [`docs/ESCROW.md`](ESCROW.md) — escrow lifecycle, release and refund conditions.
- [`docs/DISPUTE.md`](DISPUTE.md) — dispute state machine and its effect on settlement.
- [`docs/default-finality-matrix.md`](default-finality-matrix.md) — default transition decision table.
- [`docs/ERROR_CODES.md`](ERROR_CODES.md) — complete typed error reference.

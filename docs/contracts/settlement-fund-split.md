# Settlement Fund Split

**Audience:** Contributors reviewing or modifying settlement logic.

This document explains how `settle_invoice_internal` (in
`quicklendx-contracts/src/settlement.rs`) splits the total payment across three
destinations: the **investor**, the **platform treasury**, and (implicitly) the
**business** — who has already received their invoice funds via escrow release.

---

## Fund flow at a glance

```
Business pays invoice
         │
         ▼
 ┌──────────────────┐
 │  total_paid      │  (sum of all partial payments on the invoice)
 └──────────────────┘
         │
         ├──────────────────────────────────────────────┐
         │                                              │
         ▼                                              ▼
 investor_return                                 platform_fee
 = total_paid − platform_fee                     = ⌊gross_profit × fee_bps / 10_000⌋
         │                                              │
         ▼                                              ▼
  transferred to                              routed to treasury
  investor address                            (or contract address
                                               if no treasury set)
```

The business already received the funded amount when the escrow was released
during settlement (see `release_escrow` call inside `settle_invoice_internal`).
No separate transfer to the business happens at split time.

---

## The split formula

Implemented in `src/profits.rs` (`PlatformFee::calculate_with_fee_bps`):

```
gross_profit  = max(0, total_paid − investment_amount)

platform_fee  = ⌊gross_profit × fee_bps / 10_000⌋

investor_return = total_paid − platform_fee
```

### Key properties

| Property | Guarantee |
|---|---|
| **No dust** | `investor_return + platform_fee == total_paid` always |
| **Fee rounds down** | `platform_fee` uses integer floor division, favouring the investor |
| **No fee on loss** | If `total_paid ≤ investment_amount`, `gross_profit = 0` and `platform_fee = 0`; investor recovers whatever was paid |
| **Overflow-safe** | Intermediate multiplication uses `checked_mul`; saturating fallback returns `(total_paid, 0)` |

### Default and maximum fee rates

| Constant | Value | Location |
|---|---|---|
| `DEFAULT_PLATFORM_FEE_BPS` | `200` (2 %) | `src/profits.rs` |
| `MAX_PLATFORM_FEE_BPS` | `1_000` (10 %) | `src/profits.rs` |
| `BPS_DENOMINATOR` | `10_000` | `src/profits.rs` |

The live fee rate is stored in contract instance storage under key `plt_fee` and
is read at settlement time via `PlatformFee::get_config`. If the key is absent, the
default 2 % applies.

---

## Worked examples

### Example 1 — Normal profit (fee = 2 %)

```
investment_amount = 1_000
total_paid        = 1_100
fee_bps           = 200

gross_profit  = 1_100 − 1_000 = 100
platform_fee  = ⌊100 × 200 / 10_000⌋ = 2
investor_return = 1_100 − 2 = 1_098
```

**Transfers:** 1 098 → investor, 2 → treasury.

### Example 2 — Exact repayment (no profit)

```
investment_amount = 1_000
total_paid        = 1_000
fee_bps           = 200

gross_profit  = 0
platform_fee  = 0
investor_return = 1_000
```

**Transfer:** 1 000 → investor. No fee collected.

### Example 3 — Underpayment / loss

```
investment_amount = 1_000
total_paid        = 900
fee_bps           = 200

gross_profit  = max(0, 900 − 1_000) = 0
platform_fee  = 0
investor_return = 900
```

**Transfer:** 900 → investor. Investor absorbs the 100-unit shortfall; no fee.

### Example 4 — Rounding (fee rounds down)

```
investment_amount = 1_000
total_paid        = 1_049
fee_bps           = 200

gross_profit  = 49
platform_fee  = ⌊49 × 200 / 10_000⌋ = ⌊0.98⌋ = 0
investor_return = 1_049
```

**Transfer:** 1 049 → investor, 0 → treasury. The rounding loss stays with the
platform (fee = 0 instead of 0.98).

---

## Where the split happens in code

`settle_invoice_internal` in `src/settlement.rs`:

```rust
// 1. Calculate the split
let (investor_return, platform_fee) = match crate::fees::FeeManager::calculate_platform_fee(
    env,
    investment.amount,   // investment_amount
    invoice.total_paid,  // total_paid
) {
    Ok(result) => result,
    // Fallback when fee config is absent (tests / fresh deploy)
    Err(QuickLendXError::StorageKeyNotFound) => {
        crate::profits::calculate_profit(env, investment.amount, invoice.total_paid)
    }
    Err(error) => return Err(error),
};

// 2. Accounting invariant — no disbursement until verified
let disbursement_total = investor_return
    .checked_add(platform_fee)
    .ok_or(QuickLendXError::InvalidAmount)?;
if disbursement_total != invoice.total_paid {
    return Err(QuickLendXError::InvalidAmount);
}

// 3. Transfer to investor
transfer_funds(env, &invoice.currency, &business_address, &investor_address, investor_return)?;

// 4. Route fee to treasury (skipped when fee == 0)
if platform_fee > 0 {
    let fee_recipient = crate::fees::FeeManager::route_platform_fee(
        env, &invoice.currency, &business_address, platform_fee,
    )?;
    emit_platform_fee_routed(env, invoice_id, &fee_recipient, platform_fee);
}
```

The hard check at step 2 (`disbursement_total != invoice.total_paid`) means any
rounding bug that causes the two sides to diverge aborts the transaction before
money moves.

---

## Treasury routing

`FeeManager::route_platform_fee` sends `platform_fee` to:

1. The treasury address stored under key `plt_fee` (set via `set_treasury_address`
   / two-step rotation — see [`docs/contracts/revenue-split.md`](./revenue-split.md)),
   **if one is configured**.
2. `env.current_contract_address()` (the contract itself) otherwise.

The two-step rotation flow that protects treasury address changes is documented in
[`docs/contracts/revenue-split.md`](./revenue-split.md#treasury-address-rotation).

---

## Revenue split (second-level split of the platform fee)

After the platform fee is collected it can be further split among treasury,
developer, and platform pools via `distribute_revenue`. This is an independent,
admin-triggered operation and does not affect the per-invoice accounting described
above. See [`docs/contracts/revenue-split.md`](./revenue-split.md) for full details.

---

## Events emitted at settlement

| Event topic | Fields | Purpose |
|---|---|---|
| `inv_stl` | `invoice_id, investor_return, platform_fee` | Settlement totals (consumed by indexers) |
| `inv_stlf` | `invoice_id, final_amount, paid_at` | Finality marker with timestamp |
| `fee_routed` | `invoice_id, fee_recipient, platform_fee` | Where the fee went |

---

## Invariants enforced by tests

| Test file | What it covers |
|---|---|
| `src/test_settlement_accounting_identity.rs` | `investor_return + platform_fee == total_paid` for all inputs |
| `src/test_profit_fee_formula.rs` | Formula correctness, rounding, boundary values |
| `src/test_revenue_split.rs` | Revenue distribution after fee collection |
| `src/test_settlement.rs` | End-to-end settlement with fund transfer assertions |
| `tests/profit_fee_golden.rs` | Golden-file regression for the profit/fee formula |

---

## Related documents

- [`docs/contracts/settlement.md`](./settlement.md) — Payment record storage and partial payment architecture
- [`docs/contracts/revenue-split.md`](./revenue-split.md) — Revenue distribution configuration and treasury rotation
- [`docs/contracts/fee-model.md`](./fee-model.md) — Full fee model including volume tiers and late/early payment surcharges
- [`docs/contracts/profit-fee-formula.md`](./profit-fee-formula.md) — Detailed formula derivation and fuzz results
- [`docs/PLATFORM_FEES.md`](../../docs/PLATFORM_FEES.md) — Fee schedule and tenant override documentation

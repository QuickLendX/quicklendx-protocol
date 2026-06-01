# Concurrent-Acceptance Race Regression — `accept_bid_and_fund`

> **Reference:** `src/lib.rs::accept_bid_and_fund`, `src/escrow.rs::accept_bid_and_fund`,
> `src/payments.rs::create_escrow`
> **Test file:** `src/test_accept_bid_race.rs`

---

## Background

Soroban executes each transaction atomically and sequentially. However, when
multiple investors submit `accept_bid_and_fund` for the **same invoice** within
the same ledger, validators can order those transactions adversarially. The
outcome must be identical regardless of ordering:

- **Exactly one** acceptance succeeds.
- **All subsequent** acceptance attempts for the same invoice fail with a stable,
  deterministic error.
- **Zero partial state** (escrow records, investment records, token transfers)
  lingers from any losing leg.

---

## Security Severity

| Aspect | Rating |
|--------|--------|
| Confidentiality impact | None |
| Integrity impact | **Critical** |
| Availability impact | High |
| Exploitability | Medium (adversarial ledger ordering required) |

A successful double-funding would:

1. Create a second escrow record whose funds are locked with no redemption path
   (permanent loss of investor capital).
2. Produce duplicate investment records, corrupting analytics and settlement
   accounting.
3. Potentially allow the business to withdraw from one escrow while the other
   remains in `Held` state indefinitely.

---

## Protocol Defence Layers

The protocol defends against this race at **two independent layers**:

### Layer 1 — `load_accept_bid_context` (escrow.rs)

Before any funds move, the context loader performs three independent checks:

```rust
// Check 1: invoice status
if invoice.status == InvoiceStatus::Funded {
    return Err(QuickLendXError::InvoiceAlreadyFunded);
}

// Check 2: invoice funding metadata
if invoice.funded_amount != 0 || invoice.funded_at.is_some() || invoice.investor.is_some() {
    return Err(QuickLendXError::InvalidStatus);
}

// Check 3: existing escrow / investment records
if EscrowStorage::get_escrow_by_invoice(env, invoice_id).is_some()
    || InvestmentStorage::get_investment_by_invoice(env, invoice_id).is_some()
{
    return Err(QuickLendXError::InvalidStatus);
}
```

### Layer 2 — `create_escrow` (payments.rs)

Even if the higher-level check is bypassed (e.g. via direct contract call),
`create_escrow` re-checks for an existing escrow record before executing any
token transfer. This makes the guard effective even against attacks that
skip the public API entry point.

### Layer 3 — Reentrancy guard

`accept_bid_and_fund` is wrapped in `reentrancy::with_payment_guard`, preventing
re-entrant calls during the token-transfer phase.

---

## Expected Error Codes

| Scenario | Error |
|----------|-------|
| Invoice already in `Funded` status | `QuickLendXError::InvoiceAlreadyFunded` (1002) |
| Escrow or investment record already exists | `QuickLendXError::InvalidStatus` (1401) |
| Invoice status is not `Verified` | `QuickLendXError::InvoiceNotAvailableForFunding` (1001) |
| Invoice in `Refunded` terminal state | `QuickLendXError::InvalidStatus` (1401) |

---

## Test Coverage

All tests live in `src/test_accept_bid_race.rs` and run via:

```bash
cargo test test_accept_bid_race
```

### Test Matrix

| Test Name | Ordering Tested | What Is Asserted |
|-----------|-----------------|------------------|
| `test_race_ordering_a_wins` | A → B | A succeeds; B gets `InvoiceAlreadyFunded`/`InvalidStatus`; invoice is `Funded`; only A's bid is `Accepted` |
| `test_race_ordering_b_wins` | B → A | B succeeds; A gets `InvoiceAlreadyFunded`/`InvalidStatus`; only B's bid is `Accepted` |
| `test_race_same_bid_both_orderings` | replay | Replay of the same bid fails deterministically |
| `test_race_no_partial_state_on_failure` | A → B | Loser (B) has zero escrow, zero investment, unchanged token balance |
| `test_race_idempotent_after_accept` | multiple retries | All retries fail; contract state is stable |
| `test_race_different_bids_only_one_escrow` | A → B | Exactly one escrow record exists in `Held` status |
| `test_race_three_concurrent_investors` | A → B → C | First wins; B and C both fail; contract holds exactly one bid amount |
| `test_race_accept_after_refund` | fund → refund → accept | Post-refund acceptance is rejected; contract balance returns to 0 |

### Edge Cases Covered

- Same bid accepted twice (replay attack)
- Different bids on the same invoice (simultaneous competing investors)
- Three-way race (A, B, C)
- Post-refund re-acceptance attempt
- Token balance verification at every step

---

## Invariants Validated

1. **One escrow per invoice** — `get_escrow_details(invoice_id)` returns exactly
   one record after any number of concurrent acceptance attempts.
2. **No double-credit** — `escrow.amount == bid_amount` (never `2 * bid_amount`).
3. **No orphan investments** — losing investors have `get_investments_by_investor == []`.
4. **Token accounting** — `token.balance(contract) == bid_amount` after a single
   successful acceptance; `0` after a refund.
5. **Bid state** — winner's bid is `Accepted`; loser's bid is **not** `Accepted`.

---

## How to Run

```bash
# From the repo root:
cd quicklendx-contracts
cargo test test_accept_bid_race -- --nocapture

# Run the full test suite including this regression:
cargo test
```

---

## References

- `src/escrow.rs` — `load_accept_bid_context`, `accept_bid_and_fund`
- `src/payments.rs` — `create_escrow`, `EscrowStorage`
- `src/lib.rs` — `accept_bid_and_fund` (public entry point)
- `src/reentrancy.rs` — `with_payment_guard`
- `src/errors.rs` — `QuickLendXError::InvoiceAlreadyFunded`, `InvalidStatus`
- `docs/contracts/security.md` — reentrancy guard documentation
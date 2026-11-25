# Solvency Invariant

## Definition

The protocol enforces the following invariant:

> Total investor payouts ≤ Total collected funds

This is implemented using settlement logic to ensure conservation of value.

---

## Rationale

This invariant guarantees that:
- The protocol never overpays investors
- No funds are created out of thin air
- Escrow integrity is preserved

A violation would indicate:
- Accounting inconsistency
- Potential exploit
- Broken settlement logic

---

## Enforcement

The invariant is enforced via:

- `validate_solvency_invariant(...)` in `src/invariants.rs`
- Checked after every simulated state transition in tests

---

## Edge Cases Covered

- Full funding vs partial funding
- Boundary conditions (`funded == face`)
- Repeated settlement simulations
- Randomized lifecycle inputs

## Property-Tested Accounting

`quicklendx-contracts/src/test_settlement_conservation_props.rs` checks randomized
settlement sequences with different principal amounts, platform fee rates,
insurance coverage, and revenue-split shares. The properties assert that:

- Repayment ledger buckets sum to the cumulative amount paid after every
  partial payment, and no bucket decreases as payments arrive.
- Final investor and platform-fee token payouts sum exactly to the invoice
  amount collected, including when settlement follows multiple partial
  payments.
- Treasury, developer, and platform revenue allocations sum exactly to
  collected fees; the platform receives any division remainder.

Run the suite with `PROPTEST_CASES=1000 cargo test --features fuzz-tests --lib test_settlement_conservation_props`
from `quicklendx-contracts`. The test defaults to 32 cases when
`PROPTEST_CASES` is not set and stores minimized failures in a source-adjacent
proptest regression file.

---

## Security Classification

**P0 — Critical**

Any violation of this invariant represents a protocol-wide solvency failure.

---

## Usage

This invariant can be used:
- In tests (stateful validation)
- In off-chain monitoring tools
- As a guardrail for future feature changes
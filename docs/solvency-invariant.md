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
# Revenue Distribution Fuzz Harness

## Overview

This document describes the property-based fuzz harness for
`fees::FeeManager::distribute_revenue` in
[`src/fees.rs`](../src/fees.rs).

The harness lives in
[`src/test_fuzz_distribute_revenue.rs`](../src/test_fuzz_distribute_revenue.rs)
and is compiled only when the `fuzz-tests` feature flag is active.

---

## Run Command

```bash
# Standard (50,000 cases — meets acceptance criterion)
PROPTEST_CASES=50000 cargo test --features fuzz-tests test_fuzz_distribute_revenue

# Quick smoke run (256 cases — fast CI)
cargo test --features fuzz-tests test_fuzz_distribute_revenue

# Extended coverage
PROPTEST_CASES=200000 cargo test --features fuzz-tests test_fuzz_distribute_revenue
```

---

## Fuzzing Strategy

### Input Space

The harness generates:

| Input | Strategy |
|---|---|
| `(treasury_bps, developer_bps, platform_bps)` | Stratified: `t ∈ [0,10000]`, `d ∈ [0, 10000−t]`, `p = 10000−t−d` — always valid |
| `pending` (fee amount) | `i128 ∈ [1, 10_000_000_000_000]` |
| Boundary amounts | `{1, 2, MAX−1, MAX}` explicitly exercised |
| Near-`i128::MAX` amounts | `i128::MAX − offset`, `offset ∈ [0, 1e9]` |
| `min_distribution_amount` | Swept from `1` to `MAX_AMOUNT` |

### Invalid-input classes (deterministic tests)

The harness also runs deterministic edge-case tests (no proptest loop)
covering:

- `pending == 0` → `OperationNotAllowed`
- `bps sum > 10,000` → error
- `bps sum < 10,000` → error
- Second distribution on same period (idempotency) → `OperationNotAllowed`

---

## Invariants Tested

### (a) Conservation of value

```
treasury_amount + developer_amount + platform_amount == pending
```

The production code assigns platform the **exact remainder** after two
`floor`-divisions, so this invariant is satisfied exactly — not within a
dust tolerance.

**Dust bound: 0 stroops.** No stroop is ever discarded.

### (b) No invalid allocations

- All three amounts are `≥ 0`
- Overflow is caught via `checked_mul` / `checked_sub`; the function
  returns `QuickLendXError::ArithmeticOverflow` rather than silently
  wrapping
- Underflow is prevented by the explicit negativity guard in the
  production code

### (c) Zero-bps behaviour

| Config | Expected |
|---|---|
| `treasury_bps = 0` | `treasury_amount == 0` |
| `developer_bps = 0` | `developer_amount == 0` |
| `treasury_bps = developer_bps = 0` | only platform receives value |

### (d) Order independence

Swapping `treasury_bps ↔ developer_bps` must not change the aggregate
`treasury_amount + developer_amount`. Because both amounts use identical
`floor(pending × bps / 10_000)` arithmetic, the sums are always equal.

---

## Rounding Assumptions

The production code uses integer floor division throughout:

```rust
treasury_amount  = floor(pending × treasury_bps  / 10_000)
developer_amount = floor(pending × developer_bps / 10_000)
platform_amount  = pending - treasury_amount - developer_amount   // remainder
```

### Why this is lossless

Floor division on two of the three shares can at most discard `1 stroop`
per share (i.e., up to `2 stroops` total from the two rounding steps).
The remainder assignment to `platform` recaptures both discards exactly,
so the sum is always exact.

### Dust accumulation over many periods

Each individual distribution call accumulates **0 stroops** of dust.
Over N periods the total dust is still **0**. There is no compound
rounding error.

---

## Known Limitations

1. **Fixed three-recipient model.** The production function has a hard-coded
   `(treasury, developer, platform)` triple. The harness cannot test an
   arbitrary number of recipients because that is not the production API.

2. **Auth mocking.** The harness uses `env.mock_all_auths()` to bypass
   Soroban's authorisation checks. Real on-chain behaviour requires a
   validly signed transaction; the harness does not cover auth rejection
   paths.

3. **Period handling.** All fuzz cases use `period = 0` (current ledger
   period). Multi-period accumulation is tested only in the deterministic
   idempotency test, not across the fuzz sweep.

4. **Treasury-address cross-check.** When `treasury_share_bps > 0` and a
   fee treasury address is configured in `PlatformFeeConfig`, the
   production code checks that the two addresses match. The harness does
   not set a fee treasury (leaving it `None`), so this guard is not
   exercised in the fuzz path. It is covered by the deterministic tests in
   `test_fees.rs` / `test_revenue_split.rs`.

---

## Security Implications of Dust Accumulation

Because `platform_amount = pending − treasury_amount − developer_amount`
(remainder, not floor), **no dust accumulates** in the current
implementation. The security analysis is therefore:

| Scenario | Risk | Mitigated by |
|---|---|---|
| Repeated distributions over many periods | Zero dust per call → zero compound drift | Remainder assignment |
| Very small pending amounts (1–9 stroops) | All rounding may go to platform | Correct by design; platform acts as dust absorber |
| Very large pending amounts (near i128::MAX) | Overflow in `checked_mul` | `ArithmeticOverflow` error returned; no silent wrap |
| Invalid bps (sum ≠ 10,000) | Could create allocation gaps or double-spend | `validate_revenue_shares` called at distribution time |

### If the rounding strategy changes

If a future change moves away from remainder assignment (e.g., platform
also uses floor division), dust of up to `2 stroops per distribution call`
would accumulate in the contract's accounting with no clear owner. This
must be explicitly modelled and bounded before deployment.

---

## Regression Tracking

Proptest automatically persists failing seeds to:

```
proptest-regressions/distribute_revenue.txt
```

This file is committed to source control so every developer and CI run
replays previously discovered failures before generating novel cases.

---

## Test Inventory

### Proptest suites (randomised, ≥ 50,000 cases each)

| Test function | Invariant |
|---|---|
| `test_fuzz_distribute_revenue_conservation` | (a) exact value conservation |
| `test_fuzz_distribute_revenue_boundary_amounts` | (a) conservation at 1, 2, MAX−1, MAX stroops |
| `test_fuzz_distribute_revenue_zero_treasury_bps` | (c) zero-bps → zero amount |
| `test_fuzz_distribute_revenue_zero_developer_bps` | (c) zero-bps → zero amount |
| `test_fuzz_distribute_revenue_all_to_platform` | (c) platform absorbs 100% |
| `test_fuzz_distribute_revenue_all_to_treasury` | (c) treasury absorbs 100% |
| `test_fuzz_distribute_revenue_order_independence` | (d) swap treasury ↔ developer |
| `test_fuzz_distribute_revenue_below_min_threshold` | threshold rejection |
| `test_fuzz_distribute_revenue_at_min_threshold` | threshold boundary |
| `test_fuzz_distribute_revenue_no_silent_overflow` | (b) no overflow wrap |
| `test_fuzz_distribute_revenue_no_negative_amounts` | (b) all amounts ≥ 0 |

### Deterministic edge-case tests (always run)

| Test function | Coverage |
|---|---|
| `distribute_revenue_zero_pending_rejected` | `OperationNotAllowed` on zero pending |
| `distribute_revenue_one_stroop_equal_split` | 1-stroop conservation |
| `distribute_revenue_one_stroop_all_platform` | 1-stroop all-platform |
| `distribute_revenue_zero_treasury_zero_developer` | all-zero → platform gets all |
| `distribute_revenue_mixed_zero_nonzero_bps` | mixed 0 / non-zero bps |
| `distribute_revenue_max_bps_treasury` | treasury = 10,000 bps |
| `distribute_revenue_max_bps_developer` | developer = 10,000 bps |
| `distribute_revenue_very_small_amount_1` | 1-stroop uneven bps |
| `distribute_revenue_very_small_amount_2` | 2-stroop uneven bps |
| `distribute_revenue_large_amount_even_split` | MAX_AMOUNT conservation |
| `distribute_revenue_large_amount_all_treasury` | MAX_AMOUNT single recipient |
| `distribute_revenue_bps_over_10000_rejected` | invalid bps rejection |
| `distribute_revenue_bps_under_10000_rejected` | invalid bps rejection |
| `distribute_revenue_idempotent_second_call_rejected` | re-distribution blocked |
| `distribute_revenue_platform_is_exact_remainder_no_dust` | remainder = exact |
| `distribute_revenue_no_excessive_dust_1_stroop_split` | 1-stroop: platform absorbs rounding |

---

## References

- [`src/fees.rs`](../src/fees.rs) — production implementation
- [`src/test_fuzz_distribute_revenue.rs`](../src/test_fuzz_distribute_revenue.rs) — this harness
- [`proptest-regressions/distribute_revenue.txt`](../proptest-regressions/distribute_revenue.txt) — persisted seeds
- [Proptest documentation](https://docs.rs/proptest/)
- [Property-based testing](https://hypothesis.works/articles/what-is-property-based-testing/)

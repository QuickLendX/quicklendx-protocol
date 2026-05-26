# Fuzz Testing — QuickLendX Protocol

## Overview

Property-based fuzz tests validate core protocol invariants under randomised
inputs using [`proptest`](https://docs.rs/proptest/).  Tests are gated behind
`--features fuzz-tests` so they never block a normal `cargo test` run.

---

## Invariants Covered (Issue #812)

### 1. `total_paid <= total_due`

**File:** `src/test_fuzz_invariants.rs`

Every payment step caps the applied amount to `remaining_due`, so the running
total can never exceed the invoice face value.

| Test | What it validates |
|------|-------------------|
| `fuzz_settlement_total_paid_conservation` | `investor_payout + protocol_fee == total_collected` for all valid inputs |
| `fuzz_total_paid_never_exceeds_total_due` | Simulated multi-step payments never exceed `face_value` |
| `fuzz_investor_profit_sign_consistency` | `investor_profit >= 0` iff `payout >= funded` |

### 2. Escrow transitions valid and non-reentrant

**File:** `src/test_fuzz_invariants.rs`

Once an escrow is settled (Released/Refunded), re-settlement is a no-op.
The arithmetic layer enforces this via the finalization guard.

| Test | What it validates |
|------|-------------------|
| `fuzz_settlement_idempotent_non_reentrant` | Same inputs always produce identical results (pure function) |
| `fuzz_escrow_release_non_reentrant` | After full settlement `remaining_due == 0`; second release applies 0 |
| `fuzz_escrow_invalid_inputs_always_rejected` | Zero face, funded > face, fee > 100% always return `None` |

### 3. Bid caps enforced (`MAX_BIDS_PER_INVOICE`, per-investor limits)

**File:** `src/test_fuzz_invariants.rs`

The verification module enforces tier × risk multipliers and per-investment
caps for High/VeryHigh risk investors.

| Test | What it validates |
|------|-------------------|
| `fuzz_bid_effective_limit_bounded` | Effective limit ≤ `base_limit × 10` (VIP × Low) |
| `fuzz_bid_cap_over_limit_always_rejected` | Bid of `limit + 1` always rejected with typed error |
| `fuzz_bid_within_limit_always_accepted` | Bid ≤ effective limit always accepted for verified investor |
| `fuzz_bid_unverified_always_rejected` | Pending/Rejected/None investors always rejected |
| `fuzz_per_investment_cap_enforced` | High/VeryHigh per-bid caps enforced independently of limit |
| `fuzz_zero_bid_always_rejected` | Zero-amount bids always return `ZeroAmount` |
| `fuzz_tier_monotone_in_investment` | Higher activity → tier multiplier is non-decreasing |
| `fuzz_risk_score_full_coverage` | All scores 0–100 map to a valid `RiskLevel` |
| `fuzz_risk_score_over_max_rejected` | Scores > 100 always return `None` |

### Cross-invariant integration

| Test | What it validates |
|------|-------------------|
| `fuzz_valid_bid_settlement_no_investor_loss` | Valid bid + settlement → investor profit ≥ 0 |

---

## Existing Arithmetic Fuzz Tests

**File:** `src/test_fuzz.rs` (deterministic sweep-based, no proptest required)

| Test | Invariant |
|------|-----------|
| `fuzz_settlement_conservation_invariant` | Conservation for all boundary inputs |
| `fuzz_settlement_penalty_monotonicity` | Higher penalty_bps → higher late_penalty |
| `fuzz_settlement_fee_reduces_payout` | Higher fee_bps → lower investor_payout |
| `fuzz_fees_never_exceed_principal` | All fees ≤ principal |
| `fuzz_fees_cap_enforcement` | Rate > cap → `None` |
| `fuzz_fees_zero_rate_yields_zero_fee` | Zero rate → zero fee |
| `fuzz_fees_monotone_in_rate` | Fee increases with rate |
| `fuzz_total_fees_additivity` | `total_fees == sum(individual fees)` |
| `fuzz_gross_profit_sign_consistency` | Profit sign matches payout vs funded |
| `fuzz_net_profit_le_gross_profit` | `net_profit ≤ gross_profit` |
| `fuzz_roi_sign_matches_net_profit` | ROI sign matches net_profit sign |
| `fuzz_aggregate_revenue_internal_consistency` | `total_revenue == fees + penalties` |
| `fuzz_revenue_share_full_ownership` | 100% pool share → full revenue |
| `fuzz_revenue_share_proportional_split` | 50/50 split → ~half each |
| `fuzz_settlement_to_profit_pipeline` | Settlement output feeds profit correctly |
| `fuzz_fees_and_settlement_arithmetic_compatibility` | Fee modules use compatible arithmetic |

---

## Running Tests

```bash
# Run all fuzz tests (default: 200 cases per proptest test)
cargo test --features fuzz-tests fuzz_

# Extended run (1 000 cases)
PROPTEST_CASES=1000 cargo test --features fuzz-tests fuzz_

# Thorough run (10 000 cases)
PROPTEST_CASES=10000 cargo test --features fuzz-tests fuzz_

# Run a specific test
cargo test --features fuzz-tests fuzz_total_paid_never_exceeds_total_due

# Reproduce a failure from a saved seed
PROPTEST_SEED=<seed> cargo test --features fuzz-tests <test_name>
```

### Expected output

```
running 32 tests
test test_fuzz::fuzz_aggregate_revenue_internal_consistency ... ok
test test_fuzz::fuzz_fees_and_settlement_arithmetic_compatibility ... ok
...
test test_fuzz_invariants::fuzz_total_paid_never_exceeds_total_due ... ok
test test_fuzz_invariants::fuzz_valid_bid_settlement_no_investor_loss ... ok
test test_fuzz_invariants::fuzz_zero_bid_always_rejected ... ok

test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured
```

---

## Security Assumptions

- **Bounded arithmetic**: all operations use `checked_*`; overflow returns `None`.
- **Bounded iteration**: sweep sizes are O(N) where N ≤ 10 000 per test run.
- **Deterministic**: proptest seeds are fixed per-run; failures are reproducible.
- **Deny-by-default**: every guard returns `Err` unless the actor is `Verified`.
- **No silent wrapping**: `u128` with `overflow-checks = true` in release profile.

---

## CI Integration

```yaml
# .github/workflows/test.yml
- name: Run fuzz tests
  run: |
    PROPTEST_CASES=1000 cargo test --features fuzz-tests fuzz_
```

---

## Troubleshooting

If a proptest test fails:

1. Note the seed from the error output.
2. Reproduce: `PROPTEST_SEED=<seed> cargo test --features fuzz-tests <test_name>`
3. Fix the underlying invariant violation.
4. Re-run the full suite to confirm no regressions.

### Performance

| Cases | Approx. time |
|-------|-------------|
| 200 (default) | < 1 s |
| 1 000 | ~5 s |
| 10 000 | ~30 s |

---

## References

- [Proptest Documentation](https://docs.rs/proptest/)
- [Soroban Testing Guide](https://soroban.stellar.org/docs/how-to-guides/testing)
- [Rust Fuzz Book](https://rust-fuzz.github.io/book/)

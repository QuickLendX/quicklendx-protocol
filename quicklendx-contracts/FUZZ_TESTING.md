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

### 4. Payment capping and replay protection (Issue #1080)

**File:** `src/test_fuzz.rs`

Partial payments enforce strict capping and nonce-based replay protection under random
sequences, providing coverage beyond the deterministic unit tests in `test_partial_payments.rs`.

| Test | What it validates |
|------|-------------------|
| `fuzz_payment_capping_invariant` | Random payment sequences never exceed `total_due`; nonce uniqueness preserved |
| `fuzz_overpay_then_underpay_sequence` | Overpayment followed by underpayment caps correctly at settlement boundary |
| `fuzz_repeated_nonce_replay_protection` | Duplicate nonces are idempotent; state unchanged after replay |
| `fuzz_payment_count_exhaustion` | Payments rejected at `MAX_PAYMENT_COUNT` boundary (`1_000` limit) |

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


---

## libFuzzer targets (Issue #3051)

The standalone libFuzzer harnesses exercise the public bid-acceptance wrapper,
settlement query/attempt paths, and the shared overflow-safe pagination
primitives with bounded structured inputs. The contract-facing targets start
from an empty test environment, so missing-state rejections are expected; the
pagination target directly checks bounds, caps, and slice consistency. They
complement the deterministic and proptest coverage above.

### Build and run

Install the nightly toolchain and cargo-fuzz once:

```bash
rustup toolchain install nightly
cargo install cargo-fuzz
cd quicklendx-contracts/fuzz
cargo +nightly fuzz build
cargo +nightly fuzz run accept_bid_and_fund
cargo +nightly fuzz run settlement
cargo +nightly fuzz run paged_queries
```

Run a short smoke pass from `quicklendx-contracts` (30 seconds per target
by default; pass another positive integer to change the duration):

```bash
./scripts/fuzz-smoke.sh
./scripts/fuzz-smoke.sh 30
./scripts/fuzz-smoke.sh --help
```

Each target caps the number of operations per input to keep individual
executions bounded. Contract-level rejections for missing or invalid state are
expected; an unexpected host panic or violated pagination assertion is a
fuzzer finding.

### Corpus and crash handling

Cargo-fuzz stores target corpora and crash artifacts under
`fuzz/corpus/<target>` and `fuzz/artifacts/<target>`. These are local
development artifacts: do not commit generated corpora, crash dumps, or
minimized inputs unless a small regression fixture is intentionally added to
the test suite. Keep any committed corpus below 1 MB. Reproduce a crash with:

```bash
cargo +nightly fuzz run <target> fuzz/artifacts/<target>/<crash-file>
```

After fixing a finding, add a deterministic regression test, rerun the relevant
unit tests, rebuild all fuzz targets, and repeat the smoke run.

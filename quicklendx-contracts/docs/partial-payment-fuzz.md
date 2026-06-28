# Partial Payment Fuzz Harness

## Overview

Property-based fuzz tests for [`settlement::process_partial_payment`](../src/settlement.rs)
validate nonce/transaction_id replay protection, cumulative payment capping, and
monotonic payment-count accounting under interleaved valid and replayed payments.

The harness lives in
[`src/test_fuzz_partial_payment.rs`](../src/test_fuzz_partial_payment.rs)
and is compiled only with `--features fuzz-tests`.

---

## Run Command

```bash
# Fast smoke (10 cases — local dev / quick CI)
cargo test --features fuzz-tests test_fuzz_partial_payment_smoke

# Full acceptance (50,000 cases — CI target)
PROPTEST_CASES=50000 cargo test --features fuzz-tests test_fuzz_partial_payment

# Deterministic edge-case tests only (~seconds)
cargo test --features fuzz-tests partial_payment_zero partial_payment_exact partial_payment_after partial_payment_replay partial_payment_reordered
```

---

## API Under Test

`process_partial_payment(env, invoice_id, payment_amount, transaction_id)` delegates to
`record_payment`, which enforces:

| Parameter | Role |
|-----------|------|
| `invoice_id` | Invoice being paid |
| `payment_amount` | Requested payment (may be capped to remaining due) |
| `transaction_id` | Per-invoice nonce / idempotency key (replay protection) |

Empty `transaction_id` skips the replay table (caller must guarantee uniqueness).

---

## Fuzzing Strategy

### Input space

| Input | Strategy |
|-------|----------|
| `invoice_amount` | `i128 ∈ [100, 100_000]` |
| Sequence length | `1..40` actions |
| `ValidPayment` | `(amount ∈ [1, invoice_amount], tx_index ∈ [0, 20])` |
| `ReplaySame` | Re-submit an earlier `tx_index` with the first applied amount |
| `ReplayDifferentAmount` | Re-submit with a different amount (must not credit extra) |

### Action enum

- **`ValidPayment`**: fresh or first-seen `(transaction_id, amount)` pair.
- **`ReplaySame`**: idempotent resubmission — no double-credit.
- **`ReplayDifferentAmount`**: malicious replay with altered amount — still no extra credit.

### Oracle

`PartialPaymentOracle` is a reference model that mirrors production semantics:

1. Reject `amount <= 0` → `InvalidAmount`
2. Reject when invoice finalized → `InvalidStatus`
3. On unseen non-empty nonce: apply `min(amount, remaining_due)`, increment count
4. On seen nonce: return `Ok(())` with **no** state change (idempotent replay)

Each fuzz step asserts contract state matches the oracle.

### Reordering

After the forward interleaved sequence, the harness replays the same multiset of
actions with replay steps moved after first-seen payments (`reorder_replays_after_first_seen`).
Final `(total_paid, payment_count)` must match — transaction_id deduplication is
order-stable for replay steps.

---

## Invariants Tested

### (a) Cumulative cap (security)

```
total_paid <= invoice.amount   (always)
```

No sequence may over-credit the business past the invoice face value.
**Double-credit via nonce replay is a critical security failure** — replays must
never increase `total_paid` or `payment_count`.

### (b) Monotonic payment count

`payment_count` never decreases; it increments only when a new non-empty nonce is recorded.

### (c) Transaction_id / nonce deduplication

Duplicate `(invoice_id, transaction_id)` pairs are idempotent:

- `total_paid` unchanged
- `payment_count` unchanged
- Holds when replay steps are reordered after first-seen payments

### (d) Contract ↔ oracle agreement

Every action produces matching `Ok`/`Err` between the contract and the oracle.

---

## Deterministic Edge Cases

| Test | Coverage |
|------|----------|
| `partial_payment_zero_amount_rejected` | `amount == 0` → `InvalidAmount` |
| `partial_payment_exact_final_marks_paid` | Exact remaining payment → `Paid` |
| `partial_payment_after_finalization_rejected` | Post-settlement payment → `InvalidStatus` |
| `partial_payment_replay_no_double_credit` | Same `transaction_id` twice → single credit |
| `partial_payment_reordered_replays_match_forward` | Reordered replays → same totals |

---

## Security Note: Double-Credit Risk

If replay protection on `(invoice_id, transaction_id)` were bypassed, an attacker
could resubmit the same on-chain payment identifier with a higher `payment_amount`
and inflate `total_paid` without transferring additional funds — breaking escrow
accounting and enabling premature settlement.

The fuzz harness specifically interleaves `ReplayDifferentAmount` steps to ensure
replays never increase cumulative paid balance. Any regression that credits twice
for one nonce will fail the oracle comparison and be persisted to the regression file.

---

## Regression Tracking

Proptest persists failing seeds to:

```
proptest-regressions/partial_payment.txt
```

Commit this file so CI and all developers replay known failures before generating novel cases.

---

## Test Inventory

### Proptest (randomised)

| Test | Invariant |
|------|-----------|
| `test_fuzz_partial_payment` | Cap, count monotonicity, replay dedup, reordering stability |

### Deterministic (always run with `fuzz-tests`)

| Test | Invariant |
|------|-----------|
| `partial_payment_zero_amount_rejected` | Zero amount rejected |
| `partial_payment_exact_final_marks_paid` | Exact finalization |
| `partial_payment_after_finalization_rejected` | Post-final rejection |
| `partial_payment_replay_no_double_credit` | No double-credit |
| `partial_payment_reordered_replays_match_forward` | Reorder stability |

---

## References

- [`src/settlement.rs`](../src/settlement.rs) — `process_partial_payment`, `record_payment`
- [`src/test_partial_payments.rs`](../src/test_partial_payments.rs) — deterministic unit tests
- [`src/test_fuzz_partial_payment.rs`](../src/test_fuzz_partial_payment.rs) — this harness
- [`proptest-regressions/partial_payment.txt`](../proptest-regressions/partial_payment.txt) — persisted seeds

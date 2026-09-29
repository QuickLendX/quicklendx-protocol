# Issue #2464 — Repayment and profit distribution: atomic rollback

**QE-2026-08 · Quality/High · Area: financial correctness / accounting**

## Summary

This issue asked for a "production-grade guarantee" that repayment
allocation, fees, principal, and profit distribution stay deterministic
under normal, invalid, repeated, concurrent, and failure conditions, with
no partial writes or side effects surviving a failed operation.

Before changing anything, I traced `settlement::record_payment`,
`settlement::settle_invoice_internal`, and `defaults::handle_default` —
the three functions that actually move money or transition an invoice to a
terminal state — end to end. The base guarantee this issue is worried about
(a failed operation leaving *partial* writes/transfers behind) is largely
**already provided by Soroban's own transaction model**: a contract
invocation that returns `Err` or panics has its entire effect set —
storage writes, cross-contract token transfers, and emitted events —
discarded by the host. There is no way, within a single call to
`process_partial_payment`, `settle_invoice`, or `handle_default`, for a
later failure to leave an earlier successful transfer standing.

What *is* genuinely this crate's own responsibility, and where I found and
fixed two real, narrow gaps, is whether the **code's own ordering** matches
that guarantee — i.e. whether every failable check runs before any
effect, so a human reading the function (not just the Soroban runtime) can
see the checks-effects-interactions boundary — and whether the accounting
identity that guards fund disbursement is verified through more than one
independently-implemented path. Both are addressed below. The remaining,
larger part of this change is regression coverage proving the existing
invariants at the real contract-entrypoint boundary, since none of the
existing test suite specifically asserted "a rejected call leaves token
balances and storage untouched."

## What changed

### 1. `settle_invoice_internal` — finalize before effects, not after

`src/settlement.rs`. Previously, `mark_finalized(env, invoice_id)` was
called *after* the investor/fee transfers, right before the invoice/
investment status updates. All of the checks that can reject settlement
(payment-too-low, missing investor, fee-calculation failure, the
accounting-identity mismatch) already ran before any transfer — so this
was never a live double-spend path (both settlement entrypoints that reach
this function are wrapped in `reentrancy::with_payment_guard`, and Soroban
has no callback/hook mechanism during a plain token transfer that could
re-enter mid-call regardless). It was, however, checks-then-*some*-effects-
then-one-more-check-ish ordering that didn't match this crate's own stated
philosophy elsewhere (see `record_payment`'s and `handle_default`'s "only
after all finality checks pass" comments). `mark_finalized` now runs
immediately after the last check and before the first effect (escrow
auto-release), so the only way `is_finalized(invoice_id)` can ever read
`true` is once every precondition already held.

### 2. Defense-in-depth: `verify_no_dust` wired into the real disbursement path

`src/settlement.rs` / `src/profits.rs`. `settle_invoice_internal` already
asserted `investor_return.checked_add(platform_fee) == invoice.total_paid`
before disbursing. `profits::verify_no_dust` — an existing, independently
implemented, and already-tested function performing the same check via
`saturating_add` — was marked `#[allow(dead_code)]` and never called from
production code. It's now called as a second, independent assertion of the
same identity immediately after the existing check, and the
`#[allow(dead_code)]` is removed. This doesn't change behavior for any
valid input (both checks agree on every value that doesn't overflow
`i128`, and the pre-existing `checked_add`-based check already rejects the
only inputs where the two could disagree, at the overflow boundary) — it's
a belt-and-suspenders addition, not a new rejection surface.

### 3. `handle_default` — run the insurance check before any state mutation

`src/defaults.rs`. The active-insurance-at-settlement check
(`require_active_insurance_at_settlement`) previously ran *after* the
invoice had already been transitioned to `Defaulted`, its status indexes
updated, business/investor default-history counters incremented, and an
`invoice_expired` event emitted — all within the same function, all still
correctly reverted by Soroban if the check then failed, but visually out of
order with the function's own "checks before the guard, guard before
effects" pattern that already governs `check_and_set_default_guard`'s
position. The investment lookup and the check that depends on it now run
immediately after `ensure_default_transition_open`, before
`check_and_set_default_guard` and everything after it. The later block that
uses the investment (marking it `Defaulted`, processing insurance claims)
now reuses that same lookup instead of re-fetching and re-running an
already-passed check.

### 4. New regression coverage: `src/test_settlement_atomic_rollback.rs`

Ten tests, run through the actual contract client (not the internal
`settlement`/`defaults` functions directly), several asserting on real
token balances rather than just return values or struct fields:

- **Repeated**: a duplicated payment nonce is rejected
  (`DuplicateNonce`), and `total_paid`, `payment_count`, and the business's
  token balance are byte-for-byte unchanged from immediately before the
  retry.
- **Invalid**: a non-positive payment amount is rejected before any
  storage write.
- **Normal / boundary**: an overpayment request is capped to exactly
  `remaining_due`, never partially double-applied, and triggers automatic
  settlement when it reaches `total_due`.
- **Normal, at the real integration boundary**: after a full payment
  triggers automatic settlement, the exact token-balance deltas satisfy
  `business_paid_out == investor_received + platform_fee_received` against
  the live token contract, not just the values `settle_invoice` returns.
- **Repeated / failure**: a second settlement attempt on an
  already-finalized invoice is rejected, and no second disbursement
  reaches either the investor or the business's counterparty balance.
- **Invalid / rejected**: defaulting a never-funded (`Verified`, not
  `Funded`) invoice is rejected and its status is untouched.
- **Repeated**: defaulting an already-`Defaulted` invoice is rejected
  (`InvoiceAlreadyDefaulted`) and its status is untouched by the retry.
- **Failure boundary**: `handle_default` never moves any token balance —
  confirmed directly, since it's a pure bookkeeping/status transition with
  no transfer side effect for a rejected or repeated call to leave
  partially applied.

## Compatibility

No public signature, response shape, or documented error for any existing
entrypoint changes. `mark_finalized`'s new position and the insurance
check's new position in `handle_default` are internal reorderings within
functions whose observable *success* behavior (what gets written, what
events fire, what balances change) is identical to before — only the
*order* in which already-existing checks run relative to already-existing
effects changed, and only in the direction of running checks earlier. A
call that used to succeed still succeeds with the same outcome; a call that
used to fail still fails, with the same error, just without executing the
intervening effects it used to (harmlessly, since Soroban was already
discarding them). `verify_no_dust` being called from production code adds
no new rejections beyond what the pre-existing check already rejected. No
migration is needed — no storage layout, key, or schema changed.

## Failure behavior / no partial or unauthorized state

- Every fund-moving effect in `settle_invoice_internal` remains gated
  behind the reentrancy-guarded entrypoints (`settle_invoice`,
  `process_partial_payment`) in `lib.rs`; this change adds no new
  authorization surface and removes none.
- `record_payment` continues to perform every failable validation (frozen
  check, payable-status check, replay/nonce check, payment-count cap,
  remaining-due/overflow checks) before its first storage write — unchanged
  by this pass, and now explicitly covered by regression tests rather than
  only implicitly relied upon.
- `settle_invoice_internal` now marks an invoice finalized only once every
  check has passed, and `handle_default` now runs the insurance check only
  once every earlier check has passed — in both cases *before* any
  irreversible effect, matching what Soroban's transaction atomicity
  already guaranteed and making that guarantee visible in the code's own
  ordering rather than solely relying on the runtime to paper over it.

## Operational limitations

- This change does not add compensating/saga-style rollback logic for
  *cross-transaction* workflows (e.g. a caller submitting `settle_invoice`
  in one transaction and expecting to reconcile it against a separate
  transaction if something downstream, off-chain, later disagrees) — that
  is a materially different, much larger problem than this focused pass
  addresses, and nothing in the current codebase implements or claims
  multi-transaction sagas. Soroban's atomicity guarantee — and this
  change — apply within a single contract invocation.
- Notification failures (`NotificationSystem::notify_*`) are still
  intentionally swallowed (`let _ = ...`) in both `process_partial_payment`
  and `handle_default`, per those functions' existing, documented design:
  "Notification failures must not roll back funds." This change does not
  alter that — it's a deliberate best-effort side channel, not an
  accounting-relevant effect.

## Security assumptions

- Relies on Soroban's protocol-level guarantee that a failed transaction's
  effects (storage, transfers, events) are fully discarded. This is a
  platform guarantee, not something this contract can or needs to
  reimplement; the changes here are about the contract's own code matching
  that guarantee in its visible ordering, and about proving the resulting
  invariants with tests, not about substituting for it.
- No change to any authorization check (`require_auth`, admin/business/
  investor identity checks) anywhere in this diff.

## Validation

Commands intended to be run in `quicklendx-contracts/` (see the PR
description for actual output from this environment, including any
toolchain limitations encountered while running them):

```bash
cargo check --tests
cargo test --lib test_settlement_atomic_rollback
cargo test --lib   # full suite
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

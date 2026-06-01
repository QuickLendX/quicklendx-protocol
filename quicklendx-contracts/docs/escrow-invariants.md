# Escrow state-machine invariants

QuickLendX escrow moves through `Held -> Released | Refunded`. The property-based
model in `src/test_escrow_invariant_model.rs` drives randomized legal and illegal
public-transition attempts (accept, replay accept, release, refund, double refund,
refund-after-release, pause, and emergency-mode toggles) and checks the oracle
after every step.

## Formal invariants

1. **One Held escrow per invoice**: for a single invoice, the count of active
   `Held` escrows is never greater than one. Replays of `accept_bid` must be
   rejected once an escrow or investment exists.
2. **Terminal terminality**: once an escrow reaches `Released` or `Refunded`, no
   later action may change it back to `Held` or move it to the other terminal
   state.
3. **Conservation of principal**: total released plus total refunded movement is
   never greater than the original accepted principal, and exactly equals the
   principal when the escrow reaches a terminal state.
4. **Cross-module coherence**: invoice, investment, and escrow status must match:
   `Verified/None/None`, `Funded/Active/Held`, `Settled/Repaid/Released`, or
   `Refunded/Refunded/Refunded`.
5. **Mode gating**: pause and emergency modes reject mutating lifecycle calls and
   must not change balances or terminal statuses.

## Randomized execution counts

- Local developer runs execute 1,024 sequences by default for fast feedback.
- CI runs execute at least 10,000 sequences when the `CI` environment variable is
  present.
- Nightly/deep runs execute 1,000,000 sequences when
  `QUICKLENDX_NIGHTLY_INVARIANTS=1` is set.

Failing seeds persist to `proptest-regressions/escrow_invariant_model.txt`.

## Security note

This model is designed to catch double-spend escrow bugs, state replay bugs,
terminal-state regression bugs, and cross-module drift where an invoice or
investment says funds are active after escrow has already released or refunded
principal.

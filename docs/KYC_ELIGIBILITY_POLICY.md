# KYC eligibility policy

All non-terminal invoice, bid, funding, and settlement entrypoints use
`kyc_policy::authorize_before_side_effect`. The predicate is deliberately
small and deterministic: missing, pending, revoked, invalid-expiry, and
expired records have distinct errors; verified records are valid only while
`now < expires_at`. Equality at the expiry boundary is rejected.

The check runs before a bid is made visible or funds move. A terminal financial
record is immutable and is not retroactively invalidated when KYC changes; the
verification version still advances only for non-terminal records. This keeps
financial history stable while preventing future actions from using stale
verification.

Business and investor actors share the status/expiry predicate. Their action
authorization is applied after eligibility, so an invalid actor cannot turn a
pending or expired record into a generic authorization success.

## Migration and rollback

Call sites can migrate to the policy without changing stored record shape. Add
the module first, switch each entrypoint, then enable the boundary matrix in
CI. Rollback leaves records untouched and restores the prior caller behavior;
operators should still reject new actions while the old code is deployed if
KYC freshness is a safety requirement.

## Validation

The tests cover each status, exact expiry, before/after expiry, missing records,
both actor classes, all dependent actions, terminal immutability, version
replays, and invalid expiry. No KYC payload or identity is logged by the
policy.

## Decision table

| Record | Before expiry | At expiry | After expiry |
| --- | --- | --- | --- |
| missing | `Missing` | `Missing` | `Missing` |
| pending | `Pending` | `Pending` | `Pending` |
| rejected | `Revoked` | `Revoked` | `Revoked` |
| verified | allowed | `Expired` | `Expired` |
| verified with zero expiry | `InvalidExpiry` | `InvalidExpiry` | `InvalidExpiry` |

The table is shared by business and investor actors. Actor permissions are a
second decision after status eligibility: business actors create invoices,
investors submit bids and fund invoices, and settlement is treated as a
terminal financial operation. A caller cannot obtain a successful result by
choosing a different entrypoint or actor label.

## Security properties

- Expiry comparisons use integer ledger time and have no timezone conversion.
- Equality at expiry is denied, avoiding a one-ledger-tick race.
- Pending and revoked are not collapsed into a generic verified value.
- Verification version numbers must increase for non-terminal updates.
- Terminal outcomes are not recalculated after KYC changes.
- No policy function performs a storage write or transfers funds.
- Callers receive stable enum errors suitable for API mapping.
- Invalid zero expiry is rejected even if the status is pending.

## Entry-point integration rules

An entrypoint must load the actor's KYC record, call the policy, and only then
perform a state mutation, make a bid visible, or transfer funds. It must pass
the same ledger timestamp to the policy and to any audit record. It must not
cache a successful decision across a transaction boundary. If the operation is
already terminal, it must pass an explicit terminal flag and preserve the
existing financial record rather than deriving a new outcome from current KYC.

## Test evidence

The unit tests exercise the predicate directly. The matrix tests exercise the
five record states across all dependent actions and both actor types. Boundary
tests cover zero, one, exact expiry, one tick after expiry, and maximum time.
Version tests cover new, repeated, lower, and terminal updates. This keeps the
policy reviewable even while individual contract entrypoints evolve.

## Upgrade considerations

Adding a new KYC status requires a new stable error and an explicit row in the
decision table before any entrypoint accepts it. Adding a new KYC-dependent
operation requires a new `KycDependentAction` variant, actor mapping, and
matrix case. Storage migrations must preserve status, expiry, and version;
missing legacy rows intentionally fail closed as `Missing`.

The policy is the single source of truth for KYC-dependent actions.

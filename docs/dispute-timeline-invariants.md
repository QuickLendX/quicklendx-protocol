# Dispute Timeline Invariants

This document is the executable reference for the dispute timeline property
tests in `quicklendx-contracts/src/test_dispute_timeline_props.rs`.

## State Machine

| Action | Allowed current `dispute_status` | Next `dispute_status` | Timeline effect | Terminal |
|---|---|---|---|---|
| `create` | `None` | `Disputed` | Append `Opened` | No |
| `evidence` | `Disputed` | `Disputed` | No new timeline entry | No |
| `under_review` | `Disputed` | `UnderReview` | Append `UnderReview` | No |
| `resolve` | `UnderReview` | `Resolved` | Append `Resolved` | Yes |

Legal action grammar:

`create -> evidence* -> (under_review -> resolve?)?`

## Ordering Rules

- `Opened` is always the first visible timeline entry.
- `UnderReview` may appear at most once and only after `Opened`.
- `Resolved` may appear at most once and only after `UnderReview`.
- Evidence updates are allowed only while the dispute remains `Disputed`, and
  they never create a visible timeline row.
- `Resolved` is terminal. Any later `evidence`, `under_review`, or `resolve`
  action must be rejected deterministically.

## Timestamp Rules

- `Opened.timestamp` is the dispute creation ledger timestamp.
- `UnderReview.timestamp` is the exact ledger timestamp when the dispute enters
  review, persisted separately from the final resolution timestamp.
- `Resolved.timestamp` is the dispute resolution ledger timestamp.
- When accepted transitions occur at strictly increasing ledger timestamps, the
  timeline returned by `get_dispute_timeline` must also be strictly increasing.

## Duplicate Prevention

- The timeline must not contain duplicate lifecycle entries.
- Duplicate `create` attempts must be rejected with `DisputeAlreadyExists`.
- Duplicate `under_review` attempts after review has started must be rejected
  with `InvalidStatus`.
- Duplicate `resolve` attempts after final resolution must be rejected with
  `DisputeNotUnderReview`.

## Audit Trail Interplay

The dispute timeline is a user-facing summary and does not replace the append-only invoice audit trail.
Timeline redaction rules remain in force even when audit queries are available,
so evidence and privileged reviewer identity are not leaked through the
timeline endpoint.

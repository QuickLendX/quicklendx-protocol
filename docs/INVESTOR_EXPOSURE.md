# Atomic investor exposure caps

QuickLendX treats an investor cap as a reservation limit, not as a lifetime
volume limit. Every new bid must fit beside the investor's pending bids and
funded active positions. The check and the reservation happen inside one
Soroban invocation, so the ledger's normal transaction serialization provides
the atomic read-check-write boundary.

## What counts as exposure

Current exposure is the sum of two authoritative state sets:

1. `BidStatus::Placed` bids that have not reached their expiration timestamp.
2. `InvestmentStatus::Active` investments in the active-investment index.

The proposed amount is added to that sum and must be no greater than the
investor's configured `investment_limit`.

```text
current_exposure = active_pending_bid_amounts
                 + active_investment_principal

new_exposure = current_exposure + proposed_bid_amount
require new_exposure <= investment_limit
```

`InvestorVerification::total_invested` remains a historical analytics counter.
It is used for risk and tier calculations but never consumes current capacity.
Otherwise a completed or defaulted position would permanently reduce the
investor's available limit and make the cap depend on history rather than
outstanding risk.

## Atomic placement path

`place_bid` performs validation before it writes the idempotency marker or bid:

```text
pause / identity / authentication
        |
invoice and bid validation
        |
read active bid + active investment indexes
        |
checked exposure comparison
        |
create bid, index it, and store idempotency marker
```

Soroban transactions execute serially against a ledger snapshot. A second
transaction therefore observes the first transaction's newly indexed bid or
investment before its own capacity check. If the comparison fails, the
transaction returns an error before any reservation or idempotency state is
written. The existing idempotency key remains a separate duplicate-submission
guard and is not used as an exposure counter.

## Lifecycle release

Pending bids release capacity when they become `Cancelled`, `Withdrawn`,
`Expired`, or `Accepted`. Funded positions release capacity when their
investment changes from `Active` to one of the terminal states:

| Terminal state | Exposure effect | Historical record |
| --- | --- | --- |
| `Completed` | Removes principal from active sum | Preserved |
| `Defaulted` | Removes principal from active sum | Preserved |
| `Refunded` | Removes principal from active sum | Preserved |
| `Withdrawn` | Removes principal from active sum | Preserved |

The active-investment index is maintained by `InvestmentStorage::update_investment`.
The exposure query scans that index and includes only records whose status is
`Active` and whose investor matches. This makes release behavior derive from
the same state transition that governs the position, avoiding a second mutable
counter that could drift if a lifecycle path is added later.

## Arithmetic and fail-closed behavior

Exposure amounts are positive `i128` principal values. The active investment
sum uses checked addition. A malformed non-positive active record, a missing
record referenced by the active index, or an addition overflow returns
`i128::MAX`, making the subsequent capacity check reject new exposure instead
of wrapping to a small value or silently creating capacity.

The bid-side sum retains the existing saturating arithmetic defense. Combining
the two sums and the proposed amount also saturates. Saturation can only make a
request harder to accept; it cannot create capacity from an overflow.

## Compatibility

No storage migration is required. Existing investor verification records retain
their serialized shape and `total_invested` semantics. The new calculation reads
the existing bid and active-investment indexes. The public read method
`get_investor_active_exposure` exposes the same combined value used to explain
capacity decisions to operators and clients.

The failure category remains `InvalidAmount`, preserving the existing ABI error
surface where capacity failures were already reported through that variant.
The dedicated per-invoice whale cap and the maximum-active-bids count remain
independent protections; a request must satisfy all three controls.

## Validation matrix

The focused regression suite covers:

| Case | Guarantee |
| --- | --- |
| Empty active index | Full configured limit is available |
| Multiple active investments | Matching investor amounts sum exactly |
| Other investor | No cross-account capacity leakage |
| Exact cap | Final amount is accepted |
| One unit above cap | Rejected before state mutation |
| Pending bid plus active investment | Both reservations share one ceiling |
| Completed/defaulted/refunded/withdrawn | Each terminal transition releases once |
| Repeated terminal transition | History remains while exposure stays zero |
| Non-positive active amount | Fails closed |
| Integer overflow | Fails closed |
| Lifetime analytics | Does not consume current capacity |
| Many mixed transitions | Active index sum remains deterministic |

These tests are intentionally independent of wall-clock timing except for the
existing bid expiration behavior. They operate on the storage helpers directly
where useful, then exercise the same validator called by the contract entry
point. This keeps the arithmetic and lifecycle guarantees easy to audit.

## Operational notes

The calculation is bounded by the number of indexed active bids and active
investments for the investor. Both indexes already exist for lifecycle and
query operations. Deployments should monitor index integrity using the
existing invariant self-check before changing exposure limits.

If an upgrade introduces a new position status, it must classify that status as
active or terminal and add a transition test. It must not alter only
`total_invested` or introduce a second exposure counter without updating this
document and the invariant suite.

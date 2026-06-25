# Escrow

> Audience: **operators and downstream integrators**. For the full
> entrypoint/error reference used by contract contributors, see
> [quicklendx-contracts/docs/contracts/escrow.md](../quicklendx-contracts/docs/contracts/escrow.md).

## What this document covers

When an investor's bid is accepted on an invoice, their funds move into
**escrow** — locked inside the contract — until one of three terminal
outcomes occurs: the funds are **released** to the business, **refunded** to
the investor, or the investor **withdraws** before release. This document
explains the lock window and the conditions under which each outcome happens,
so you can reason about fund safety without reading the contract source.

## The lock window

Funds enter escrow the moment `accept_bid_and_fund` succeeds, and they stay
locked (`EscrowStatus::Held`) until exactly one of these three calls succeeds:

| Call | Who can call it | Funds move to | Resulting status |
|---|---|---|---|
| `release_escrow_funds` | Admin (via invoice verification) | Business | `Released` (terminal) |
| `refund_escrow_funds` | Admin or the invoice's business owner | Investor | `Refunded` (terminal) |
| `withdraw_investment` | The investor themself | Investor | `Refunded` (terminal) |

There is no time-based auto-expiry on the lock — an escrow stays `Held`
indefinitely until one of the above is explicitly called. Once an escrow
reaches `Released` or `Refunded`, it cannot transition again: any further
attempt to release or refund it is rejected.

## Release conditions

`release_escrow_funds` succeeds only when:

1. The invoice is in `Funded` status (i.e. it has gone through
   `accept_bid_and_fund` and not already settled or refunded).
2. The escrow record for that invoice is `Held`.
3. The contract holds enough of the escrowed token to cover the transfer.

If all three hold, funds move from the contract to the business address and
the escrow becomes `Released`. This is the path used when an invoice is
verified as repaid.

## Refund conditions

Two different calls can return funds to the investor, with different
authorization rules:

- **`refund_escrow_funds`** — callable by the contract admin or the invoice's
  business owner. Used for cancellations or dispute resolution. Requires the
  invoice to be `Funded` and the escrow to be `Held`.
- **`withdraw_investment`** — callable only by the investor who funded the
  bid. Used when the investor wants to pull out before the invoice is
  released. Requires the investment to be `Active`, the escrow to be `Held`,
  and the invoice to be `Funded`; on success the invoice reverts to
  `Verified` (as if it had never been funded) rather than moving to a
  `Refunded` state.

In both cases, the refund authorization matrix and idempotency guarantees are
detailed in [docs/contracts/escrow-refund.md](./contracts/escrow-refund.md).

## Worked example

```text
1. investor calls accept_bid_and_fund(invoice_id, bid_id)
   -> tokens locked: investor -> contract
   -> EscrowStatus::Held

2a. business repays off-chain / invoice gets verified as repaid
    -> admin (via verify_invoice) triggers release_escrow_funds(invoice_id)
    -> tokens move: contract -> business
    -> EscrowStatus::Released   (terminal)

   -- OR --

2b. dispute or cancellation
    -> admin or business calls refund_escrow_funds(invoice_id, caller)
    -> tokens move: contract -> investor
    -> EscrowStatus::Refunded   (terminal)

   -- OR --

2c. investor changes their mind before release
    -> investor calls withdraw_investment(invoice_id, investor)
    -> tokens move: contract -> investor
    -> EscrowStatus::Refunded   (terminal)
    -> invoice reverts to Verified
```

All three terminal paths are mutually exclusive — once 2a, 2b, or 2c
succeeds, the other two are rejected with `InvalidStatus` because the escrow
is no longer `Held`.

## Safety properties at a glance

- **Reentrancy-safe** — every entrypoint above is wrapped in a reentrancy
  guard; concurrent calls on the same invoice are rejected, not interleaved.
- **One escrow per invoice, for life** — a second `accept_bid_and_fund` on
  the same invoice can never create a second escrow record, even after the
  first one has been released or refunded.
- **Transfer-then-write** — the contract never marks an escrow `Released` or
  `Refunded` unless the underlying token transfer already succeeded, so a
  failed transfer leaves the escrow safely retryable in its prior state.

For the full state-machine diagram, error codes, and call graphs, see
[quicklendx-contracts/docs/contracts/escrow.md](../quicklendx-contracts/docs/contracts/escrow.md).
For the formal invariants checked by the property-based test model, see
[quicklendx-contracts/docs/escrow-invariants.md](../quicklendx-contracts/docs/escrow-invariants.md).
For token-transfer error handling and query-surface details, see
[docs/contracts/escrow.md](./contracts/escrow.md).

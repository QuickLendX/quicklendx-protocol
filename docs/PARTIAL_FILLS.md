# Partial Fill Mechanics

> Audience: contributors and downstream integrators who need to know how QuickLendX treats a request that does not fully cover the invoice amount.

QuickLendX does not maintain a separate bid-level “partial fill” state machine. A bid is accepted and funded atomically through the invoice funding flow; there is no later contract path that partially consumes a bid amount in-place.

The behavior that people often call “partial fill” is the invoice settlement path: when a business calls `process_partial_payment`, the contract records a payment against the funded invoice and only applies the portion that is still due.

In practice, that means:

- a payment request is never applied beyond the invoice’s `remaining_due`
- duplicate transaction IDs are treated as idempotent replays
- once the accumulated `total_paid` reaches the invoice amount, settlement is triggered automatically

## The rule in one sentence

For a payment request on a funded invoice, `applied_amount = min(requested_amount, remaining_due)`.

## Real entrypoint

The public entrypoint is:

```rust
pub fn process_partial_payment(
    env: &Env,
    invoice_id: &BytesN<32>,
    payment_amount: i128,
    transaction_id: String,
) -> Result<(), QuickLendXError>
```

The normal call pattern in tests is:

```rust
client.process_partial_payment(
    &invoice_id,
    &400,
    &String::from_str(&env, "pay-1"),
);
```

## Concrete example

Assume:

- invoice amount = `1_000`
- current `total_paid` = `0`
- the invoice is already `Funded`

A first call:

```rust
client.process_partial_payment(&invoice_id, &400, &String::from_str(&env, "pay-1"));
```

results in:

- `applied_amount = 400`
- `total_paid = 400`
- `remaining_due = 600`
- invoice status remains `Funded`

A second call:

```rust
client.process_partial_payment(&invoice_id, &800, &String::from_str(&env, "pay-2"));
```

The contract does not “over-apply” the request. It caps the payment to the outstanding balance:

- `remaining_due` before the call = `600`
- `requested_amount` = `800`
- `applied_amount = min(800, 600) = 600`
- `total_paid` becomes `1_000`
- `remaining_due` becomes `0`

At that point, the settlement path is triggered automatically and the invoice moves to its terminal paid state.

## What is preserved

The contract maintains three invariants for this flow:

1. `total_paid` never exceeds the invoice amount.
2. `record_payment` is idempotent for the same `(invoice_id, nonce)` pair.
3. a transaction that would exceed the remaining balance is silently capped rather than rejected.

That last rule is the key part of the “partial fill” behavior: the system records the payment that fits, and it does not leave the invoice in a half-processed state.

## Replay behavior

The nonce is the caller’s deduplication key for a payment attempt.

If the same `transaction_id` is re-used on the same invoice:

- the contract returns the current invoice progress
- no new payment record is appended
- `total_paid` does not increase a second time

This is important for downstream integrators because a retried payment request should behave like a harmless replay, not as a second partial match.

## Why this matters operationally

For contributors and support staff, the practical interpretation is:

- a “partial fill” of the invoice amount is not a separate state machine
- the payment request is capped to the remaining balance
- one payment attempt can finish the invoice if it reaches the final amount exactly
- the same nonce can be safely replayed without corrupting accounting

## Related reading

- [docs/SETTLEMENT.md](SETTLEMENT.md)
- [quicklendx-contracts/src/settlement.rs](../quicklendx-contracts/src/settlement.rs)
- [quicklendx-contracts/src/test_partial_payments.rs](../quicklendx-contracts/src/test_partial_payments.rs)

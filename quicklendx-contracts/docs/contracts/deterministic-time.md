# Deterministic Ledger Time

This note is for smart contract contributors and reviewers.
It explains what counts as deterministic time in QuickLendX contract logic and why
`env.ledger().timestamp()` is the only source of time that smart contracts can trust.

## What deterministic time means

In Soroban contracts, deterministic time means a timestamp value that is:

- the same for every instance of contract execution in a given ledger,
- produced by the blockchain host, and
- independent of any local machine clock or request-delivery timing.

For QuickLendX, the deterministic value is:

- `env.ledger().timestamp()` — the ledger close timestamp from the Soroban host.

This is not a wall-clock timestamp from a user's browser, backend service, or
API gateway.

## What counts as deterministic time

`env.ledger().timestamp()` is the only deterministic time source available inside
contract code. It is safe to use for:

- deadline checks, e.g. `now > invoice.due_date`
- age-based expiration policies
- storage TTL logic and invariants
- event timestamps emitted with contract state changes

Example:

```rust
use soroban_sdk::{contractimpl, Env};

pub struct Contract;

#[contractimpl]
impl Contract {
    pub fn is_invoice_due(env: Env, due_date: u64) -> bool {
        env.ledger().timestamp() > due_date
    }
}
```

In this example, `env.ledger().timestamp()` is the only correct source of time
for determining whether an invoice is due.

## What does not count as deterministic time

The following are not deterministic and must not be trusted for contract control
flow or security-sensitive comparisons:

- off-chain wall-clock time from the backend or client
- the time the API request arrived at an indexer or gateway
- a client-supplied `timestamp` field in transaction data
- host system time on a validator node

Even though an API server or user interface may display a wall-clock time,
contract logic must always use `env.ledger().timestamp()` when making
consensus-critical decisions.

## Why the distinction matters

Soroban contract execution must be replayable and identical across validators.
Off-chain clocks are not synchronized in the same deterministic way, so they
cannot be used inside contract logic.

If contract state changes depend on a non-deterministic timestamp, different
validators could reach different results for the same transaction, which would
break consensus.

## Practical guidance

- Use `env.ledger().timestamp()` for all deadline, expiration, and freshness checks.
- Do not accept an externally provided timestamp as proof that the current time
  is after some deadline.
- Treat `env.ledger().timestamp()` as the canonical on-chain time source.

## Related documentation

- See [Data Freshness Semantics](freshness.md) for how ledger time is used to
  compute index lag and freshness metadata.

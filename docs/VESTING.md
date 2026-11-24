# Vesting Model

This document explains the time-locked vesting model implemented in the `quicklendx-contracts` module. It is intended for protocol operators and integrators who need to understand how tokens are locked, released, and managed over time.

## 1. Vesting Model

The vesting mechanism locks protocol tokens or rewards in the contract and releases them linearly over time, following an optional cliff period. 

### Core Mechanics
- **Linear Release:** Between the `start_time` and `end_time`, tokens unlock at a constant rate per second.
- **Cliff:** If a `cliff_time` is configured, no tokens can be released until this timestamp is reached. Once the cliff is passed, the accumulated vested amount becomes immediately claimable.
- **Custody:** When an admin creates a schedule, the full `total_amount` of tokens is immediately transferred from the admin to the contract's custody.

### Concrete Example

When a schedule is created, it is stored on-chain with the following structure. For example, a 1-year vesting schedule with a 3-month cliff:

```rust
VestingSchedule {
    id: 1,
    token: Address("CDLZFC..."),           // The token being vested
    beneficiary: Address("GBB3M2..."),     // The account receiving the tokens
    total_amount: 120_000_000_000,         // Total amount in stroops (12,000 units)
    released_amount: 0,                    // Tracks tokens already claimed
    start_time: 1704067200,                // Jan 1, 2024
    cliff_time: 1711929600,                // Apr 1, 2024 (3 months)
    end_time: 1735689600,                  // Jan 1, 2025
    created_at: 1704067000,
    created_by: Address("GADMIN..."),
}
```

The beneficiary can invoke `release_vested_tokens` (alias `release_vesting`) at any point after the cliff. The contract computes the releasable amount via `get_vesting_releasable` (alias `get_releasable_vesting`) based on the current ledger timestamp.

## 2. Edge Cases and Protections

The contract handles several edge cases to guarantee safe execution:

- **Integer Truncation:** The formula for vested amount is `total_amount * elapsed / duration`. Because integer division truncates toward zero, the beneficiary may receive up to 1 unit *less* than a fractional real-valued curve would suggest until the next second boundary. 
- **Rounding Dust Elimination:** The final release at `end_time` unconditionally delivers the exact remaining `total_amount`, ensuring no "dust" is trapped in the contract due to accumulated rounding errors.
- **Idempotency:** If a beneficiary calls `release` when no new tokens have vested, the contract returns `0` rather than trapping or panicking. This ensures batch-release bots or automated scripts do not fail unnecessarily.
- **Pre-cliff Calls:** Calling `release` before the `cliff_time` explicitly returns an `InvalidTimestamp` error to help distinguish "too early" from "nothing currently available".

## 3. Admin Overrides (Irrevocability)

By design, **vesting schedules are irrevocable once created.** 

There is no `revoke`, `cancel`, or `clawback` function available to the admin in the `vesting.rs` module. Once the admin funds a schedule, the tokens belong to the beneficiary according to the time-lock curve.

**Emergency Fallback:** In the event of an extreme protocol vulnerability or stuck funds, the protocol-wide `initiate_emergency_withdraw` (followed by its timelock) is the only mechanism an admin could theoretically use to move funds out of the contract. However, this is a global circuit breaker, not a vesting-specific override.

The emergency solvency check reads the per-token outstanding vesting reserve
(`VestingStorage::get_outstanding_vesting`) and subtracts it from the
withdrawable balance, so custodied vesting tokens can never be swept as surplus.

## 4. Contract Entrypoints

The following functions are exposed on the contract (see `src/lib.rs`) and are
callable through the generated client:

| Function | Authorization | Description |
| --- | --- | --- |
| `create_vesting_schedule(admin, token, beneficiary, total_amount, start_time, cliff_seconds, end_time)` | Admin | Creates a schedule and moves `total_amount` of `token` into contract custody. Returns the new schedule id. |
| `get_vesting_schedule(id)` | None (read-only) | Returns the `VestingSchedule` for `id`, or `None`. |
| `get_vesting_summary(user)` | None (read-only) | Aggregated `VestingSummary` across all of `user`'s schedules. |
| `get_vesting_vested(id)` | None (read-only) | Total vested amount for `id` at the current ledger time. |
| `get_vesting_releasable(id)` / `get_releasable_vesting(id)` | None (read-only) | Amount currently releasable for `id`. |
| `release_vested_tokens(beneficiary, id)` / `release_vesting(beneficiary, id)` | Beneficiary | Transfers the releasable amount to the beneficiary and returns it. |

Token custody movements use `payments::transfer_funds`.

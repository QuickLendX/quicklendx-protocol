# Vesting Contract Documentation

## Overview

The vesting module implements time-locked token release schedules for protocol tokens or rewards. Beneficiaries can claim vested tokens as they unlock after an optional cliff period.

## Vesting Schedule Structure

| Field | Type | Description |
|-------|------|-------------|
| `id` | u64 | Unique schedule identifier |
| `token` | Address | Token contract address |
| `beneficiary` | Address | Recipient of vested tokens |
| `total_amount` | i128 | Total tokens to vest |
| `released_amount` | i128 | Tokens already released |
| `start_time` | u64 | Unix timestamp when vesting starts |
| `cliff_time` | u64 | Unix timestamp when cliff ends (release begins) |
| `end_time` | u64 | Unix timestamp when all tokens vested |
| `created_at` | u64 | Ledger timestamp of creation |
| `created_by` | Address | Admin who created the schedule |

## Vesting Timeline

```
start_time                    cliff_time                      end_time
    |                              |                               |
    |         cliff period         |       vesting period          |
    |<---------------------------->|<------------------------------->|
    |         (locked)             |        (releases)             |
                                    
    0%                          ~25%                            100%
                               vested                          vested
```

## Cliff Boundary Behavior

### Before Cliff (`now < cliff_time`)
- `vested_amount = 0`
- `releasable_amount = 0`
- Release attempts fail

### At Cliff (`now == cliff_time`)
- Vested amount calculated based on elapsed time from `start_time`
- `vested_amount = total_amount * (cliff_time - start_time) / (end_time - start_time)`
- Release succeeds with vested amount

### After Cliff, Before End (`cliff_time < now < end_time`)
- Linear vesting from cliff to end
- `vested_amount = total_amount * (now - start_time) / (end_time - start_time)`
- Only new unvested amount is releasable

### At End Time (`now >= end_time`)
- `vested_amount = total_amount`
- `releasable_amount = total_amount - released_amount`

## Off-by-One Timestamp Handling

The implementation uses inclusive/exclusive boundaries correctly:

| Timestamp | Vested | Releasable |
|-----------|--------|------------|
| `cliff_time - 1` | 0 | 0 |
| `cliff_time` | > 0 | > 0 |
| `end_time - 1` | < total | < total |
| `end_time` | total | remaining |
| `end_time + N` | total | remaining |

## Security Considerations

1. **Admin authorization**: Schedule creation requires admin auth; non-admin callers are rejected with `NotAdmin`
2. **Beneficiary authorization**: Release requires beneficiary auth; non-beneficiary callers are rejected with `Unauthorized`
3. **Cliff enforcement**: `release()` returns `InvalidTimestamp` (not a silent no-op) when called before `cliff_time`, so callers can distinguish "too early" from "fully released"
4. **No over-release**: `released_amount` is tracked and validated after every release; overflow uses checked arithmetic
5. **Overflow protection**: `checked_mul`, `checked_add`, `checked_sub` used throughout; overflow returns `InvalidAmount`
6. **Timestamp validation**: `end_time > start_time` and `cliff_time < end_time` enforced at creation; backdated `start_time` rejected
7. **State invariant re-check**: `validate_schedule_state` re-validates stored schedule before every arithmetic operation

## Admin Threat Model

### Admin Powers
The protocol admin is the only address that can create vesting schedules. Specifically, admin can:
- Lock any amount of any token into a new schedule for any beneficiary
- Transfer the admin role to a new address (after which the old address loses all admin powers)

### Threat Scenarios

| Threat | Mitigation |
|--------|-----------|
| Non-admin creates a schedule | `require_auth` + `require_admin` gate; rejected with `NotAdmin` |
| Admin creates zero-amount schedule | `total_amount <= 0` check; rejected with `InvalidAmount` |
| Admin backdates `start_time` | `start_time < now` check; rejected with `InvalidTimestamp` |
| Admin sets `end_time <= start_time` | Explicit check; rejected with `InvalidTimestamp` |
| Admin sets `cliff_time >= end_time` (degenerate) | `cliff_time >= end_time` check; rejected with `InvalidTimestamp` |
| Old admin retains power after role transfer | `require_admin` reads live admin key; old address fails after transfer |
| Beneficiary releases before cliff | `release()` returns `InvalidTimestamp`; no state mutation occurs |
| Beneficiary double-releases | `released_amount` tracking; second call returns `Ok(0)` |
| Beneficiary releases more than total | Post-release `validate_schedule_state` catches `released_amount > total_amount` |
| Non-beneficiary releases tokens | `beneficiary` field compared to caller; rejected with `Unauthorized` |

### Not Mitigated
- **Compromised admin key**: A stolen key can create arbitrary schedules. Mitigate at the key-management layer (multisig, hardware wallet).
- **Consensus-level time manipulation**: Ledger timestamp is trusted; extreme validator collusion could affect cliff/end boundaries.
- **Token contract bugs**: `transfer_funds` delegates to the token contract; a malicious token can re-enter or fail silently.

## Time Boundaries Table

| Phase | Condition | Vested Amount | Releasable |
|-------|-----------|---------------|------------|
| Before cliff | `now < cliff_time` | 0 | 0 |
| At cliff | `now == cliff_time` | `total * cliff_duration / total_duration` | > 0 |
| After cliff | `cliff_time < now < end_time` | `total * elapsed / duration` | vested - released |
| At end | `now >= end_time` | total | total - released |

## API Reference

### `create_vesting_schedule`

```rust
pub fn create_schedule(
    admin: &Address,
    token: Address,
    beneficiary: Address,
    total_amount: i128,
    start_time: u64,
    cliff_seconds: u64,
    end_time: u64,
) -> Result<u64, QuickLendXError>
```

Creates a new vesting schedule. Transfers `total_amount` of `token` from admin to contract.

**Validation:**
- `total_amount > 0`
- `end_time > start_time`
- `cliff_time <= end_time` (where `cliff_time = start_time + cliff_seconds`)

### `get_vesting_schedule`

```rust
pub fn get_schedule(env: &Env, id: u64) -> Option<VestingSchedule>
```

Returns the vesting schedule by ID, if exists.

### `get_vesting_vested`

```rust
pub fn get_vesting_vested(env: Env, id: u64) -> Option<i128>
```

Calculates total vested amount at current time using linear vesting from `start_time`.
Returns `None` if the schedule does not exist or the stored state is invalid.

### `get_vesting_releasable`

```rust
pub fn releasable_amount(env: &Env, schedule_id: u64) -> Result<i128, QuickLendXError>
```

Returns amount available for release: `max(vested - released, 0)`.

### `release_vested_tokens`

```rust
pub fn release_vested_tokens(env: Env, beneficiary: Address, id: u64) -> Result<i128, QuickLendXError>
```

Transfers releasable tokens to beneficiary. Updates `released_amount`.

- Returns `InvalidTimestamp` if called before `cliff_time` (not a silent no-op).
- Returns `Ok(0)` if called after full release (idempotent).
- Returns `Unauthorized` if caller is not the schedule beneficiary.

## Over-Release Safety

The contract guarantees that the cumulative amount released to a beneficiary
can **never exceed `total_amount`**, regardless of how many times `release` is
called or how far past `end_time` the ledger advances.

The protection is layered:

1. **`releasable_amount` is bounded** — it returns `vested_amount - released_amount`.
   Because `vested_amount` is capped at `total_amount` (the `now >= end_time`
   branch), the releasable value is always ≤ `total_amount - released_amount`.
2. **`released_amount` is updated with `checked_add`** — overflow returns
   `InvalidAmount` rather than wrapping.
3. **`validate_schedule_state` re-runs after every release** — it asserts
   `released_amount <= total_amount`; any state corruption is caught before the
   updated schedule is persisted.
4. **Idempotent post-full-vest behaviour** — once `released_amount == total_amount`
   the releasable value is 0 and `release()` returns `Ok(0)` without mutating
   state, so repeated calls are safe.

## Testing

Run vesting tests:

```bash
cargo test test_vesting --lib
```

### Test Coverage

#### Cliff boundary
- Before cliff: 0 releasable; `release()` returns `InvalidTimestamp`
- At cliff: positive releasable
- After cliff, before end: partial release
- At end time: full amount
- After end time: full amount
- Zero cliff edge case
- Off-by-one timestamp boundaries

#### Monotonicity
- `vested_amount` is non-decreasing across a sequence of timestamps spanning
  pre-cliff, cliff, mid-vest, end, and post-end (`test_vested_amount_is_monotonic_across_schedule`)
- `releasable_amount` (before any release) is non-decreasing over time
  (`test_releasable_amount_is_monotonic_before_any_release`)

#### Over-release protection
- Releasing at every checkpoint: cumulative sum never exceeds `total_amount`
  (`test_sum_of_releases_never_exceeds_total`)
- Double release after full vest returns 0; `released_amount` stays at `total`
  (`test_double_release_after_full_vest_is_zero`)
- Release at `start_time` (before cliff) yields 0 releasable
  (`test_release_at_start_time_yields_zero_releasable`)

#### Timestamp overflow safety
- Schedule with timestamps near 10¹² seconds computes correct vested amount
  without overflow (`test_timestamp_near_max_does_not_overflow`)
- Large `total_amount` (near token mint cap) does not overflow `checked_mul`
  (`test_large_amount_vesting_arithmetic_does_not_overflow`)

#### Other edge cases
- Multiple partial releases tracking
- Integer division rounding (truncation)
- Release idempotency
- `vested_amount` and `releasable_amount` always non-negative

#### Admin boundary
- Non-admin rejected
- Zero amount rejected
- Backdated start rejected
- `end_time <= start_time` rejected
- `cliff_time >= end_time` rejected
- Old admin loses power after role transfer
- Non-beneficiary release rejected
- Querying non-existent schedule returns `None`

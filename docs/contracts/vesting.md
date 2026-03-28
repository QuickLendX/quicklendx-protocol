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

1. **Admin authorization**: Schedule creation requires admin auth
2. **Beneficiary authorization**: Release requires beneficiary auth
3. **No over-release**: `released_amount` tracked to prevent double-spending
4. **Overflow protection**: Checked arithmetic for calculations
5. **Timestamp validation**: `end_time > start_time` enforced

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
pub fn vested_amount(env: &Env, schedule_id: u64) -> Result<i128, QuickLendXError>
```

Calculates total vested amount at current time using linear vesting from `start_time`.

### `get_vesting_releasable`

```rust
pub fn releasable_amount(env: &Env, schedule_id: u64) -> Result<i128, QuickLendXError>
```

Returns amount available for release: `max(vested - released, 0)`.

### `release_vested_tokens`

```rust
pub fn release(env: &Env, beneficiary: &Address, id: u64) -> Result<i128, QuickLendXError>
```

Transfers releasable tokens to beneficiary. Updates `released_amount`.

## Testing

Run vesting tests:

```bash
cargo test vesting --lib
```

### Test Coverage

- Before cliff: 0 releasable
- At cliff: positive releasable
- After cliff, before end: partial release
- At end time: full amount
- After end time: full amount
- Zero cliff edge case
- Off-by-one timestamp boundaries
- Multiple partial releases
- Integer division rounding

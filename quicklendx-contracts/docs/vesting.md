# Vesting Module

Token vesting engine for QuickLendX. Supports admin-created schedules that lock protocol tokens in the contract and release them linearly over time after an optional cliff. Beneficiaries call `release_vesting` to claim vested tokens as they unlock.

---

## Cliff / linear slope semantics

```
vested(t) = 0                                           if t < cliff_time
          = total_amount                                if t >= end_time
          = total_amount × (t − start_time)            otherwise
                         / (end_time − start_time)
```

- **`start_time`** — Unix timestamp when the linear curve begins accruing.
- **`cliff_time` = `start_time + cliff_seconds`** — No tokens are releasable before this point. At exactly `cliff_time` the full elapsed-proportion is immediately claimable.
- **`end_time`** — All `total_amount` tokens are vested from this point onward.
- **Integer division truncates** (rounds toward zero). The final release at `end_time` always delivers the exact `total_amount`, eliminating accumulated rounding dust.

`cliff_time` must satisfy `start_time <= cliff_time < end_time`. A `cliff_seconds` of `0` means the cliff coincides with `start_time` and tokens start vesting immediately.

---

## Public entrypoints

### `create_vesting_schedule`

```rust
fn create_vesting_schedule(
    admin: Address,
    token: Address,
    beneficiary: Address,
    total_amount: i128,
    start_time: u64,
    cliff_seconds: u64,
    end_time: u64,
) -> Result<u64, QuickLendXError>
```

Creates a new vesting schedule. `total_amount` of `token` is transferred from `admin`'s balance into contract custody at creation time.

**Auth:** Admin (`AdminStorage::require_admin`).

**Guard:** Payment reentrancy guard (token transfer at entry).

**Returns:** Unique `schedule_id` (monotonically increasing `u64`).

**Errors:**
| Error | Condition |
|---|---|
| `InvalidAmount` | `total_amount <= 0` |
| `InvalidTimestamp` | `start_time < now`, `end_time <= start_time`, or `cliff_time >= end_time` |
| `NotAdmin` | Caller is not the current admin |

---

### `get_vesting_schedule`

```rust
fn get_vesting_schedule(id: u64) -> Option<VestingSchedule>
```

Returns the full `VestingSchedule` struct for `id`, or `None` if it does not exist. Read-only; no auth required.

---

### `get_vesting_vested`

```rust
fn get_vesting_vested(id: u64) -> Option<i128>
```

Returns the total vested amount at the **current ledger timestamp**. Returns `None` if `id` does not exist. Read-only.

---

### `get_vesting_releasable`

```rust
fn get_vesting_releasable(id: u64) -> Option<i128>
```

Returns `vested_amount − released_amount`, i.e. tokens claimable right now. Returns `None` if `id` does not exist. Read-only.

---

### `release_vesting`

```rust
fn release_vested_tokens(
    beneficiary: Address,
    id: u64,
) -> Result<i128, QuickLendXError>
```

Transfers the currently releasable amount to `beneficiary` and persists the updated `released_amount`.

**Auth:** Beneficiary (`beneficiary.require_auth()`).

**Guard:** Payment reentrancy guard (SAC token transfer).

**Returns:** Amount released this call (`0` if nothing new has vested — idempotent).

**Errors:**
| Error | Condition |
|---|---|
| `InvalidTimestamp` | Called before `cliff_time` |
| `Unauthorized` | Caller does not match the schedule's `beneficiary` |
| `StorageKeyNotFound` | Schedule `id` does not exist |
| `OperationNotAllowed` | Re-entrant call detected |

**Emits:** `(vesting, released)` event with `(id, beneficiary, token, amount)`.

---

### `distribute_revenue_vested`

```rust
fn distribute_revenue_vested(
    admin: Address,
    period: u64,
    developer: Address,
    token: Address,
    vesting_start: u64,
    vesting_cliff_seconds: u64,
    vesting_end: u64,
) -> Result<(i128, u64, i128), QuickLendXError>
```

Wraps `FeeManager::distribute_revenue` with an optional on-chain vesting path for the developer share.

1. Calls `distribute_revenue` to finalize the `treasury / developer / platform` split for `period`.
2. If `developer_amount > 0`, creates a vesting schedule locking `developer_amount` of `token` for `developer`.

**Auth:** Admin (delegated through `FeeManager::distribute_revenue`).

**Guard:** Payment reentrancy guard (token transfer for vesting creation).

**Returns:** `(treasury_amount, schedule_id, platform_amount)`. `schedule_id` is `0` when `developer_amount` is `0`.

**Emits:** `(vesting, created)` event when a schedule is created.

---

## `VestingSchedule` struct

| Field | Type | Description |
|---|---|---|
| `id` | `u64` | Monotonic schedule identifier |
| `token` | `Address` | Vested token contract address |
| `beneficiary` | `Address` | Address entitled to claim vested tokens |
| `total_amount` | `i128` | Total tokens locked at creation |
| `released_amount` | `i128` | Cumulative tokens already released |
| `start_time` | `u64` | Unix timestamp when linear accrual begins |
| `cliff_time` | `u64` | Earliest timestamp when tokens are releasable |
| `end_time` | `u64` | Unix timestamp of full vest |
| `created_at` | `u64` | Ledger timestamp at creation |
| `created_by` | `Address` | Admin address that created the schedule |

---

## Overflow safety

- `elapsed` and `duration` use `saturating_sub` on `u64` — always ≥ 0.
- The numerator `total_amount × elapsed` uses `checked_mul` on `i128`; overflow returns `InvalidAmount`.
- The `released_amount` increment uses `checked_add`; overflow returns `InvalidAmount`.
- `overflow-checks = true` is set in `Cargo.toml` for all release profiles.

---

## Revenue routing integration

Protocol or OSS treasuries can vest contributor allocations by routing the developer share of fee revenue through `distribute_revenue_vested`. The admin supplies vesting timing parameters; the developer amount is transferred from admin's balance into contract custody and unlocks according to the cliff/slope curve. This turns the accounting-only `distribute_revenue` path into a fully auditable on-chain lock.

**Typical governance flow:**
1. Admin calls `configure_revenue_distribution` to set `developer_share_bps`.
2. After fees accumulate, admin calls `distribute_revenue_vested` with developer address and vesting schedule parameters.
3. Developer calls `release_vested_tokens` periodically to claim unlocked allocations.

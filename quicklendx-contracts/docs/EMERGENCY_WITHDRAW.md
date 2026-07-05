# Emergency Withdraw Timelock and Nonce Model

The emergency-withdraw flow is a last-resort admin recovery path for tokens
that are stuck in the contract outside live escrow obligations. It is
implemented in `src/emergency.rs` and exposed through public contract
entrypoints in `src/lib.rs`.

This flow is intentionally slower than a normal admin action:

1. The admin initiates a pending withdrawal.
2. The request waits through a 24-hour timelock.
3. The request can be executed only during the 7-day execution window.
4. The admin can cancel the current pending request instead of executing it.

## Public Entry Points

The public contract API wraps the internal `EmergencyWithdraw` helper:

| Entrypoint | Internal call | Purpose |
| --- | --- | --- |
| `initiate_emergency_withdraw(env, admin, token, amount, target_address)` | `EmergencyWithdraw::initiate` | Queue one pending emergency withdrawal. |
| `execute_emergency_withdraw(env, admin)` | `EmergencyWithdraw::execute` | Execute the current pending withdrawal after the timelock. |
| `cancel_emergency_withdraw(env, admin)` | `EmergencyWithdraw::cancel` | Cancel the current pending withdrawal. |
| `get_pending_emergency_withdraw(env)` | `EmergencyWithdraw::get_pending` | Read the current pending withdrawal, if any. |
| `can_exec_emergency(env)` | `EmergencyWithdraw::can_execute` | Return whether the current pending withdrawal is executable now. |
| `emg_time_until_unlock(env)` | `EmergencyWithdraw::time_until_unlock` | Return seconds until the pending withdrawal unlocks, or `0`. |
| `emg_time_until_expire(env)` | `EmergencyWithdraw::time_until_expiration` | Return seconds until the pending withdrawal expires, or `0`. |

## Stored State

`PendingEmergencyWithdrawal` is stored in instance storage under the
`emg_wd` symbol key. The struct contains:

| Field | Meaning |
| --- | --- |
| `token` | Token contract address to withdraw from the QuickLendX contract balance. |
| `amount` | Amount to transfer; must be positive at initiation. |
| `target` | Recipient of the recovered funds; cannot be the current contract address. |
| `unlock_at` | Earliest ledger timestamp at which execution is allowed. |
| `expires_at` | First ledger timestamp at which execution is no longer allowed. |
| `initiated_at` | Ledger timestamp when the request was initiated. |
| `initiated_by` | Admin address that initiated the request. |
| `nonce` | Monotonic request identifier assigned at initiation. |
| `cancelled` | Whether the current pending request has been cancelled. |
| `cancelled_at` | Ledger timestamp when cancellation occurred, or `0` if not cancelled. |

The contract uses a single pending-withdrawal slot. A later initiation writes a
new `PendingEmergencyWithdrawal` into that slot with a fresh nonce.

## Timelock and Expiration Constants

The defaults are defined in `src/emergency.rs`:

| Constant | Value | Meaning |
| --- | ---: | --- |
| `DEFAULT_EMERGENCY_TIMELOCK_SECS` | `24 * 60 * 60` | 24 hours from initiation to unlock. |
| `DEFAULT_EMERGENCY_EXPIRATION_SECS` | `7 * 24 * 60 * 60` | 7 days from unlock to expiration. |
| `MIN_EMERGENCY_TIMELOCK_SECS` | `60 * 60` | 1-hour lower bound constant. |
| `MAX_EMERGENCY_TIMELOCK_SECS` | `30 * 24 * 60 * 60` | 30-day upper bound constant. |

The current public initiation path uses the default 24-hour timelock and
7-day post-unlock expiration window.

## State Machine

```text
No pending request
  |
  | initiate_emergency_withdraw(admin, token, amount, target)
  v
Pending / locked
  |
  | now < unlock_at
  | execute -> EmergencyWithdrawTimelockNotElapsed
  |
  | cancel_emergency_withdraw(admin)
  v
Cancelled pending request
  |
  | execute -> EmergencyWithdrawCancelled
  |
  | initiate_emergency_withdraw(...) writes a new pending request with a new nonce

Pending / locked
  |
  | now >= unlock_at
  v
Pending / executable
  |
  | execute_emergency_withdraw(admin)
  v
Executed, pending slot removed

Pending / executable
  |
  | now >= expires_at
  v
Expired pending request
  |
  | execute -> EmergencyWithdrawExpired
  | cancel -> marks the pending request cancelled
```

### Boundary Rules

The execution window is `[unlock_at, expires_at)`.

| Time | `execute_emergency_withdraw` result |
| --- | --- |
| `now < unlock_at` | Rejects with `EmergencyWithdrawTimelockNotElapsed`. |
| `now == unlock_at` | Timelock has elapsed; execution may proceed if other checks pass. |
| `unlock_at < now < expires_at` | Execution may proceed if other checks pass. |
| `now == expires_at` | Rejects with `EmergencyWithdrawExpired`. |
| `now > expires_at` | Rejects with `EmergencyWithdrawExpired`. |

## Initiation Rules

`initiate_emergency_withdraw` requires the supplied `admin` to authorize and to
match the stored protocol admin through `AdminStorage::require_admin`.

It rejects when:

| Condition | Error |
| --- | --- |
| Caller is not the stored admin | `NotAdmin` from `AdminStorage::require_admin`. |
| `amount <= 0` | `InvalidAmount`. |
| `token` is the current contract address | `InvalidAddress`. |
| `target` is the current contract address | `InvalidAddress`. |
| Computed `expires_at <= now` | `InvalidTimestamp`. |

On success, initiation:

- computes `unlock_at = now + DEFAULT_EMERGENCY_TIMELOCK_SECS`;
- computes `expires_at = unlock_at + DEFAULT_EMERGENCY_EXPIRATION_SECS`;
- increments the global nonce and stores the incremented value in the pending request;
- writes `PendingEmergencyWithdrawal` with `cancelled = false`;
- emits `EmergencyWithdrawalInitiated`.

## Execution Rejection Conditions

`execute_emergency_withdraw` requires admin authorization and then validates the
current pending request in a fixed order:

| Check | Rejection |
| --- | --- |
| Supplied admin is not authorized or is not the stored admin | `NotAdmin`. |
| No pending request exists | `EmergencyWithdrawNotFound`. |
| Pending request is marked cancelled | `EmergencyWithdrawCancelled`. |
| `now < unlock_at` | `EmergencyWithdrawTimelockNotElapsed`. |
| `now >= expires_at` | `EmergencyWithdrawExpired`. |
| Held escrow reserve state is incomplete | `EmergencyWithdrawInsufficientBalance`. |
| Contract token balance is below held reserve | `EmergencyWithdrawInsufficientBalance`. |
| Requested amount exceeds same-token non-escrow surplus | `EmergencyWithdrawInsufficientBalance`. |
| Token transfer fails | The transfer error is returned. |

On success, execution transfers the requested `amount` of `token` from the
contract address to `target`, removes the pending slot, and emits
`EmergencyWithdrawalExecuted`.

## Nonce and Cancellation Model

The nonce model is intentionally simple:

- `get_nonce` returns the stored global nonce or `1` when the key is absent.
- `initiate` calls `increment_nonce`, stores the incremented value globally, and
  copies that value into the new pending request.
- `cancel` marks the pending request as `cancelled = true`, sets
  `cancelled_at = now`, and stores `(emg_cnl, nonce) = true`.
- `is_nonce_cancelled(nonce)` reads the `(emg_cnl, nonce)` marker.

There is no public `execute(nonce)` entrypoint. Execution always targets the
single current pending slot. A stale nonce cannot be supplied by a caller to
execute an old request. A cancelled current request is rejected because the
stored pending struct has `cancelled = true`; the per-nonce cancellation marker
is useful for audit and off-chain tracking.

Re-initiating after a cancellation writes a new pending request with a new
nonce. The old nonce remains marked cancelled.

## Operator Runbook

Before initiating:

1. Confirm the token balance is accidental surplus, not live escrow reserve.
2. Confirm the target address is correct and is not the contract address.
3. Confirm the admin key is the current stored admin.

After initiating:

```text
pending = get_pending_emergency_withdraw()
assert pending is Some

seconds_to_unlock = emg_time_until_unlock()
seconds_to_expire = emg_time_until_expire()
ready = can_exec_emergency()
```

Recommended operator behavior:

- If `seconds_to_unlock > 0`, wait and re-check later.
- If `seconds_to_unlock == 0` and `ready == true`, execute before
  `seconds_to_expire` reaches `0`.
- If `ready == false` after unlock, inspect the pending request and token
  reserves before retrying; the surplus check may be failing.
- If the request was initiated with the wrong token, amount, or target, call
  `cancel_emergency_withdraw(admin)` and initiate a new request.
- If `seconds_to_expire == 0`, the request can no longer execute. Cancel it or
  initiate a new request after reviewing the incident.

## Security Notes

- Emergency withdrawal is admin-only and should remain a last-resort path.
- The timelock is the primary safety control; it gives operators time to detect
  and cancel a bad request before execution.
- Execution is pause-exempt in the wrapper, so pausing the protocol does not
  block an already queued recovery action.
- The held-reserve surplus check is same-token aware: emergency withdrawal can
  only withdraw balance above the completed held escrow reserve for that token.
- Events emitted by the flow are `EmergencyWithdrawalInitiated`,
  `EmergencyWithdrawalExecuted`, and `EmergencyWithdrawalCancelled`.

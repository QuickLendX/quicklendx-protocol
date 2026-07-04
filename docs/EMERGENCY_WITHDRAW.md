# Emergency Withdraw Timelock and Nonce Cancellation

Audience: protocol operators, security reviewers, and contributors validating
the emergency-recovery path.

Emergency withdraw is the last-resort path for recovering tokens that are stuck
in the QuickLendX contract and cannot be returned through normal invoice,
escrow, settlement, or refund flows. The implementation lives in
`quicklendx-contracts/src/emergency.rs` and is exposed through contract
entrypoints in `quicklendx-contracts/src/lib.rs`.

## Recovery Scope

Use this path only when all of the following are true:

- The token balance is not part of a live invoice, bid, escrow, settlement, or
  refund flow.
- Governance or the deployment operator has documented the reason, token,
  amount, target, and planned execution window.
- The same-token held escrow reserve is complete, or the admin is prepared to
  run `repair_held_escrow_reserve` before execution.

Emergency withdraw is not a bypass for normal user-facing flows. Execution
rechecks withdrawable surplus and fails closed if the token's held-escrow
reserve is incomplete or the requested amount would touch reserved escrow funds.

## Entrypoint Flow

| Step | Entrypoint | Caller | Result |
|------|------------|--------|--------|
| 1 | `initiate_emergency_withdraw(admin, token, amount, target)` | Admin | Stores one pending withdrawal, assigns a new nonce, sets `unlock_at`, and emits `emg_init`. |
| 2 | `get_pending_emergency_withdraw()` | Anyone | Returns the current pending withdrawal, including token, amount, target, nonce, timestamps, and cancellation status. |
| 3 | `emg_time_until_unlock()` / `can_exec_emergency()` | Anyone | Monitors whether the 24-hour timelock has elapsed and whether the request can execute. |
| 4 | `execute_emergency_withdraw(admin)` | Admin | Transfers the queued amount to the target after the timelock, before expiration, and only if reserve checks pass. |
| 5 | `cancel_emergency_withdraw(admin)` | Admin | Marks the pending withdrawal cancelled, records its nonce as cancelled, and emits `emg_cncl`. |

There is a single pending-withdrawal slot. Initiating a new request stores a new
pending record with a fresh nonce, so operators should treat every initiation as
a new auditable action.

## Timelock Window

`initiate_emergency_withdraw` derives the execution window from the ledger
timestamp at initiation:

```text
initiated_at = current ledger timestamp
unlock_at    = initiated_at + DEFAULT_EMERGENCY_TIMELOCK_SECS
expires_at   = unlock_at + DEFAULT_EMERGENCY_EXPIRATION_SECS
```

The constants are:

| Constant | Value | Meaning |
|----------|-------|---------|
| `DEFAULT_EMERGENCY_TIMELOCK_SECS` | 24 hours | Required wait before execution. |
| `DEFAULT_EMERGENCY_EXPIRATION_SECS` | 7 days | Time after unlock before the request expires. |
| `MIN_EMERGENCY_TIMELOCK_SECS` | 1 hour | Lower bound reserved for timelock policy. |
| `MAX_EMERGENCY_TIMELOCK_SECS` | 30 days | Upper bound reserved for timelock policy. |

The execution window is inclusive at `unlock_at` and exclusive at `expires_at`:

```text
now < unlock_at       -> EmergencyWithdrawTimelockNotElapsed
now == unlock_at      -> execution may proceed
unlock_at < now < expires_at -> execution may proceed
now >= expires_at     -> EmergencyWithdrawExpired
```

## Nonce Cancellation Model

Emergency withdrawal uses a monotonic global nonce for request identity:

- `get_nonce()` returns the current stored nonce counter, defaulting to `1`.
- `initiate_emergency_withdraw` increments that counter and stores the new value
  on the pending withdrawal.
- `cancel_emergency_withdraw` marks the pending withdrawal cancelled and writes
  the pending nonce to the cancelled-nonce map.
- `is_nonce_cancelled(env, nonce)` reports whether a nonce was cancelled.
- Cancelled withdrawals fail with `EmergencyWithdrawCancelled` and cannot be
  executed later from the current pending record.

The nonce is part of the audit trail: include it in incident notes and compare
it with `get_pending_emergency_withdraw()` before execution.

## Execution Checks

`execute_emergency_withdraw(admin)` performs these checks before transfer:

1. The caller authenticates as the current admin.
2. A pending withdrawal exists.
3. The pending withdrawal is not cancelled.
4. `now >= unlock_at`.
5. `now < expires_at`.
6. The token's held escrow reserve is complete.
7. `amount <= contract_token_balance - same_token_held_escrow_reserve`.

Only after those checks pass does the contract transfer funds from the current
contract address to the stored target and clear the pending withdrawal.

## Cancellation Checks

`cancel_emergency_withdraw(admin)` can be called by the admin while the pending
record still exists. Cancellation:

- Requires admin auth.
- Fails with `EmergencyWithdrawNotFound` if there is no pending withdrawal.
- Fails with `EmergencyWithdrawCancelled` if the pending record is already
  cancelled.
- Stores `cancelled = true`, writes `cancelled_at`, and marks the nonce
  cancelled.
- Leaves the cancelled pending record queryable for audit visibility.

## Operator Checklist

Before initiation:

- Confirm the live admin with `get_current_admin()`.
- Verify the token address is not the contract address.
- Verify the target address is not the contract address.
- Confirm the amount is positive and excludes live escrow obligations.
- Document the reason, token, amount, target, expected nonce, and approver.

During the timelock:

- Monitor `get_pending_emergency_withdraw()`.
- Track `emg_time_until_unlock()` and `emg_time_until_expire()`.
- Use `cancel_emergency_withdraw(admin)` if the request is wrong, no longer
  needed, or suspicious.

Before execution:

- Reconfirm the pending token, amount, target, nonce, and `cancelled` flag.
- Confirm `can_exec_emergency()` is true.
- Confirm the same-token held escrow reserve repair is complete.
- Execute before `expires_at`; otherwise initiate a new request.

## Events and Related Docs

| Event | Emitted by | Meaning |
|-------|------------|---------|
| `emg_init` | `initiate_emergency_withdraw` | A withdrawal was queued and timelocked. |
| `emg_exec` | `execute_emergency_withdraw` | A queued withdrawal executed successfully. |
| `emg_cncl` | `cancel_emergency_withdraw` | A queued withdrawal was cancelled. |

Related references:

- [`docs/contracts/emergency-recovery.md`](contracts/emergency-recovery.md)
- [`docs/contracts/emergency.md`](contracts/emergency.md)
- [`docs/GOVERNANCE.md`](GOVERNANCE.md)
- [`docs/RUNBOOK_INCIDENT_RESPONSE.md`](RUNBOOK_INCIDENT_RESPONSE.md)

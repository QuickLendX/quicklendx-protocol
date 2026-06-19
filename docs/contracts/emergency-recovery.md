# Emergency Withdraw / Recovery for Stuck Funds

Emergency withdraw is an admin-only, timelocked mechanism to recover tokens that are stuck in the contract (e.g. sent by mistake or due to a bug). It is a **last-resort** tool and must be used only when normal flows cannot recover funds.

## When It Is Acceptable to Use

- Wrong token or wrong amount sent to the contract and not part of any invoice/escrow flow.
- Funds demonstrably stuck due to a contract bug or misconfiguration.
- Recovery is agreed as necessary by governance and documented.

It must **not** be used to bypass normal escrow, settlement, or refund flows.

## Escrow Reserve Protection

Emergency withdraw can recover only the same-token surplus that is not committed
to live escrows. Escrow creation increases a persistent reserve for the escrow
token, and escrow release or refund decreases that reserve. Execution reads this
persisted reserve directly, so normal escrow and emergency paths do not scan all
invoices.

The reserve record is trusted only after a full token-specific repair completes.
If the record is missing, incomplete, or was reset by migration/restore recovery,
emergency execution fails closed with `EmergencyWithdrawInsufficientBalance`.
Older bare-amount reserve records are also treated as incomplete until repaired.
The admin initializes or repairs the record by running
`repair_held_escrow_reserve(admin, currency, offset, limit)` from `offset = 0`
and then continuing with each returned `next_offset` until the final page.
Starting again at `offset = 0` recomputes the reserve from scratch and is the
recommended recovery if repair was interrupted or sidecar drift is suspected.
During a multi-page repair, escrow creation, release, and refund for that
currency reject with `InvalidStatus` until the final page completes.

The executable amount is:

```text
withdrawable = contract_token_balance - same_token_held_escrow_reserve
```

If the queued amount exceeds that withdrawable surplus, or the reserve record is
not yet complete, execution fails with `EmergencyWithdrawInsufficientBalance`
and the pending withdrawal remains queued for repair, cancellation, expiry, or
replacement. This preserves the normal escrow release and refund paths for held
escrows.

## Hardened Lifecycle Constraints

### Timelock Integrity

The emergency withdraw implements a mandatory timelock to provide a window for intervention:

- **Timelock period**: 24 hours (`DEFAULT_EMERGENCY_TIMELOCK_SECS`)
- Withdrawal cannot be executed until `unlock_at` (initiation time + timelock) has passed
- Execution at exactly `unlock_at` is permitted (boundary inclusive)

### Expiration Window

To prevent indefinite pending requests and stale withdrawal reuse:

- **Expiration window**: 7 days after `unlock_at` (`DEFAULT_EMERGENCY_EXPIRATION_SECS`)
- Withdrawals become invalid if not executed within this window
- `expires_at` = `unlock_at` + 7 days
- Execution exactly at `expires_at` fails (boundary exclusive)

### Cancellation Guarantees

Cancellation provides a way to invalidate pending withdrawals:

- Cancelled withdrawals **cannot be re-executed**, even after the timelock passes
- Cancellation is permanent and tracked via nonce
- Once cancelled, the same nonce cannot be used for execution
- A new initiation after cancellation creates a fresh withdrawal with new nonce

### Nonce-Based Replay Prevention

Each withdrawal request is assigned a unique nonce:

- Nonces are monotonically increasing (start at 1)
- Cancelled nonces are permanently recorded
- Attempting to execute a cancelled nonce fails with `EmergencyWithdrawCancelled`
- New initiations always increment the nonce, preventing stale request reuse

## Mechanism

1. **Initiate** (`initiate_emergency_withdraw`): Admin specifies token, amount, and target address.
   - A pending withdrawal is stored with `unlock_at` and `expires_at`
   - A unique `nonce` is assigned and incremented
   - Fails if amount ≤ 0, token equals contract, or target equals contract

2. **Execute** (`execute_emergency_withdraw`): After the timelock has elapsed and before expiration.
   - Fails if no pending withdrawal exists
   - Fails if timelock has not elapsed (`EmergencyWithdrawTimelockNotElapsed`)
   - Fails if expired (`EmergencyWithdrawExpired`)
   - Fails if cancelled (`EmergencyWithdrawCancelled`)
   - Fails if the token reserve repair is incomplete or amount exceeds non-escrow same-token surplus (`EmergencyWithdrawInsufficientBalance`)
   - On success, transfers tokens and clears the pending withdrawal

3. **Cancel** (`cancel_emergency_withdraw`): Admin can abort a pending withdrawal.
   - Fails if no pending withdrawal exists (`EmergencyWithdrawNotFound`)
   - Fails if already cancelled (`EmergencyWithdrawCancelled`)
   - Marks the withdrawal as cancelled with timestamp
   - Records the nonce as cancelled for replay prevention

4. **Query helpers**:
   - `get_pending_emergency_withdraw()`: Returns current pending withdrawal
   - `can_exec_emergency()`: Returns true if executable
   - `emg_time_until_unlock()`: Seconds until timelock elapses
   - `emg_time_until_expire()`: Seconds until expiration

5. **Reserve repair** (`repair_held_escrow_reserve`): Admin can recompute one
   token's held escrow reserve from indexed invoices in bounded pages. Pages
   must be run in returned-offset order. The token remains closed to emergency
   execution, escrow creation, release, and refund until the final page
   completes.

## Entrypoints

| Function | Who | Description |
|----------|-----|-------------|
| `initiate_emergency_withdraw(admin, token, amount, target)` | Admin | Schedules a withdrawal with timelock; fails if amount ≤ 0 or invalid addresses |
| `execute_emergency_withdraw(admin)` | Admin | Executes pending withdrawal after timelock, before expiration, and if not cancelled |
| `cancel_emergency_withdraw(admin)` | Admin | Cancels pending withdrawal; prevents future execution |
| `get_pending_emergency_withdraw()` | Anyone | Returns current pending withdrawal state |
| `can_exec_emergency()` | Anyone | Returns whether withdrawal can be executed now |
| `emg_time_until_unlock()` | Anyone | Returns seconds until timelock elapses |
| `emg_time_until_expire()` | Anyone | Returns seconds until expiration |
| `repair_held_escrow_reserve(admin, currency, offset, limit)` | Admin | Recomputes one token's held escrow reserve from indexed invoices in sequential pages capped at 100 |

## Security

- **Auth**: Both initiate, execute, and cancel require the current admin (from `AdminStorage`). Admin must authorize the transaction.
- **Timelock**: 24 hours (`DEFAULT_EMERGENCY_TIMELOCK_SECS`). Execute before unlock time returns `EmergencyWithdrawTimelockNotElapsed`.
- **Expiration**: 7 days after unlock (`DEFAULT_EMERGENCY_EXPIRATION_SECS`). Execute after expiration returns `EmergencyWithdrawExpired`.
- **Cancellation**: Permanent invalidation of withdrawal; cannot be undone. Cancelled withdrawals fail with `EmergencyWithdrawCancelled`.
- **Escrow reserve**: Same-token `Held` escrow amounts are excluded from the withdrawable balance after the token reserve has been fully initialized by repair. Incomplete reserve state blocks emergency execution.
- **Address validation**: Prevents using the contract address as token or target.
- **No optional second admin/multisig** in the current implementation; governance can require a second signer at the transaction level (e.g. multisig account).

## Errors

| Error | Code | Description |
|-------|------|-------------|
| `InvalidAmount` | 1200 | amount ≤ 0 on initiate |
| `InvalidAddress` | 1201 | token or target equals contract address |
| `EmergencyWithdrawNotFound` | 2101 | execute/cancel called with no pending withdrawal |
| `EmergencyWithdrawTimelockNotElapsed` | 2102 | execute called before unlock_at |
| `EmergencyWithdrawExpired` | 2103 | execute called at or after expires_at |
| `EmergencyWithdrawCancelled` | 2104 | execute/cancel called after cancellation |
| `EmergencyWithdrawAlreadyExists` | 2105 | Not currently used (only one pending at a time) |
| `EmergencyWithdrawInsufficientBalance` | 2106 | Requested amount exceeds contract balance after same-token held escrow reserve, or reserve repair is incomplete |

## Events

| Event | When | Data |
|-------|------|------|
| `emg_init` | On successful initiate | token, amount, target, unlock_at, admin |
| `emg_exec` | On successful execute | token, amount, target, admin |
| `emg_cncl` | On successful cancel | token, amount, target, admin |

## State Diagram

```
[No Pending]
     |
     | initiate()
     v
[Pending: unlock_at > now]
     |
     | now >= unlock_at
     v
[Pending: can_execute=true, cancelled=false]
     |                    |
     | cancel()           | execute()
     v                    v
[Cancelled]         [Executed]
 (perma-             (cleared)
  nent)
```

## Time Boundaries

| Boundary | Condition | Execute Allowed? |
|----------|-----------|------------------|
| Before unlock | `now < unlock_at` | No |
| Exactly at unlock | `now == unlock_at` | Yes |
| Before expiration | `now < expires_at` | Yes (if not cancelled) |
| Exactly at expiration | `now == expires_at` | No |
| After expiration | `now > expires_at` | No |

## Governance and Documentation

- Use only after internal and, if applicable, external review.
- Document each use: reason, amount, token, target, approval, and nonce.
- Prefer fixing normal flows or adding dedicated recovery paths over relying on emergency withdraw for recurring cases.
- Monitor pending withdrawals and their expiration times.

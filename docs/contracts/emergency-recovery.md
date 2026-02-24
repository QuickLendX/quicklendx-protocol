# Emergency Withdraw / Recovery for Stuck Funds

Emergency withdraw is an admin-only, timelocked mechanism to recover tokens that are stuck in the contract (e.g. sent by mistake or due to a bug). It is a **last-resort** tool and must be used only when normal flows cannot recover funds.

## When It Is Acceptable to Use

- Wrong token or wrong amount sent to the contract and not part of any invoice/escrow flow.
- Funds demonstrably stuck due to a contract bug or misconfiguration.
- Recovery is agreed as necessary by governance and documented.

It must **not** be used to bypass normal escrow, settlement, or refund flows.

## Mechanism

1. **Initiate** (`initiate_emergency_withdraw`): Admin specifies token, amount, and target address. A pending withdrawal is stored with an **unlock timestamp** = current time + timelock (default 24 hours).
2. **Execute** (`execute_emergency_withdraw`): After the timelock has elapsed, admin calls execute. The contract transfers the specified amount of the token from the contract balance to the target address and clears the pending withdrawal.
3. **Cancel** (`cancel_emergency_withdraw`): Admin can abort a pending withdrawal at any time before execute. Clears the pending slot. Use immediately if initiate was triggered by mistake or account compromise.

Only one pending emergency withdrawal exists at a time; a new initiate overwrites any existing pending withdrawal.

## Entrypoints

| Function                                                            | Who    | Description                                                                                             |
| ------------------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------- |
| `initiate_emergency_withdraw(admin, token, amount, target_address)` | Admin  | Schedules a withdrawal; callable only by admin. Fails if amount ≤ 0.                                    |
| `execute_emergency_withdraw(admin)`                                 | Admin  | Executes the pending withdrawal after timelock. Fails if no pending withdrawal or timelock not elapsed. |
| `get_pending_emergency_withdraw()`                                  | Anyone | Returns the current pending withdrawal,                                                                 |
| `cancel_emergency_withdraw(admin)`                                  | Admin  | Cancels pending withdrawal. Fails if no pending withdrawal exists.                                      |
| if any.                                                             |

## Security

- **Auth**: Both initiate and execute require the current admin (from `AdminStorage`). Admin must authorize the transaction.
- **Timelock**: Default 24 hours (`DEFAULT_EMERGENCY_TIMELOCK_SECS`). Execute before unlock time returns `OperationNotAllowed`.
- **No optional second admin/multisig** in the current implementation; governance can require a second signer at the transaction level (e.g. multisig account).

## Errors

- **InvalidAmount**: amount ≤ 0 on initiate.
- **StorageKeyNotFound**: execute called when there is no pending withdrawal.
- **OperationNotAllowed**: execute called before the timelock has elapsed.
- **InsufficientFunds** / **OperationNotAllowed**: token transfer fails (e.g. contract balance less than amount).
- **StorageKeyNotFound**: cancel called when there is no pending withdrawal.

## Events

- `emg_init`: On successful initiate (token, amount, target, unlock_at, admin).
- `emg_exec`: On successful execute (token, amount, target, admin).
- `emg_cncl`: On successful cancel (token, amount, target, admin).

## Governance and Documentation

- Use only after internal and, if applicable, external review.
- Document each use: reason, amount, token, target, and approval.
- Prefer fixing normal flows or adding dedicated recovery paths over relying on emergency withdraw for recurring cases.

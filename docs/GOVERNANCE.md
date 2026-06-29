# Governance

Audience: **operators** running a QuickLendX deployment.

This document describes the on-chain governance model of the QuickLendX
contract: who holds privileged authority, how that authority is transferred,
and which actions are protected by a timelock. Everything below maps to a real
contract entrypoint so reviewers can verify the documented intent against the
code.

## Governance model

QuickLendX uses a **single-admin** model. There is exactly one privileged
`admin` address at any time. The admin is the only principal authorized to call
the privileged entrypoints (pausing, protocol configuration, emergency
recovery, admin handover). All privileged entrypoints require Soroban auth
(`require_auth`) from the admin address, so holding the admin key — not merely
spoofing the address — is required.

The admin is set once at deployment via `initialize(admin)` and can only change
through the handover flow described below. There is no implicit super-admin and
no recovery path other than the timelocked emergency-withdraw flow.

## Proposal / handover flow

Admin handover protects against transferring control to a wrong or
uncontrolled address. Two modes are supported.

### One-step transfer

```text
transfer_admin(new_admin)
```

Requires auth from the current admin. The admin role moves immediately to
`new_admin`. Use only when `new_admin` is known to be controllable (e.g. a
contract you also operate).

### Two-step transfer (recommended)

Enabled with `set_two_step_enabled(true)`. The new admin must explicitly accept,
which proves the destination key is live before control moves:

```text
1. initiate_admin_transfer(new_admin)   # current admin proposes; role unchanged
2. accept_admin_transfer()              # new_admin accepts; role moves now
   # or
   cancel_admin_transfer()              # current admin aborts the pending proposal
```

Until step 2 completes the current admin retains full control. Inspect a pending
proposal with `get_pending_admin()` and check `is_transfer_locked()` /
`is_two_step_enabled()` for the current policy.

## Timelock: emergency withdrawal

The one privileged action that can move funds out of the contract — emergency
recovery of mistakenly-sent tokens — is **timelocked** so the community has a
window to react. It is a queue-then-execute flow:

```text
1. initiate_emergency_withdraw(...)   # admin queues the withdrawal, emits an event
2. (wait for the timelock to elapse)
3. execute_emergency_withdraw(admin)  # admin executes only after unlock time
   # or
   cancel_emergency_withdraw(admin)   # admin cancels; the nonce can never re-execute
```

Timelock parameters (see `emergency.rs`):

| Parameter | Value | Meaning |
|-----------|-------|---------|
| Default timelock | 24 hours | Minimum wait between initiate and execute |
| Minimum timelock | 1 hour | Lower bound on configurable timelock |
| Maximum timelock | 30 days | Upper bound on configurable timelock |
| Expiration | 7 days | A queued withdrawal that is not executed expires |

Read-only helpers for monitoring:

- `get_pending_emergency_withdraw()` — the queued withdrawal, if any
- `can_execute()` — whether the timelock has elapsed
- `time_until_unlock()` / `time_until_expiration()` — countdowns in seconds

A cancelled withdrawal's nonce is permanently burned, so a cancelled request can
never be replayed even after its timelock would have passed.

## Pausing

The admin can halt state-changing entrypoints in an incident:

```text
pause(admin)     # block protected entrypoints
unpause(admin)   # resume normal operation
```

Read-only entrypoints remain available while paused. Note that the emergency
withdraw execute/cancel paths are intentionally **pause-exempt** — the timelock,
not the pause flag, is the safety control for recovery.

## Operator checklist

1. Confirm the live admin with `get_current_admin()`.
2. Enable two-step handover (`set_two_step_enabled(true)`) before any rotation.
3. When rotating, `initiate_admin_transfer` then verify the destination accepts.
4. Treat any pending emergency withdrawal as a paging-worthy event and monitor
   `time_until_unlock()` until it resolves.

## See also

- [`docs/RUNBOOK_INCIDENT_RESPONSE.md`](RUNBOOK_INCIDENT_RESPONSE.md) — operator
  playbook for incident-mode recovery.
- [`docs/UPGRADE_PATHS.md`](UPGRADE_PATHS.md) — contract upgrade procedures.

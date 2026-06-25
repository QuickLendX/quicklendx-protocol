# Incident Response Runbook

This runbook is for QuickLendX protocol operators responding to unexpected
contract behavior on a deployed Soroban network. It assumes the operator has
authority to use the configured admin account, but it does not replace the
project's key-management or governance procedures.

Use this playbook when the contract accepts, rejects, emits, or settles
something in a way that does not match the documented protocol intent.
For information on which automated alerts to set up before an incident occurs, see the [Monitoring Guide](MONITORING.md).

## Decision Summary

| Situation | First action | Why |
| --- | --- | --- |
| Suspected value-moving, authorization, or settlement fault | Enter incident mode | Freezes user writes with one auditable action. |
| Planned upgrade or routine configuration work | Use maintenance mode | Keeps reads available while blocking writes. |
| UI, indexer, or freshness issue with no contract fault | Keep contract live and investigate off-chain services | Avoids unnecessary protocol downtime. |
| Unknown severity | Enter incident mode, then downgrade after triage | Biases toward preserving funds and auditability. |

Incident mode means calling `enter_incident_mode(admin, reason)`, which
atomically enables both the hard pause and maintenance mode. See
[`docs/contracts/operations.md`](contracts/operations.md#incident-mode-coordinated-pause--maintenance)
for the underlying contract behavior.

## 1. Open an Incident Record

Create one incident record before changing state. Include:

- reporter and timestamp,
- network name and contract ID,
- transaction hash, ledger sequence, or failing request ID,
- observed behavior,
- expected behavior,
- affected entrypoint, account, invoice ID, bid ID, escrow, or token if known.

Keep raw observations separate from hypotheses. For example, record "settlement
returned `ContractPaused`" before writing "pause flag may be stale".

## 2. Classify Severity

Use the highest matching severity.

| Severity | Examples | Required response |
| --- | --- | --- |
| Critical | Funds can move incorrectly, admin authorization appears bypassed, escrow reserve accounting is inconsistent, or settlement finality is wrong. | Enter incident mode immediately. |
| High | User writes are failing unexpectedly, pause/maintenance flags disagree, or diagnostics show repeated panics on a value-moving path. | Enter incident mode unless a maintainer confirms it is isolated. |
| Medium | Read-only data is stale, indexer lag is high, or UI displays an incorrect degraded-state banner while contract queries are healthy. | Keep contract live and follow the off-chain reliability path. |
| Low | Documentation mismatch, confusing error text, or local-only reproduction failure. | Do not pause; open a normal issue or PR. |

When severity is uncertain, treat it as High until a read-only check proves
otherwise.

## 3. Freeze Writes When Needed

For Critical or High incidents, use the coordinated incident entrypoint instead
of calling `pause` and `set_maintenance_mode` separately:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ACCOUNT> \
  --network <NETWORK> \
  -- enter_incident_mode \
  --admin <ADMIN_ADDRESS> \
  --reason "incident-<YYYYMMDD>-<short-summary>"
```

Record the returned `IncidentSnapshot` in the incident record:

- `is_paused`,
- `is_maintenance`,
- `reason`,
- `timestamp`.

The reason must be concise and no longer than 256 bytes. Do not include private
keys, seed phrases, customer secrets, or personally identifiable information.

## 4. Confirm the Protocol State

After entering incident mode, verify the state through read-only entrypoints.
Reads remain available during pause and maintenance.

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <READONLY_ACCOUNT> \
  --network <NETWORK> \
  -- is_paused

stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <READONLY_ACCOUNT> \
  --network <NETWORK> \
  -- is_maintenance_mode

stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <READONLY_ACCOUNT> \
  --network <NETWORK> \
  -- get_maintenance_reason
```

If the deployment exposes `get_health_status()`, prefer it for a single
ledger-consistent view of write availability:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <READONLY_ACCOUNT> \
  --network <NETWORK> \
  -- get_health_status
```

Expected incident-mode state:

- `is_paused == true`,
- `is_maintenance == true`,
- `writes_allowed == false` when using `get_health_status()`.

If pause and maintenance drift out of sync, re-run `enter_incident_mode` to
realign both flags or use `exit_incident_mode` only after maintainers decide the
protocol can safely return to normal.

## 5. Preserve Evidence

Capture enough data for reviewers to reproduce the behavior without needing
private credentials:

- transaction hash and ledger range,
- exact entrypoint and arguments with sensitive values redacted,
- emitted events,
- contract return value or error code,
- current pause, maintenance, and health snapshot,
- relevant indexer or backend request IDs,
- local reproduction command and result.

For local reproduction, enable diagnostics only in local builds. The diagnostics
guide explains the zero-overhead production behavior and the safe local
commands:

```bash
cargo test <test_name> -- --nocapture
cargo test --features diagnostics <test_name> -- --nocapture
```

See [`docs/diagnostics.md`](diagnostics.md) for log domains and feature-flag
details.

## 6. Triage the Fault Domain

Use read-only checks first.

| Fault domain | Checks | Related docs |
| --- | --- | --- |
| Pause or maintenance gate | `is_paused`, `is_maintenance_mode`, `get_maintenance_reason`, failed write error code | [`contracts/operations.md`](contracts/operations.md), [`contracts/emergency.md`](contracts/emergency.md) |
| Settlement or escrow | invoice state, escrow details, payment records, events, held reserve status | [`contracts/escrow.md`](contracts/escrow.md), [`contracts/settlement.md`](contracts/settlement.md) |
| Freshness or indexer lag | `get_health_status`, backend health endpoint, indexer lag and queue depth | [`../reliability.md`](../reliability.md) |
| Diagnostics or local reproduction | `qlx_log!` domains, targeted test logs, CLI verbose output | [`diagnostics.md`](diagnostics.md) |

Do not run emergency withdrawal, admin rotation, migration, or reserve repair
until the incident owner has a written recovery plan and required approvals.

## 7. Decide and Execute Recovery

Choose the narrowest recovery that fixes the confirmed fault.

| Confirmed finding | Recovery path |
| --- | --- |
| False alarm or off-chain-only issue | Keep contract state unchanged, resolve the off-chain issue, then document why no protocol action was needed. |
| Contract writes were frozen correctly | Keep incident mode active until the remediation PR, patch, or governance action is ready. |
| Pause/maintenance flags were toggled separately and drifted | Use `enter_incident_mode` to realign during the incident, or `exit_incident_mode` after approval to clear both. |
| Emergency withdraw may be required | Follow the timelock and reserve-safety procedure in [`contracts/emergency.md`](contracts/emergency.md#emergency-withdraw-procedure). |

Before returning to normal operation, confirm:

- root cause is understood or explicitly accepted as unknown,
- no further value-moving writes need to remain blocked,
- any remediation has been reviewed,
- monitoring is watching the affected entrypoint or invariant,
- incident record has the final state snapshot.

## 8. Exit Incident Mode

Only exit after approval from the incident owner or maintainer on duty.

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ACCOUNT> \
  --network <NETWORK> \
  -- exit_incident_mode \
  --admin <ADMIN_ADDRESS>
```

Verify normal state:

- `is_paused == false`,
- `is_maintenance_mode == false`,
- `get_maintenance_reason` is empty or absent,
- `writes_allowed == true` when using `get_health_status()` and no other
  backpressure condition is active.

Run a low-risk read-only smoke check first, then a maintainer-approved write
smoke check if the deployment procedure requires one.

## 9. Close the Incident

The incident record should end with:

- root cause,
- affected users, invoices, bids, escrows, or ledgers,
- exact recovery actions and timestamps,
- tests or commands used to validate the fix,
- follow-up issues or PRs,
- monitoring changes,
- whether any documentation or runbook step was missing.

If the runbook was unclear, update this document in the same PR as the
post-incident documentation fix or open a follow-up issue.

# Appeals Process

Audience: **operators** responsible for reviewing and resolving dispute
appeals on a QuickLendX deployment.

This document describes how a dispute resolution may be appealed, who
reviews the appeal, the timeline for each stage, and the possible
outcomes. It complements the dispute lifecycle documented in
[DISPUTE.md](DISPUTE.md).

## Overview

After an admin resolves a dispute via `resolve_dispute` or
`resolve_dispute_structured`, the losing party (business or investor)
may file an **appeal** — a formal request for a different reviewer to
overturn or modify the outcome.

Appeals are **off-chain governed**: there is no on-chain `appeal`
entrypoint.  The contract treats `Resolved` as a write-once terminal
state.  Instead, the appeal process is a policy layer that operators
follow, using existing admin entrypoints to implement the result.

## State machine

```
                 ┌────────────────────────────────────────────┐
                 │          Dispute Resolved (terminal)       │
                 └──────────────────┬─────────────────────────┘
                                    │  losing party files appeal (off-chain)
                                    ▼
                 ┌────────────────────────────────────────────┐
                 │           Appeal Filed (off-chain)         │
                 │  reviewed by an operator who did NOT       │
                 │  write the original resolution             │
                 └──────┬──────────────────────┬──────────────┘
                        │                      │
           ┌────────────┴─────┐        ┌──────┴─────────────┐
           │                  │        │                    │
           ▼                  ▼        ▼                    ▼
     ┌──────────┐      ┌──────────┐ ┌──────────┐      ┌──────────┐
     │ Upheld   │      │ Overturned│ │ Modified │      │Dismissed │
     │          │      │          │ │          │      │          │
     │ Original │      │ Escrow   │ │ Escrow   │      │ No change│
     │ outcome  │      │ released │ │ split /  │      │ to funds │
     │ stands   │      │ reversed │ │ adjusted │      │          │
     └──────────┘      └──────────┘ └──────────┘      └──────────┘
```

`Resolved` stays terminal on-chain regardless of the appeal outcome.
The appeal outcome is recorded off-chain and enforced via the
appropriate admin action (escrow release, settlement override, etc.).

## Who can file an appeal

The **losing party** from the dispute resolution:

| Resolution outcome | Who may appeal |
|---|---|
| `FavorBusiness` | Investor |
| `FavorInvestor` | Business owner |
| `Split` | Either party |
| `Dismissed` | Either party |

The appeal must be filed by a signer matching the invoice's
`business` or `investor` address (same auth check that
`create_dispute` uses).

## Who reviews the appeal

An appeal is reviewed by an operator who **did not** write the
original dispute resolution.  This avoids the reviewer passing
judgment on their own prior decision.

| Role | Who holds it | Can resolve disputes? | Can review appeals? |
|---|---|---|---|
| Admin (primary) | The configured `admin` address | Yes | Yes (but disqualified if they wrote the original resolution) |
| Admin (secondary) | A separate key held by a different operator | Yes | Yes |
| Escalation committee | Multi-sig or governance proposal | No (directly) | Yes (via `Governable` trait, if wired) |

A deployment SHOULD maintain at least two distinct operator accounts
so that appeals can be assigned to a reviewer who did not handle the
original dispute.

## Timeline

| Stage | Duration (policy) | Notes |
|---|---|---|
| Appeal window | **7 calendar days** from `Resolved` timestamp | After this, the resolution is final and the appeal right expires. |
| Assignment | Within 1 business day of filing | The appeal is assigned to a qualified reviewer. |
| Review period | **14 calendar days** from assignment | Reviewer investigates and issues a decision. |
| Escalation (optional) | +7 days if reviewer does not meet the deadline | The appeal auto-escalates to the escalation committee. |
| Implementation | Within 1 business day of decision | Operator executes the on-chain action (if any). |

These durations are policy defaults enforced by off-chain tooling
(monitoring, alerts, dashboard SLAs).  They are not enforced on-chain;
see [Timers and deadlines](#timers-and-deadlines) below.

## Appeal outcomes and on-chain effect

### Upheld

The original resolution stands.  No on-chain action is required beyond
closing the off-chain appeal record.

### Overturned

The opposite outcome is substituted.  Depending on the case, this may
require an admin entrypoint:

| Original | Overturned to | On-chain action |
|---|---|---|
| `FavorBusiness` | `FavorInvestor` | Admin calls `refund_escrow` to return funds to investor. |
| `FavorInvestor` | `FavorBusiness` | Admin calls `release_escrow` to release funds to business. |
| `Dismissed` | either favor | Admin calls the appropriate escrow entrypoint. |

```bash
# Example: overturn FavorBusiness → FavorInvestor
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ACCOUNT> \
  --network <NETWORK> \
  -- refund_escrow \
  --admin <ADMIN_ADDRESS> \
  --invoice_id <INVOICE_ID>
```

### Modified

The outcome is adjusted (e.g., `Split` with different percentages).
The admin may need to:

1. Manually compute the adjusted split amounts.
2. Call `release_escrow` or `transfer` to distribute funds per the
   modified split.
3. Record the modified outcome in the off-chain appeal record.

### Dismissed

The appeal is rejected as unfounded, untimely, or procedurally
defective.  No on-chain action is taken.  The original resolution is
treated as final.

## Timers and deadlines

**There are no on-chain timeouts for appeals.**  The contract does not
have an appeal state, an appeal deadline check, or an auto-escalation
entrypoint.  All timeline enforcement is off-chain:

- The frontend dashboard displays the appeal window countdown.
- Monitoring alerts (see [`MONITORING.md`](MONITORING.md)) page an
  operator when a `Resolved` dispute has no appeal decision within the
  policy window.
- An off-chain scheduler (or cron job) auto-escalates overdue appeals
  by reassigning them to the escalation committee.

> **Operational note** — If on-chain appeal deadlines are desired, the
> appeal mechanism would need to be added as a contract entrypoint
> (see [Future enhancements](#future-enhancements)).

## How appeals affect funds

While an appeal is pending:

- The dispute remains `Resolved` on-chain.
- Escrow stays locked (the invoice stays in its current state).
- Settlement is blocked — the settlement module rejects finalisation
  while `dispute_status != None` (same gate that operates during the
  primary dispute).
- Partial payments may still be recorded off-chain but do not advance
  settlement.

Once the appeal is decided:

| Outcome | Fund effect |
|---|---|
| Upheld | No change; escrow and settlement proceed per the original resolution. |
| Overturned | Admin executes the appropriate escrow entrypoint to reverse the flow of funds. |
| Modified | Admin manually distributes funds per the adjusted split. |
| Dismissed | Same as Upheld — original resolution governs. |

## Operator workflow

### 1. Receive the appeal

The losing party submits an appeal through the frontend or by
contacting support.  The off-chain system records:

- `invoice_id` and the `Resolved` dispute record,
- the appellant's address,
- the original resolution outcome and reviewer,
- the appellant's statement and supporting evidence,
- a timestamp within the 7-day appeal window.

### 2. Assign a reviewer

Pick an operator who did **not** write the original resolution.  If
the deployment has only one operator account, the appeal must be
escalated to the committee or a governance proposal.

### 3. Investigate

The reviewer examines:

- the invoice metadata and payment history,
- the original dispute reason and evidence,
- the resolution note and structured outcome,
- any new evidence provided with the appeal.

Use the read-only query entrypoints:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <READONLY_ACCOUNT> \
  --network <NETWORK> \
  -- get_dispute_details \
  --invoice_id <INVOICE_ID>
```

### 4. Decide

The reviewer issues one of the four outcomes above and records it in
the off-chain appeal record.

### 5. Execute

If the outcome requires an on-chain action (overturn or modify), an
admin operator executes the appropriate entrypoint.  If the outcome
is upheld or dismissed, only the off-chain record is closed.

## Logging and audit trail

Every appeal step SHOULD be logged to an off-chain audit system:

| Event | Data to record |
|---|---|
| Appeal filed | `invoice_id`, appellant, original resolution hash, timestamp |
| Appeal assigned | `invoice_id`, reviewer address, assignment timestamp |
| Appeal decided | `invoice_id`, outcome, reviewer note, decision timestamp |
| Appeal executed | `invoice_id`, on-chain tx hash (if applicable), executor, timestamp |

The on-chain dispute timeline (see `get_dispute_timeline`) records the
original resolution but **does not** record the appeal — the appeal is
entirely off-chain.  Cross-reference the off-chain appeal ID with the
on-chain `invoice_id` and `Resolved` event.

## Relationship to governance

Appeals that require funds to move against a `FavorBusiness` or
`FavorInvestor` resolution may be subject to governance constraints
if the deployment has wired the `Governable` trait for escrow
overrides.  See [`GOVERNANCE.md`](GOVERNANCE.md) for the admin
handover and timelock flows that apply to privileged entrypoints.

## Future enhancements

If on-chain appeal deadlines, auto-escalation, or multi-sig appeal
review are desired, the following would need to be added to the
contract:

1. An `AppealStatus` enum (`None`, `Filed`, `UnderReview`, `Decided`)
   stored alongside `DisputeStatus`.
2. An `appeal_dispute` entrypoint that transitions `Resolved` to
   `AppealFiled` with a 7-day window check.
3. Escalation timers that auto-escalate overdue appeals.
4. A `resolve_appeal` entrypoint that records the appeal outcome.

These enhancements are tracked in the
[DISPUTE_IMPLEMENTATION_COMPLETE.md](../DISPUTE_IMPLEMENTATION_COMPLETE.md#future-enhancements)
future-enhancements list.

## Related documentation

- [`DISPUTE.md`](DISPUTE.md) — Primary dispute lifecycle (open, review, resolve).
- [`GOVERNANCE.md`](GOVERNANCE.md) — Admin authority, handover, and timelock.
- [`ESCROW.md`](ESCROW.md) — How funds are locked and released.
- [`RUNBOOK_INCIDENT_RESPONSE.md`](RUNBOOK_INCIDENT_RESPONSE.md) — Incident-mode recovery for disputed resolution faults.
- [`MONITORING.md`](MONITORING.md) — Alerts for overdue disputes and appeals.
- [`INVOICE_LIFECYCLE.md`](INVOICE_LIFECYCLE.md) — Full invoice state machine.

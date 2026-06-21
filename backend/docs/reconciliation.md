# Reconciliation Worker — Design & Operations Reference

This document describes the drift-detection, severity-classification, automated
mitigation, and alerting pipeline introduced in the reconciliation subsystem.

---

## Overview

The reconciliation worker periodically compares the on-chain state of the
QuickLendX protocol against the locally indexed dataset.  Discrepancies are
called *drift events*.  The worker classifies every drift event by severity,
takes automated mitigation actions for critical events, and routes alerts to
the appropriate notification channels.

```
On-chain state ──► ReconciliationWorker ──► classifyDrift()
                                        │
                        ┌───────────────┼────────────────┐
                        ▼               ▼                ▼
                      LOW            MEDIUM            HIGH
                        │               │                │
                   standard alert  standard alert  ┌────▼──────────┐
                   (informational)  (informational) │ pauseRun()    │
                                                    │ routeAlert()  │
                                                    └───────────────┘
```

---

## Severity Classification

Drift severity is determined by `classifyDrift(report: DriftReport)` using the
following exclusive rules (evaluated in priority order):

| Priority | Severity | Condition |
|---|---|---|
| 1 (highest) | **HIGH** | `settlementAccountingMismatches ≥ 1` (any amount) |
| 2 | **HIGH** | `invoiceMismatches > 100` |
| 3 | **MEDIUM** | `2 ≤ invoiceMismatches ≤ 100` AND `settlementAccountingMismatches == 0` |
| 4 | **LOW** | `invoiceMismatches == 1` AND `settlementAccountingMismatches == 0` |
| 5 (lowest) | **LOW** | `invoiceMismatches == 0` AND `settlementAccountingMismatches == 0` (no drift) |

### Rationale

- **Settlement accounting mismatches** represent financial correctness
  violations and are treated as immediately critical regardless of count.
- **> 100 invoice mismatches** indicates a systemic indexing failure rather than
  transient lag, warranting immediate operator intervention.
- **2 – 100 invoice mismatches** is elevated but still within the range of
  replayable transient failures; automated alerting without a full stop allows
  the worker to self-heal.
- **1 invoice mismatch** is within the normal noise floor for eventual-
  consistency systems and requires monitoring but not immediate action.

---

## DriftReport Schema

```typescript
interface DriftReport {
  runId: string;                        // UUID identifying the reconciliation run
  timestamp: number;                    // UTC epoch-ms when this report was produced
  invoiceMismatches: number;            // on-chain vs. indexed invoice state divergences
  settlementAccountingMismatches: number; // settlement amounts that do not reconcile
}
```

---

## Automated Mitigation — HIGH Severity

When `classifyDrift` returns `HIGH` the worker performs two actions
**automatically and synchronously** before returning:

### 1. Pause the Backfill Run

```
backfillService.pauseRun(runId, reason)
```

- The call is **idempotent**: if the run is already paused (e.g. from a prior
  worker iteration) the call is silently ignored.
- If the run has not been explicitly registered, it is auto-created in the
  `Paused` state.
- Runs in terminal states (`Completed`, `Failed`) are unaffected.

### 2. Route a Critical Alert

```
alertRouter.routeAlert(alertKey, Severity.HIGH, message)
```

- The `alertKey` is derived as `HIGH_DRIFT:<runId>`.
- **Deduplication**: if an `Open` alert with the same key already exists the
  channel is NOT notified again.  This prevents alert spam across consecutive
  worker runs when the operator has not yet responded.
- Once the operator acknowledges the alert, the key becomes available again and
  the next HIGH event for the same run will fire a fresh notification.

---

## Alert Deduplication

The `AlertRouter` stores one alert per `alertKey`.  Before dispatching to a
notification channel it checks:

```
if existing alert with this key AND status === Open → suppress (return false)
```

Consecutive worker runs with identical HIGH drift on the same `runId` produce
**at most one** outstanding notification.  The channel is re-notified only
after the previous alert for that key is acknowledged.

---

## Notification Channels

| Severity | Channel |
|---|---|
| HIGH | `criticalChannel` (default: `NoOpChannel` — wire in PagerDuty/webhook) |
| MEDIUM / LOW | `standardChannel` (default: `NoOpChannel` — wire in Slack/email) |

Channels are swapped at runtime via:

```typescript
alertRouter.setCriticalChannel(new PagerDutyChannel(...));
alertRouter.setStandardChannel(new SlackChannel(...));
```

---

## Operator Recovery Flow (High-Level)

1. Receive critical alert (PagerDuty / Slack / email).
2. Investigate the root cause via on-chain explorer and indexer logs.
3. Acknowledge the alert: `alertRouter.acknowledgeAlert(alertKey)`.
4. Mark acknowledgement in the backfill service:
   `backfillService.markAlertAcknowledged(runId, alertKey)`.
5. Resume the run: `backfillService.resumeRun(runId)`.

See [`runbooks.md`](./runbooks.md) for the detailed step-by-step procedure.

---

## Module Map

| Module | Location | Responsibility |
|---|---|---|
| `reconciliationWorker` | `src/services/reconciliationWorker.ts` | Classify drift, trigger mitigation |
| `backfillService` | `src/services/backfillService.ts` | Manage run lifecycle, enforce ack gate |
| `alertRouter` | `src/services/alertRouter.ts` | Route alerts, dedup, acknowledgement |
| Shared types | `src/types/reconciliation.ts` | `DriftReport`, `Severity`, `BackfillRun`, `Alert` |

---

## Configuration

No environment variables are currently required.  Channel adapters are
dependency-injected at application startup.  All in-memory stores will be
replaced with persistent DB-backed implementations in a future milestone.

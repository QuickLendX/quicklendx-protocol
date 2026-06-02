# Observability — Ingest Lag Alerting

This document describes how the QuickLendX backend turns indexer-lag threshold
breaches into **alerts** and how **degraded mode auto-recovery** works. It
complements [reliability.md](./reliability.md) (which covers how degraded mode
gates writes) and [logging.md](./logging.md) (log redaction policy).

The alerting logic lives in
[`src/services/lagMonitor.ts`](../src/services/lagMonitor.ts).

---

## Overview

`LagMonitor` computes indexer lag (in ledgers) as
`current_ledger - last_indexed_ledger`. Two thresholds classify the lag into a
**level**:

| Level      | Condition                          | Effect                                 |
| ---------- | ---------------------------------- | -------------------------------------- |
| `none`     | `lag < warnThreshold`              | Healthy. All endpoints available.      |
| `warn`     | `lag >= warnThreshold`             | Degraded. Write endpoints gated (503). |
| `critical` | `lag >= criticalThreshold`         | Critically degraded. All writes blocked. |

The level is consumed by:

- **`GET /api/v1/status`** — surfaces the current level to clients.
- **`degradedGuard`** middleware — gates write/sensitive endpoints.

Prior to this feature, threshold breaches were silent (no operator signal) and
recovery was implicit (a single good reading immediately re-opened the write
guard, allowing it to flap). This feature adds **alerts on transitions** and
**hysteresis-backed auto-recovery**.

---

## Thresholds & configuration

All four parameters are configurable via environment variables. Defaults are
chosen for a ~5s ledger cadence.

| Env var                   | Default | Meaning                                                              |
| ------------------------- | ------- | -------------------------------------------------------------------- |
| `LAG_WARN_THRESHOLD`      | `10`    | Lag (ledgers) at which the system becomes degraded (`warn`).         |
| `LAG_CRITICAL_THRESHOLD`  | `50`    | Lag (ledgers) at which the system becomes critically degraded.       |
| `LAG_HYSTERESIS_MARGIN`   | `3`     | Ledgers **below** a threshold the lag must fall to before recovering.|
| `LAG_RECOVERY_POLLS`      | `3`     | Consecutive recovered polls required before a degraded level clears. |

Non-numeric or empty values fall back to the defaults. `recoveryPolls` is
clamped to `>= 1` and `hysteresisMargin` to `>= 0`.

Thresholds can also be set at runtime in tests/bootstrap via
`setThresholds(warn, critical)` and `setHysteresis(margin, polls)`.

---

## Hysteresis & auto-recovery

To stop the monitor flapping when lag hovers around a threshold, the monitor
tracks an **effective level** separately from the **instantaneous level**
computed from the raw lag:

- **Escalation is immediate.** As soon as the raw lag reaches a higher level
  (e.g. `lag >= criticalThreshold`), the effective level jumps there. This is
  the fail-safe direction — a breach gates writes without delay.
- **De-escalation is sustained.** To clear a level, the raw lag must fall to
  the **recovery threshold** (`threshold - hysteresisMargin`) and stay there
  for `recoveryPolls` consecutive polls. A single breach anywhere in that
  window resets the streak. Recovery steps down **one level at a time**
  (`critical → warn → none`) so the `warn` write-guard window is never skipped.

Recovery thresholds with the defaults:

- Recover out of `critical` → `warn` when `lag <= 50 - 3 = 47` for 3 polls.
- Recover out of `warn` → `none` when `lag <= 10 - 3 = 7` for 3 polls.

`getLagStatus()` (used by `/status` and `degradedGuard`) reports the effective
level. It **escalates immediately** when called but **never auto-clears** — only
the scheduled `poll()` path performs de-escalation. This means the many guard
and status calls per interval can raise the level but can never lower it.

---

## Alert events

Alerts are emitted **only on transitions** of the effective level — never on
every poll. Each transition:

1. Logs a single structured JSON line (`type: "LAG_ALERT"`), at `WARN` for
   escalations and `INFO` for recoveries.
2. Increments in-process counters (see [Metrics](#metrics)).
3. Notifies any subscribers registered via `onAlert(listener)`.

### Alert payload

```jsonc
{
  "from": "warn",          // level moved away from
  "to": "critical",        // level moved to
  "direction": "escalation", // or "recovery"
  "lag": 62,               // raw lag at transition (ledgers)
  "warnThreshold": 10,
  "criticalThreshold": 50,
  "at": "2026-06-02T12:00:00.000Z"
}
```

> **Security:** Alert payloads carry **only operational fields** — lag,
> thresholds, level, timestamp. They never include request bodies, wallet
> data, auth tokens, or any other secrets. The logged line uses the same
> fixed shape, so no caller-supplied data can leak into log sinks.

### Subscribing

```ts
import { lagMonitor } from "../services/lagMonitor";

const unsubscribe = lagMonitor.onAlert((event) => {
  // forward to PagerDuty / Slack / metrics exporter, etc.
});
```

A throwing subscriber is isolated and never breaks the monitor.

---

## Polling

`poll()` reads a fresh lag value, advances the hysteresis state machine, and
emits any resulting transition alert. Schedule it on a fixed cadence (mirroring
the invariant scheduler pattern):

```ts
import { lagMonitor } from "../services/lagMonitor";

setInterval(() => {
  void lagMonitor.poll();
}, 5000);
```

Do **not** call `poll()` per request — request paths should call the read-only
`getLagStatus()`.

### Missing / corrupt current-ledger reads

If the current-ledger read throws, or yields a non-finite or negative lag, the
monitor **fails safe to `critical`** (it returns `lag = criticalThreshold`).
An unknown reading must never silently clear a degraded state or open the write
guard.

---

## Metrics

`getAlertMetrics()` returns a defensive copy of the in-process counters,
suitable for exposure via the monitoring endpoint or a scraper:

| Field                      | Meaning                                                  |
| -------------------------- | -------------------------------------------------------- |
| `escalations`              | Total escalation transitions observed.                   |
| `recoveries`               | Total recovery transitions observed.                     |
| `transitionsTo`            | Transition count by destination level (`none`/`warn`/`critical`). |
| `currentLevel`             | Current effective level.                                 |
| `consecutiveRecoveryPolls` | Consecutive polls the lag has been within recovery range. |

---

## Edge cases (covered by tests)

See [`src/tests/lagMonitor.alerts.test.ts`](../src/tests/lagMonitor.alerts.test.ts):

- **Flapping** around a threshold produces no spurious transitions; a single
  good poll never clears a degraded state.
- **Sustained breach** holds the level with no duplicate alerts.
- **Rapid recovery** still drains one level per recovery window, preserving the
  `warn` guard window.
- **Missing current-ledger read** fails safe to `critical`.

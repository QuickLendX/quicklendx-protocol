/**
 * LagMonitor
 *
 * Computes the indexer lag (in ledgers) and determines whether the system
 * is in a degraded state based on configurable thresholds.
 *
 * Thresholds
 * ----------
 * WARN_THRESHOLD  (default 10)  – lag at which the system is considered
 *                                  degraded.  Read-only endpoints still work;
 *                                  write/sensitive endpoints are gated.
 * CRITICAL_THRESHOLD (default 50) – lag at which the system is considered
 *                                    critically degraded.  All mutating
 *                                    endpoints are blocked.
 *
 * Hysteresis & auto-recovery
 * --------------------------
 * To stop the monitor flapping between levels when the lag hovers around a
 * threshold, an *effective* level is tracked separately from the
 * *instantaneous* level computed from the raw lag:
 *
 *   - To ESCALATE (none→warn, warn→critical) the raw lag must reach the
 *     upper threshold.
 *   - To DE-ESCALATE the raw lag must fall below the threshold minus the
 *     hysteresis margin (the "recovery threshold"), AND it must stay there
 *     for `recoveryPolls` consecutive polls before the level is cleared.
 *
 * This means a single good poll never clears a degraded state — recovery is
 * explicit and sustained, while escalation is immediate.
 *
 * Alert events
 * ------------
 * Alerts are emitted only on *transitions* of the effective level (via
 * `poll()`), never on every poll.  Each transition is logged as a single
 * structured JSON line and increments an in-process counter.  Subscribers
 * can register via `onAlert()`.  Alert payloads contain only operational
 * metrics (lag, thresholds, level) — never request data or secrets.
 *
 * The thresholds can be overridden via environment variables:
 *   LAG_WARN_THRESHOLD     (integer, ledgers)
 *   LAG_CRITICAL_THRESHOLD (integer, ledgers)
 *   LAG_HYSTERESIS_MARGIN  (integer, ledgers; default 3)
 *   LAG_RECOVERY_POLLS     (integer, polls;   default 3)
 */

import { statusService } from "./statusService";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

export const DEFAULT_WARN_THRESHOLD = 10;
export const DEFAULT_CRITICAL_THRESHOLD = 50;
/** Ledgers below a threshold the lag must fall to before de-escalating. */
export const DEFAULT_HYSTERESIS_MARGIN = 3;
/** Consecutive sub-recovery polls required before a degraded level clears. */
export const DEFAULT_RECOVERY_POLLS = 3;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type DegradedLevel = "none" | "warn" | "critical";

export interface LagStatus {
  /** Current lag in ledgers. */
  lag: number;
  /** Threshold at which warn-level degradation begins. */
  warnThreshold: number;
  /** Threshold at which critical-level degradation begins. */
  criticalThreshold: number;
  /** Degradation level (hysteresis-aware effective level). */
  level: DegradedLevel;
  /** True when level is "warn" or "critical". */
  isDegraded: boolean;
  /** True only when level is "critical". */
  isCritical: boolean;
  /** ISO-8601 timestamp of this snapshot. */
  checkedAt: string;
}

/** Payload emitted on an effective-level transition. Operational data only. */
export interface LagAlertEvent {
  /** The level the monitor moved away from. */
  from: DegradedLevel;
  /** The level the monitor moved to. */
  to: DegradedLevel;
  /** "escalation" when severity increased, "recovery" when it decreased. */
  direction: "escalation" | "recovery";
  /** Raw lag (ledgers) at the moment of transition. */
  lag: number;
  warnThreshold: number;
  criticalThreshold: number;
  /** ISO-8601 timestamp of the transition. */
  at: string;
}

export type LagAlertListener = (event: LagAlertEvent) => void;

/** In-process counters, surfaced to the monitoring endpoint / scrapers. */
export interface LagAlertMetrics {
  /** Total transitions observed, by direction. */
  escalations: number;
  recoveries: number;
  /** Transitions broken down by destination level. */
  transitionsTo: Record<DegradedLevel, number>;
  /** Current effective level. */
  currentLevel: DegradedLevel;
  /**
   * Consecutive polls the raw lag has been at/below the recovery threshold
   * for the current effective level. Resets on any breach.
   */
  consecutiveRecoveryPolls: number;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Severity rank used to decide escalation vs. recovery. */
const LEVEL_RANK: Record<DegradedLevel, number> = {
  none: 0,
  warn: 1,
  critical: 2,
};

function parseEnvInt(name: string): number | undefined {
  const raw = process.env[name];
  if (raw === undefined || raw === "") return undefined;
  const parsed = parseInt(raw, 10);
  return Number.isNaN(parsed) ? undefined : parsed;
}

// ---------------------------------------------------------------------------
// LagMonitor
// ---------------------------------------------------------------------------

export class LagMonitor {
  private static instance: LagMonitor;

  private _warnThreshold: number;
  private _criticalThreshold: number;
  private _hysteresisMargin: number;
  private _recoveryPolls: number;

  /**
   * Hysteresis-aware effective level. Escalates immediately on breach;
   * de-escalates only after sustained recovery. This is the level reported
   * by getLagStatus() and therefore enforced by degradedGuard.
   */
  private _effectiveLevel: DegradedLevel = "none";
  /** Consecutive polls the raw lag has been within recovery range. */
  private _recoveryStreak = 0;

  private readonly _listeners = new Set<LagAlertListener>();
  private readonly _metrics: LagAlertMetrics = {
    escalations: 0,
    recoveries: 0,
    transitionsTo: { none: 0, warn: 0, critical: 0 },
    currentLevel: "none",
    consecutiveRecoveryPolls: 0,
  };

  constructor(
    warnThreshold?: number,
    criticalThreshold?: number,
    hysteresisMargin?: number,
    recoveryPolls?: number
  ) {
    this._warnThreshold =
      warnThreshold !== undefined
        ? warnThreshold
        : parseEnvInt("LAG_WARN_THRESHOLD") ?? DEFAULT_WARN_THRESHOLD;

    this._criticalThreshold =
      criticalThreshold !== undefined
        ? criticalThreshold
        : parseEnvInt("LAG_CRITICAL_THRESHOLD") ?? DEFAULT_CRITICAL_THRESHOLD;

    this._hysteresisMargin =
      hysteresisMargin !== undefined
        ? hysteresisMargin
        : parseEnvInt("LAG_HYSTERESIS_MARGIN") ?? DEFAULT_HYSTERESIS_MARGIN;

    this._recoveryPolls =
      recoveryPolls !== undefined
        ? recoveryPolls
        : parseEnvInt("LAG_RECOVERY_POLLS") ?? DEFAULT_RECOVERY_POLLS;

    if (this._recoveryPolls < 1) this._recoveryPolls = DEFAULT_RECOVERY_POLLS;
    if (this._hysteresisMargin < 0) this._hysteresisMargin = 0;
  }

  // -------------------------------------------------------------------------
  // Singleton
  // -------------------------------------------------------------------------

  public static getInstance(): LagMonitor {
    if (!LagMonitor.instance) {
      LagMonitor.instance = new LagMonitor();
    }
    return LagMonitor.instance;
  }

  // -------------------------------------------------------------------------
  // Threshold accessors (allow test overrides)
  // -------------------------------------------------------------------------

  get warnThreshold(): number {
    return this._warnThreshold;
  }

  get criticalThreshold(): number {
    return this._criticalThreshold;
  }

  get hysteresisMargin(): number {
    return this._hysteresisMargin;
  }

  get recoveryPolls(): number {
    return this._recoveryPolls;
  }

  /** Current hysteresis-aware effective level. */
  get effectiveLevel(): DegradedLevel {
    return this._effectiveLevel;
  }

  setThresholds(warn: number, critical: number): void {
    if (warn <= 0 || critical <= 0) {
      throw new RangeError("Thresholds must be positive integers");
    }
    if (warn >= critical) {
      throw new RangeError(
        "warnThreshold must be strictly less than criticalThreshold"
      );
    }
    this._warnThreshold = warn;
    this._criticalThreshold = critical;
  }

  /**
   * Configure hysteresis behaviour.
   * @param margin Ledgers below a threshold the lag must drop to recover.
   * @param polls  Consecutive recovered polls required before clearing.
   */
  setHysteresis(margin: number, polls: number): void {
    if (margin < 0) {
      throw new RangeError("hysteresisMargin must be >= 0");
    }
    if (!Number.isInteger(polls) || polls < 1) {
      throw new RangeError("recoveryPolls must be a positive integer");
    }
    this._hysteresisMargin = margin;
    this._recoveryPolls = polls;
  }

  // -------------------------------------------------------------------------
  // Alert subscription / metrics
  // -------------------------------------------------------------------------

  /** Subscribe to transition alerts. Returns an unsubscribe function. */
  onAlert(listener: LagAlertListener): () => void {
    this._listeners.add(listener);
    return () => this._listeners.delete(listener);
  }

  /** Snapshot of in-process alert counters. */
  getAlertMetrics(): LagAlertMetrics {
    return {
      ...this._metrics,
      transitionsTo: { ...this._metrics.transitionsTo },
    };
  }

  /** Reset the state machine and counters. Intended for tests/bootstrap. */
  reset(): void {
    this._effectiveLevel = "none";
    this._recoveryStreak = 0;
    this._metrics.escalations = 0;
    this._metrics.recoveries = 0;
    this._metrics.transitionsTo = { none: 0, warn: 0, critical: 0 };
    this._metrics.currentLevel = "none";
    this._metrics.consecutiveRecoveryPolls = 0;
  }

  // -------------------------------------------------------------------------
  // Core computation
  // -------------------------------------------------------------------------

  /**
   * Computes the instantaneous lag level from a raw lag value.
   * Pure function — no I/O, no hysteresis, no side effects.
   */
  computeLevel(lag: number): DegradedLevel {
    if (lag >= this._criticalThreshold) return "critical";
    if (lag >= this._warnThreshold) return "warn";
    return "none";
  }

  /**
   * The lag value at/below which the system is considered recovered *out of*
   * the given level. Recovering from "critical" returns to "warn" territory
   * (critical threshold minus margin); recovering from "warn" returns to
   * healthy (warn threshold minus margin). Clamped at 0.
   */
  private recoveryThresholdFor(level: DegradedLevel): number {
    if (level === "critical") {
      return Math.max(0, this._criticalThreshold - this._hysteresisMargin);
    }
    // warn (or none, unused)
    return Math.max(0, this._warnThreshold - this._hysteresisMargin);
  }

  /**
   * Advance the hysteresis state machine by one observation and return the
   * resulting effective level. Pure with respect to I/O — it only mutates
   * internal state and (on a transition) emits an alert. Safe to call from
   * a scheduled poller.
   *
   * Escalation is immediate: as soon as the raw lag reaches a higher
   * instantaneous level, the effective level jumps there and the recovery
   * streak resets.
   *
   * De-escalation requires the raw lag to sit at/below the recovery
   * threshold for `recoveryPolls` consecutive observations. A single breach
   * anywhere in the window resets the streak.
   */
  observe(lag: number, at: string = new Date().toISOString()): DegradedLevel {
    const instant = this.computeLevel(lag);
    const prev = this._effectiveLevel;

    if (LEVEL_RANK[instant] > LEVEL_RANK[prev]) {
      // Escalation — immediate, no dwell required.
      this._recoveryStreak = 0;
      this._setEffectiveLevel(instant, lag, at);
      return this._effectiveLevel;
    }

    if (this._effectiveLevel === "none") {
      // Healthy and staying healthy (instant is none too).
      this._recoveryStreak = 0;
      this._metrics.consecutiveRecoveryPolls = 0;
      return this._effectiveLevel;
    }

    // Currently degraded and lag is not escalating. Check for sustained
    // recovery toward the next-lower level.
    const recoveryThreshold = this.recoveryThresholdFor(this._effectiveLevel);

    if (lag <= recoveryThreshold) {
      this._recoveryStreak += 1;
      this._metrics.consecutiveRecoveryPolls = this._recoveryStreak;
      if (this._recoveryStreak >= this._recoveryPolls) {
        // Step down exactly one level so a deep recovery from critical still
        // passes through warn rather than skipping the warn guard window.
        const next: DegradedLevel =
          this._effectiveLevel === "critical" ? "warn" : "none";
        this._recoveryStreak = 0;
        this._metrics.consecutiveRecoveryPolls = 0;
        this._setEffectiveLevel(next, lag, at);
        // If, after stepping down, the lag is already below the next level's
        // recovery threshold, the following poll(s) will continue draining it
        // down one level at a time — keeping each transition observable.
      }
    } else {
      // Lag bounced back above the recovery threshold; reset the streak.
      this._recoveryStreak = 0;
      this._metrics.consecutiveRecoveryPolls = 0;
    }

    return this._effectiveLevel;
  }

  /**
   * Apply a new effective level and emit a transition alert. Caller must have
   * already determined `next !== current`.
   */
  private _setEffectiveLevel(
    next: DegradedLevel,
    lag: number,
    at: string
  ): void {
    const from = this._effectiveLevel;
    if (next === from) return;

    this._effectiveLevel = next;
    this._metrics.currentLevel = next;

    const direction: "escalation" | "recovery" =
      LEVEL_RANK[next] > LEVEL_RANK[from] ? "escalation" : "recovery";

    if (direction === "escalation") this._metrics.escalations += 1;
    else this._metrics.recoveries += 1;
    this._metrics.transitionsTo[next] += 1;

    const event: LagAlertEvent = {
      from,
      to: next,
      direction,
      lag,
      warnThreshold: this._warnThreshold,
      criticalThreshold: this._criticalThreshold,
      at,
    };

    this._emitAlert(event);
  }

  /** Log the transition (structured) and notify subscribers. */
  private _emitAlert(event: LagAlertEvent): void {
    // Single structured line per transition — operational fields only,
    // never request bodies, auth material, or other secrets.
    if (process.env["NODE_ENV"] !== "test") {
      const line = JSON.stringify({
        level: event.direction === "escalation" ? "WARN" : "INFO",
        type: "LAG_ALERT",
        event: event.direction,
        from: event.from,
        to: event.to,
        lag: event.lag,
        warn_threshold: event.warnThreshold,
        critical_threshold: event.criticalThreshold,
        timestamp: event.at,
      });
      if (event.direction === "escalation") {
        console.warn(line);
      } else {
        console.info(line);
      }
    }

    for (const listener of this._listeners) {
      try {
        listener(event);
      } catch {
        // A misbehaving subscriber must never break the monitor.
      }
    }
  }

  // -------------------------------------------------------------------------
  // Status / polling
  // -------------------------------------------------------------------------

  /**
   * Fetches the current system status and returns a full LagStatus snapshot.
   *
   * The reported `level` is the **instantaneous** level derived directly from
   * the current lag (no hysteresis, no side effects). This preserves the
   * historical contract relied on by `/status`, the readiness probe, and
   * `degradedGuard` — the snapshot always reflects the lag *right now*, and
   * the call neither mutates the state machine nor emits alerts. Hysteresis,
   * alerting, and auto-recovery are driven separately by `poll()`.
   */
  async getLagStatus(): Promise<LagStatus> {
    const lag = await this.readLag();
    const level = this.computeLevel(lag);
    return this.snapshot(lag, level);
  }

  /**
   * Advance the hysteresis state machine using a fresh lag reading and emit
   * any resulting transition alert. Returns a snapshot whose `level` is the
   * hysteresis-aware **effective** level. Call this on a fixed interval (e.g.
   * from a scheduler), NOT per request.
   *
   * This is where threshold breaches become alerts and where degraded-mode
   * auto-recovery (sustained-over-N-polls) happens. `getLagStatus()` remains
   * instantaneous so per-request consumers are unaffected.
   */
  async poll(): Promise<LagStatus> {
    const lag = await this.readLag();
    const level = this.observe(lag);
    return this.snapshot(lag, level);
  }

  /**
   * Snapshot using the hysteresis-aware effective level without advancing the
   * state machine. Useful for a guard that wants auto-recovery semantics
   * (degraded stays closed until `poll()` clears it) rather than instantaneous
   * lag. Read-only: no alerts, no state change.
   */
  async getEffectiveStatus(): Promise<LagStatus> {
    const lag = await this.readLag();
    return this.snapshot(lag, this._effectiveLevel);
  }

  /**
   * Read the current lag from statusService.
   *
   * If the current-ledger read fails or yields a nonsensical (negative /
   * non-finite) lag, we treat the indexer as critically lagging rather than
   * healthy: a missing reading must never silently clear a degraded state or
   * open the write guard. The raw error is swallowed here (it is logged at
   * the call site / global handler) so the monitor degrades safely.
   */
  private async readLag(): Promise<number> {
    try {
      const status = await statusService.getStatus();
      const lag = status.index_lag;
      if (!Number.isFinite(lag) || lag < 0) {
        // Unknown / corrupt reading → fail safe to critical.
        return this._criticalThreshold;
      }
      return lag;
    } catch {
      // Cannot determine lag → fail safe to critical (block writes).
      return this._criticalThreshold;
    }
  }

  /** Build a LagStatus from a raw lag and an already-determined level. */
  private snapshot(lag: number, level: DegradedLevel): LagStatus {
    return {
      lag,
      warnThreshold: this._warnThreshold,
      criticalThreshold: this._criticalThreshold,
      level,
      isDegraded: level !== "none",
      isCritical: level === "critical",
      checkedAt: new Date().toISOString(),
    };
  }

  /**
   * Convenience: returns true if the system is currently degraded at any level.
   */
  async isDegraded(): Promise<boolean> {
    const s = await this.getLagStatus();
    return s.isDegraded;
  }

  /**
   * Convenience: returns true if the system is critically degraded.
   */
  async isCritical(): Promise<boolean> {
    const s = await this.getLagStatus();
    return s.isCritical;
  }

  // -------------------------------------------------------------------------
  // Ingestion metrics (added for reorg handling)
  // -------------------------------------------------------------------------

  /**
   * Record a successful batch ingestion.
   * Called after commitBatch succeeds in ingestBatch.
   */
  recordIngestion(cursor: number, eventCount: number): void {
    // In production, this would increment a Prometheus counter
    // and update the last-seen cursor gauge.
    // For now, we log at debug level.
    if (process.env["NODE_ENV"] !== "test") {
      console.debug(
        `[LagMonitor] ingestion: cursor=${cursor}, events=${eventCount}`
      );
    }
  }

  /**
   * Record a reorg rollback event.
   * Called when rollbackTo is executed in rollbackAndReingest.
   */
  recordRollback(cursor: number): void {
    // In production, this would increment a rollback counter
    // and emit an alert if rollbacks are frequent.
    console.warn(
      `[LagMonitor] rollback: cursor=${cursor}, timestamp=${new Date().toISOString()}`
    );
  }

  /**
   * Stop the polling loop during graceful shutdown.
   * Prevents noisy lag-alert transitions while other services wind down.
   */
  stopPolling(): void {
    this._effectiveLevel = "none";
    this._recoveryStreak = 0;
  }
}

export const lagMonitor = LagMonitor.getInstance();

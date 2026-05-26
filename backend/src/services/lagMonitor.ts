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
 * The thresholds can be overridden via environment variables:
 *   LAG_WARN_THRESHOLD     (integer, ledgers)
 *   LAG_CRITICAL_THRESHOLD (integer, ledgers)
 */

import { statusService } from "./statusService";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

export const DEFAULT_WARN_THRESHOLD = 10;
export const DEFAULT_CRITICAL_THRESHOLD = 50;

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
  /** Degradation level derived from the current lag. */
  level: DegradedLevel;
  /** True when level is "warn" or "critical". */
  isDegraded: boolean;
  /** True only when level is "critical". */
  isCritical: boolean;
  /** ISO-8601 timestamp of this snapshot. */
  checkedAt: string;
}

// ---------------------------------------------------------------------------
// LagMonitor
// ---------------------------------------------------------------------------

export class LagMonitor {
  private static instance: LagMonitor;

  private _warnThreshold: number;
  private _criticalThreshold: number;

  constructor(warnThreshold?: number, criticalThreshold?: number) {
    this._warnThreshold =
      warnThreshold !== undefined
        ? warnThreshold
        : (parseInt(process.env["LAG_WARN_THRESHOLD"] ?? "", 10) || DEFAULT_WARN_THRESHOLD);

    this._criticalThreshold =
      criticalThreshold !== undefined
        ? criticalThreshold
        : (parseInt(process.env["LAG_CRITICAL_THRESHOLD"] ?? "", 10) || DEFAULT_CRITICAL_THRESHOLD);
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

  // -------------------------------------------------------------------------
  // Core computation
  // -------------------------------------------------------------------------

  /**
   * Computes the current lag level from a raw lag value.
   * Pure function — no I/O.
   */
  computeLevel(lag: number): DegradedLevel {
    if (lag >= this._criticalThreshold) return "critical";
    if (lag >= this._warnThreshold) return "warn";
    return "none";
  }

  /**
   * Fetches the current system status and returns a full LagStatus snapshot.
   */
  async getLagStatus(): Promise<LagStatus> {
    const status = await statusService.getStatus();
    const lag = status.index_lag;
    const level = this.computeLevel(lag);

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
}

export const lagMonitor = LagMonitor.getInstance();

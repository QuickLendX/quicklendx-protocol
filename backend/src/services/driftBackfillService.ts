/**
 * driftBackfillService.ts
 *
 * Manages the lifecycle of drift-recovery backfill runs that re-index
 * historical on-chain data into the local store, with a gated operator
 * recovery flow (PR #1391).
 *
 * NOTE: This is distinct from src/services/backfillService.ts, which manages
 * ledger-range backfill runs (startBackfill / triggerDriftBackfill) backed by
 * SQLite. Both predate this feature and remain in use; this module is the
 * drift-severity recovery lifecycle that the AlertRouter / drift-severity
 * worker depend on. It is exported as `BackfillService` to match the
 * drift-severity test's expectations.
 *
 * Key guarantees:
 *  - `pauseRun` is idempotent: calling it on an already-paused run is safe and
 *    does not throw.
 *  - `resumeRun` enforces an operator gate: a paused run can only be resumed
 *    after the triggering alert has been acknowledged in the AlertRouter.
 */

import { BackfillRun, BackfillRunStatus } from "../types/driftSeverity";
import { AlertRouter, alertRouter as defaultAlertRouter } from "./alertRouter";

// ---------------------------------------------------------------------------
// BackfillService (singleton)
// ---------------------------------------------------------------------------

export class BackfillService {
  private static instance: BackfillService | undefined;

  /** In-memory run store keyed by runId. Replace with DB in production. */
  private readonly runs: Map<string, BackfillRun> = new Map();

  private alertRouter: AlertRouter;

  private constructor(router: AlertRouter = defaultAlertRouter) {
    this.alertRouter = router;
  }

  // --------------------------------------------------------------------------
  // Singleton access
  // --------------------------------------------------------------------------

  public static getInstance(router?: AlertRouter): BackfillService {
    if (!BackfillService.instance) {
      BackfillService.instance = new BackfillService(router);
    }
    return BackfillService.instance;
  }

  /**
   * Replaces the singleton instance. Used only in tests to obtain a fresh
   * service without carrying state across test cases.
   */
  public static resetInstance(): void {
    BackfillService.instance = undefined;
  }

  // --------------------------------------------------------------------------
  // Run management
  // --------------------------------------------------------------------------

  /**
   * Registers a new run. Throws if a run with the same id already exists.
   */
  public createRun(runId: string): BackfillRun {
    if (this.runs.has(runId)) {
      throw new Error(`Run already exists: ${runId}`);
    }
    const run: BackfillRun = {
      runId,
      status: BackfillRunStatus.Running,
      alertAcknowledged: false,
    };
    this.runs.set(runId, run);
    return run;
  }

  /**
   * Returns the run for the given id, or undefined when not found.
   */
  public getRun(runId: string): BackfillRun | undefined {
    return this.runs.get(runId);
  }

  // --------------------------------------------------------------------------
  // Pause (idempotent)
  // --------------------------------------------------------------------------

  /**
   * Pauses an active backfill run.
   *
   * - If the run is already Paused this is a no-op (idempotent).
   * - If the run does not exist it is auto-registered as Paused so callers
   *   that create runs lazily are handled gracefully.
   * - If the run is Completed or Failed the pause is silently ignored because
   *   it is meaningless to pause a terminal run.
   */
  public async pauseRun(runId: string, reason: string): Promise<void> {
    let run = this.runs.get(runId);

    if (!run) {
      // Lazy-create: the reconciliation worker may trigger a pause before
      // explicitly registering the run.
      run = {
        runId,
        status: BackfillRunStatus.Paused,
        pauseReason: reason,
        alertAcknowledged: false,
      };
      this.runs.set(runId, run);
      return;
    }

    // Already paused - idempotent, do nothing
    if (run.status === BackfillRunStatus.Paused) {
      return;
    }

    // Terminal states - ignore
    if (
      run.status === BackfillRunStatus.Completed ||
      run.status === BackfillRunStatus.Failed
    ) {
      return;
    }

    // Transition Running -> Paused
    run.status = BackfillRunStatus.Paused;
    run.pauseReason = reason;
    run.alertAcknowledged = false;
  }

  // --------------------------------------------------------------------------
  // Alert acknowledgement gate
  // --------------------------------------------------------------------------

  /**
   * Records that the operator has acknowledged the alert that caused the pause.
   * This must be called (via the AlertRouter) before `resumeRun` will succeed.
   *
   * The `alertKey` must reference an alert that is acknowledged in the
   * AlertRouter. This double-check prevents operators from calling this
   * directly without going through the proper acknowledgement flow.
   */
  public markAlertAcknowledged(runId: string, alertKey: string): void {
    const run = this.runs.get(runId);
    if (!run) {
      throw new Error(`Run not found: ${runId}`);
    }
    if (run.status !== BackfillRunStatus.Paused) {
      throw new Error(
        `Cannot acknowledge alert for run that is not paused (status: ${run.status})`
      );
    }

    // Verify the alert is actually acknowledged in the router
    const alert = this.alertRouter.getAlert(alertKey);
    if (!alert) {
      throw new Error(`Alert not found in router: ${alertKey}`);
    }
    if (this.alertRouter.hasOpenAlert(alertKey)) {
      throw new Error(
        `Alert "${alertKey}" has not been acknowledged yet; acknowledge it first`
      );
    }

    run.alertAcknowledged = true;
  }

  // --------------------------------------------------------------------------
  // Resume (gated)
  // --------------------------------------------------------------------------

  /**
   * Resumes a paused backfill run.
   *
   * @throws when the run is not found.
   * @throws when the run is not in the Paused state.
   * @throws when the operator has not acknowledged the triggering alert.
   */
  public async resumeRun(runId: string): Promise<void> {
    const run = this.runs.get(runId);
    if (!run) {
      throw new Error(`Run not found: ${runId}`);
    }
    if (run.status !== BackfillRunStatus.Paused) {
      throw new Error(
        `Cannot resume run that is not paused (current status: ${run.status})`
      );
    }
    if (!run.alertAcknowledged) {
      throw new Error(
        `Cannot resume run "${runId}": the triggering alert has not been acknowledged`
      );
    }

    run.status = BackfillRunStatus.Running;
    run.pauseReason = undefined;
    run.alertAcknowledged = false;
  }

  // --------------------------------------------------------------------------
  // Terminal transitions
  // --------------------------------------------------------------------------

  /** Marks a run as completed. */
  public async completeRun(runId: string): Promise<void> {
    const run = this.runs.get(runId);
    if (!run) {
      throw new Error(`Run not found: ${runId}`);
    }
    run.status = BackfillRunStatus.Completed;
  }

  /** Marks a run as failed. */
  public async failRun(runId: string, reason: string): Promise<void> {
    const run = this.runs.get(runId);
    if (!run) {
      throw new Error(`Run not found: ${runId}`);
    }
    run.status = BackfillRunStatus.Failed;
    run.pauseReason = reason;
  }

  // --------------------------------------------------------------------------
  // Test helpers
  // --------------------------------------------------------------------------

  /** Clears all runs. Use only in tests. */
  public clearRuns(): void {
    this.runs.clear();
  }
}

export const driftBackfillService = BackfillService.getInstance();

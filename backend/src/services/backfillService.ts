import path from "path";
import { promises as fs } from "fs";
import {
  BackfillRun,
  BackfillStartRequest,
  BackfillPreview,
  BackfillAuditEntry,
} from "../types/backfill";

const DEFAULT_MAX_LEDGER_RANGE = 5000;
const DEFAULT_MAX_CONCURRENCY = 4;
const CHUNK_SIZE_PER_WORKER = 25;

class BackfillError extends Error {
  public readonly code: string;
  public readonly statusCode: number;

  constructor(message: string, code: string, statusCode: number) {
    super(message);
    this.code = code;
    this.statusCode = statusCode;
  }
}

export class BackfillService {
  private static instance: BackfillService;
  private readonly runs = new Map<string, BackfillRun>();
  private readonly idempotencyIndex = new Map<string, string>();
  private readonly runTimers = new Map<string, NodeJS.Timeout>();
  private readonly auditLogPath: string;
  private failureAtLedger: number | null = null;

  private constructor() {
    this.auditLogPath =
      process.env.BACKFILL_AUDIT_LOG_PATH ||
      path.resolve(process.cwd(), ".data", "backfill-audit-log.jsonl");
  }

  public static getInstance(): BackfillService {
    if (!BackfillService.instance) {
      BackfillService.instance = new BackfillService();
    }
    return BackfillService.instance;
  }

  public async startBackfill(
    payload: BackfillStartRequest,
    actor: string,
  ): Promise<{ run?: BackfillRun; preview: BackfillPreview; idempotentReuse?: boolean }> {
    this.validateRequest(payload);
    const preview = this.buildPreview(payload.startLedger, payload.endLedger, payload.concurrency);

    if (payload.dryRun) {
      await this.appendAuditEntry({
        runId: "dry-run",
        timestamp: new Date().toISOString(),
        eventType: "preview",
        actor,
        metadata: {
          startLedger: payload.startLedger,
          endLedger: payload.endLedger,
          concurrency: payload.concurrency,
          totalLedgers: preview.range.totalLedgers,
          estimatedAffectedRecords: preview.estimatedAffectedRecords,
        },
      });

      return { preview };
    }

    if (payload.idempotencyKey && this.idempotencyIndex.has(payload.idempotencyKey)) {
      const runId = this.idempotencyIndex.get(payload.idempotencyKey)!;
      const run = this.runs.get(runId);
      if (run) {
        await this.appendAuditEntry({
          runId,
          timestamp: new Date().toISOString(),
          eventType: "idempotent_reuse",
          actor,
          metadata: { idempotencyKey: payload.idempotencyKey },
        });
        return { run: { ...run }, preview, idempotentReuse: true };
      }
    }

    const now = new Date().toISOString();
    const runId = this.createRunId();
    const run: BackfillRun = {
      id: runId,
      startLedger: payload.startLedger,
      endLedger: payload.endLedger,
      dryRun: false,
      concurrency: payload.concurrency,
      status: "running",
      processedLedgers: 0,
      cursorLedger: payload.startLedger,
      actor,
      createdAt: now,
      updatedAt: now,
      idempotencyKey: payload.idempotencyKey,
    };

    this.runs.set(runId, run);
    if (payload.idempotencyKey) {
      this.idempotencyIndex.set(payload.idempotencyKey, runId);
    }

    await this.appendAuditEntry({
      runId,
      timestamp: now,
      eventType: "started",
      actor,
      metadata: {
        startLedger: payload.startLedger,
        endLedger: payload.endLedger,
        concurrency: payload.concurrency,
      },
    });

    this.scheduleNextTick(runId);
    return { run: { ...run }, preview };
  }

  public async pauseRun(runId: string, actor: string): Promise<BackfillRun> {
    const run = this.getRunOrThrow(runId);
    if (run.status !== "running") {
      throw new BackfillError("Run is not currently running", "RUN_NOT_RUNNING", 409);
    }

    run.status = "paused";
    run.updatedAt = new Date().toISOString();
    this.clearRunTimer(runId);
    await this.appendAuditEntry({
      runId,
      timestamp: run.updatedAt,
      eventType: "paused",
      actor,
      metadata: { cursorLedger: run.cursorLedger },
    });

    return { ...run };
  }

  public async resumeRun(runId: string, actor: string): Promise<BackfillRun> {
    const run = this.getRunOrThrow(runId);
    if (run.status !== "paused" && run.status !== "failed") {
      throw new BackfillError("Only paused or failed runs can be resumed", "RUN_NOT_RESUMABLE", 409);
    }

    run.status = "running";
    run.error = undefined;
    run.updatedAt = new Date().toISOString();
    await this.appendAuditEntry({
      runId,
      timestamp: run.updatedAt,
      eventType: "resumed",
      actor,
      metadata: { cursorLedger: run.cursorLedger },
    });
    this.scheduleNextTick(runId);

    return { ...run };
  }

  public getRun(runId: string): BackfillRun | null {
    const run = this.runs.get(runId);
    return run ? { ...run } : null;
  }

  public listRuns(): BackfillRun[] {
    return [...this.runs.values()].map((run) => ({ ...run }));
  }

  public async resetForTests(): Promise<void> {
    this.runTimers.forEach((timer) => clearTimeout(timer));
    this.runTimers.clear();
    this.runs.clear();
    this.idempotencyIndex.clear();
    this.failureAtLedger = null;
    try {
      await fs.rm(this.auditLogPath, { force: true });
    } catch {
      await fs.mkdir(path.dirname(this.auditLogPath), { recursive: true });
      await fs.writeFile(this.auditLogPath, "", "utf8");
    }
  }

  public setFailureAtLedgerForTests(ledger: number | null): void {
    this.failureAtLedger = ledger;
  }

  private validateRequest(payload: BackfillStartRequest): void {
    if (payload.endLedger < payload.startLedger) {
      throw new BackfillError("endLedger must be >= startLedger", "INVALID_LEDGER_RANGE", 400);
    }

    const maxRange = this.getMaxRange();
    const totalLedgers = payload.endLedger - payload.startLedger + 1;
    if (totalLedgers > maxRange) {
      throw new BackfillError(
        `Requested range exceeds maximum of ${maxRange} ledgers`,
        "MAX_RANGE_EXCEEDED",
        422,
      );
    }

    const maxConcurrency = this.getMaxConcurrency();
    if (payload.concurrency > maxConcurrency) {
      throw new BackfillError(
        `Requested concurrency exceeds maximum of ${maxConcurrency}`,
        "MAX_CONCURRENCY_EXCEEDED",
        422,
      );
    }
  }

  private buildPreview(startLedger: number, endLedger: number, concurrency: number): BackfillPreview {
    const totalLedgers = endLedger - startLedger + 1;
    return {
      range: { startLedger, endLedger, totalLedgers },
      // Conservative estimate to show scope before executing.
      estimatedAffectedRecords: totalLedgers * 3,
      concurrency,
    };
  }

  private scheduleNextTick(runId: string): void {
    this.clearRunTimer(runId);
    const timer = setTimeout(() => {
      void this.processRun(runId);
    }, 0);
    this.runTimers.set(runId, timer);
  }

  private clearRunTimer(runId: string): void {
    const timer = this.runTimers.get(runId);
    if (timer) {
      clearTimeout(timer);
      this.runTimers.delete(runId);
    }
  }

  private async processRun(runId: string): Promise<void> {
    const run = this.runs.get(runId);
    if (!run || run.status !== "running") {
      return;
    }

    try {
      const chunkSize = run.concurrency * CHUNK_SIZE_PER_WORKER;
      const chunkEnd = Math.min(run.cursorLedger + chunkSize - 1, run.endLedger);

      if (this.failureAtLedger !== null && this.failureAtLedger >= run.cursorLedger && this.failureAtLedger <= chunkEnd) {
        throw new Error(`Simulated processing failure at ledger ${this.failureAtLedger}`);
      }

      const processedInChunk = chunkEnd - run.cursorLedger + 1;
      run.processedLedgers += processedInChunk;
      run.cursorLedger = chunkEnd + 1;
      run.updatedAt = new Date().toISOString();

      if (run.cursorLedger > run.endLedger) {
        run.status = "completed";
        run.completedAt = run.updatedAt;
        this.clearRunTimer(runId);
        await this.appendAuditEntry({
          runId,
          timestamp: run.updatedAt,
          eventType: "completed",
          actor: run.actor,
          metadata: { processedLedgers: run.processedLedgers },
        });
        return;
      }

      this.scheduleNextTick(runId);
    } catch (error) {
      run.status = "failed";
      run.error = error instanceof Error ? error.message : "Unknown backfill failure";
      run.updatedAt = new Date().toISOString();
      this.clearRunTimer(runId);
      await this.appendAuditEntry({
        runId,
        timestamp: run.updatedAt,
        eventType: "failed",
        actor: run.actor,
        metadata: { error: run.error, cursorLedger: run.cursorLedger },
      });
    }
  }

  private getRunOrThrow(runId: string): BackfillRun {
    const run = this.runs.get(runId);
    if (!run) {
      throw new BackfillError("Backfill run not found", "RUN_NOT_FOUND", 404);
    }
    return run;
  }

  private getMaxRange(): number {
    const raw = process.env.BACKFILL_MAX_LEDGER_RANGE;
    const parsed = raw ? Number(raw) : DEFAULT_MAX_LEDGER_RANGE;
    return Number.isFinite(parsed) && parsed > 0 ? Math.floor(parsed) : DEFAULT_MAX_LEDGER_RANGE;
  }

  private getMaxConcurrency(): number {
    const raw = process.env.BACKFILL_MAX_CONCURRENCY;
    const parsed = raw ? Number(raw) : DEFAULT_MAX_CONCURRENCY;
    return Number.isFinite(parsed) && parsed > 0 ? Math.floor(parsed) : DEFAULT_MAX_CONCURRENCY;
  }

  private createRunId(): string {
    return `bf_${Date.now()}_${Math.random().toString(36).slice(2, 10)}`;
  }

  private async appendAuditEntry(entry: BackfillAuditEntry): Promise<void> {
    await fs.mkdir(path.dirname(this.auditLogPath), { recursive: true });
    await fs.appendFile(this.auditLogPath, `${JSON.stringify(entry)}\n`, "utf8");
  }
}

export const backfillService = BackfillService.getInstance();
export { BackfillError };

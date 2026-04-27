import path from "path";
import { promises as fs } from "fs";
import { eventProcessor } from "./eventProcessor";
import { DefaultEventValidator } from "./eventValidator";
import { InMemoryRawEventStore } from "./rawEventStore";
import { InMemoryDerivedTableStore } from "./derivedTableStore";
import { 
  ReplayRun, 
  ReplayStartRequest, 
  ReplayPreview, 
  ReplayAuditEntry,
  ReplayStats,
  RawEventStore,
  DerivedTableStore,
  ReplayRunStatus,
  ReplayAuditEventType
} from "../types/replay";

export class ReplayError extends Error {
  public readonly code: string;
  public readonly statusCode: number;

  constructor(message: string, code: string, statusCode: number) {
    super(message);
    this.code = code;
    this.statusCode = statusCode;
  }
}

export class ReplayService {
  private static instance: ReplayService;
  private readonly runs = new Map<string, ReplayRun>();
  private readonly idempotencyIndex = new Map<string, string>();
  private readonly runTimers = new Map<string, NodeJS.Timeout>();
  private readonly auditLogPath: string;
  private failureAtLedger: number | null = null;

  private constructor(
    private readonly rawEventStore: RawEventStore,
    private readonly derivedTableStore: DerivedTableStore
  ) {
    this.auditLogPath =
      process.env.REPLAY_AUDIT_LOG_PATH ||
      path.resolve(process.cwd(), ".data", "replay-audit-log.jsonl");
  }

  public static getInstance(
    rawEventStore: RawEventStore,
    derivedTableStore: DerivedTableStore
  ): ReplayService {
    if (!ReplayService.instance) {
      ReplayService.instance = new ReplayService(rawEventStore, derivedTableStore);
    }
    return ReplayService.instance;
  }

  public async startReplay(
    payload: ReplayStartRequest,
    actor: string
  ): Promise<{ run?: ReplayRun; preview: ReplayPreview; idempotentReuse?: boolean }> {
    this.validateRequest(payload);
    const preview = await this.buildPreview(payload);

    if (payload.dryRun) {
      await this.appendAuditEntry({
        runId: "dry-run",
        timestamp: new Date().toISOString(),
        eventType: "started",
        actor,
        metadata: {
          fromLedger: payload.fromLedger,
          toLedger: payload.toLedger,
          batchSize: payload.batchSize,
          forceRebuild: payload.forceRebuild,
          estimatedEvents: preview.estimatedEvents,
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
    const run: ReplayRun = {
      id: runId,
      fromLedger: payload.fromLedger,
      toLedger: payload.toLedger,
      dryRun: false,
      batchSize: payload.batchSize,
      forceRebuild: payload.forceRebuild,
      status: "pending",
      processedEvents: 0,
      cursorLedger: payload.fromLedger,
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
        fromLedger: payload.fromLedger,
        toLedger: payload.toLedger,
        batchSize: payload.batchSize,
        forceRebuild: payload.forceRebuild,
      },
    });

    // Start processing after a small delay to allow response to be sent
    setTimeout(() => void this.processRun(runId), 10);

    return { run: { ...run }, preview };
  }

  public async pauseRun(runId: string, actor: string): Promise<ReplayRun> {
    const run = this.getRunOrThrow(runId);
    if (run.status !== "running") {
      throw new ReplayError("Run is not currently running", "RUN_NOT_RUNNING", 409);
    }

    run.status = "paused";
    run.updatedAt = new Date().toISOString();
    this.clearRunTimer(runId);
    await this.appendAuditEntry({
      runId,
      timestamp: run.updatedAt,
      eventType: "paused",
      actor,
      metadata: { cursorLedger: run.cursorLedger, processedEvents: run.processedEvents },
    });

    return { ...run };
  }

  public async resumeRun(runId: string, actor: string): Promise<ReplayRun> {
    const run = this.getRunOrThrow(runId);
    if (run.status !== "paused" && run.status !== "failed") {
      throw new ReplayError("Only paused or failed runs can be resumed", "RUN_NOT_RESUMABLE", 409);
    }

    run.status = "running";
    run.error = undefined;
    run.validationErrors = undefined;
    run.updatedAt = new Date().toISOString();
    await this.appendAuditEntry({
      runId,
      timestamp: run.updatedAt,
      eventType: "resumed",
      actor,
      metadata: { cursorLedger: run.cursorLedger, processedEvents: run.processedEvents },
    });

    setTimeout(() => void this.processRun(runId), 10);

    return { ...run };
  }

  public getRun(runId: string): ReplayRun | null {
    const run = this.runs.get(runId);
    return run ? { ...run } : null;
  }

  public listRuns(): ReplayRun[] {
    return [...this.runs.values()].map((run) => ({ ...run }));
  }

  public async getStats(runId: string): Promise<ReplayStats> {
    const run = this.getRunOrThrow(runId);
    const totalEvents = await this.rawEventStore.getEventCount(run.fromLedger, run.toLedger);
    
    const estimatedCompletion = run.status === "running" && run.processedEvents > 0
      ? new Date(Date.now() + ((run.toLedger - run.cursorLedger) * 1000)).toISOString()
      : undefined;

    return {
      totalEvents,
      processedEvents: run.processedEvents,
      failedEvents: 0, // Track failures in future implementation
      skippedEvents: 0, // Track skips in future implementation
      currentLedger: run.cursorLedger,
      estimatedCompletion,
    };
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

  private validateRequest(payload: ReplayStartRequest): void {
    if (payload.toLedger < payload.fromLedger) {
      throw new ReplayError("toLedger must be >= fromLedger", "INVALID_LEDGER_RANGE", 400);
    }

    const maxRange = this.getMaxRange();
    const totalLedgers = payload.toLedger - payload.fromLedger + 1;
    if (totalLedgers > maxRange) {
      throw new ReplayError(
        `Requested range exceeds maximum of ${maxRange} ledgers`,
        "MAX_RANGE_EXCEEDED",
        422,
      );
    }

    const maxBatchSize = this.getMaxBatchSize();
    if (payload.batchSize > maxBatchSize) {
      throw new ReplayError(
        `Requested batch size exceeds maximum of ${maxBatchSize}`,
        "MAX_BATCH_SIZE_EXCEEDED",
        422,
      );
    }
  }

  private async buildPreview(payload: ReplayStartRequest): Promise<ReplayPreview> {
    const totalLedgers = payload.toLedger - payload.fromLedger + 1;
    const estimatedEvents = await this.rawEventStore.getEventCount(payload.fromLedger, payload.toLedger);

    return {
      range: {
        fromLedger: payload.fromLedger,
        toLedger: payload.toLedger,
        totalLedgers,
      },
      estimatedEvents,
      batchSize: payload.batchSize,
      forceRebuild: payload.forceRebuild,
    };
  }

  private async processRun(runId: string): Promise<void> {
    const run = this.runs.get(runId);
    if (!run || run.status !== "running") {
      return;
    }

    try {
      run.status = "running";
      run.updatedAt = new Date().toISOString();

      // Clear derived tables if force rebuild is requested
      if (run.forceRebuild && run.cursorLedger === run.fromLedger) {
        await this.derivedTableStore.clearDerivedTables();
      }

      // Get next batch of events
      const toLedger = Math.min(run.cursorLedger + run.batchSize - 1, run.toLedger);
      const events = await this.rawEventStore.getEventsByLedgerRange(
        run.cursorLedger, 
        toLedger, 
        run.batchSize
      );

      if (events.length === 0) {
        // No more events to process
        run.status = "completed";
        run.completedAt = new Date().toISOString();
        this.clearRunTimer(runId);
        await this.appendAuditEntry({
          runId,
          timestamp: run.completedAt,
          eventType: "completed",
          actor: run.actor,
          metadata: { processedEvents: run.processedEvents, finalLedger: run.cursorLedger - 1 },
        });
        return;
      }

      // Simulate failure for testing
      if (this.failureAtLedger !== null && this.failureAtLedger >= run.cursorLedger && this.failureAtLedger <= toLedger) {
        throw new Error(`Simulated processing failure at ledger ${this.failureAtLedger}`);
      }

      // Process events in a transaction
      await this.derivedTableStore.beginTransaction();
      
      try {
        for (const event of events) {
          await eventProcessor.processEvent(event);
          run.processedEvents++;
        }
        
        await this.derivedTableStore.commitTransaction();
        
        // Update cursor
        run.cursorLedger = toLedger + 1;
        run.updatedAt = new Date().toISOString();
        
        // Store replay cursor for checkpointing
        await this.rawEventStore.setReplayCursor(run.cursorLedger);

      } catch (processingError) {
        await this.derivedTableStore.rollbackTransaction();
        throw processingError;
      }

      // Schedule next batch
      if (run.cursorLedger <= run.toLedger) {
        setTimeout(() => void this.processRun(runId), 0);
      } else {
        // Completed
        run.status = "completed";
        run.completedAt = new Date().toISOString();
        this.clearRunTimer(runId);
        await this.appendAuditEntry({
          runId,
          timestamp: run.completedAt,
          eventType: "completed",
          actor: run.actor,
          metadata: { processedEvents: run.processedEvents },
        });
      }

    } catch (error) {
      run.status = "failed";
      run.error = error instanceof Error ? error.message : "Unknown replay failure";
      run.updatedAt = new Date().toISOString();
      this.clearRunTimer(runId);
      await this.appendAuditEntry({
        runId,
        timestamp: run.updatedAt,
        eventType: "failed",
        actor: run.actor,
        metadata: { error: run.error, cursorLedger: run.cursorLedger, processedEvents: run.processedEvents },
      });
    }
  }

  private getRunOrThrow(runId: string): ReplayRun {
    const run = this.runs.get(runId);
    if (!run) {
      throw new ReplayError("Replay run not found", "RUN_NOT_FOUND", 404);
    }
    return run;
  }

  private getMaxRange(): number {
    const raw = process.env.REPLAY_MAX_LEDGER_RANGE;
    const parsed = raw ? Number(raw) : 100000; // 100k ledgers default
    return Number.isFinite(parsed) && parsed > 0 ? Math.floor(parsed) : 100000;
  }

  private getMaxBatchSize(): number {
    const raw = process.env.REPLAY_MAX_BATCH_SIZE;
    const parsed = raw ? Number(raw) : 1000; // 1000 events default
    return Number.isFinite(parsed) && parsed > 0 ? Math.floor(parsed) : 1000;
  }

  private createRunId(): string {
    return `rp_${Date.now()}_${Math.random().toString(36).slice(2, 10)}`;
  }

  private clearRunTimer(runId: string): void {
    const timer = this.runTimers.get(runId);
    if (timer) {
      clearTimeout(timer);
      this.runTimers.delete(runId);
    }
  }

  private async appendAuditEntry(entry: ReplayAuditEntry): Promise<void> {
    await fs.mkdir(path.dirname(this.auditLogPath), { recursive: true });
    await fs.appendFile(this.auditLogPath, `${JSON.stringify(entry)}\n`, "utf8");
  }
}

// Create singleton instances for the application
const eventValidator = new DefaultEventValidator();
const rawEventStore = new InMemoryRawEventStore(eventValidator);
const derivedTableStore = new InMemoryDerivedTableStore();
export const replayService = ReplayService.getInstance(rawEventStore, derivedTableStore);

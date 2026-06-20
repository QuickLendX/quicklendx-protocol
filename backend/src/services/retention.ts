import path from "path";
import { promises as fs } from "fs";
import { retentionConfig } from "../config";
import { auditService } from "./auditService";
import { ReconciliationWorker } from "./reconciliationWorker";
import { replayService } from "./replayService";
import {
  FileRawEventStore,
  InMemoryRawEventStore,
} from "./rawEventStore";
import type { SnapshotRetentionRecord } from "./snapshotService";
import { DefaultEventValidator } from "./eventValidator";
import { AuditEntry } from "../types/audit";
import { RawEvent } from "../types/replay";

export interface RetentionPolicy {
  rawEventsMs: number;
  auditLogsMs: number;
  snapshotsMs: number;
  batchSize: number;
  intervalMs: number;
  archiveDir: string;
  actor: string;
}

export interface RetentionCategorySummary {
  retentionMs: number;
  cutoff: string;
  scanned: number;
  eligible: number;
  protected: number;
  purged: number;
  archived: number;
  archivePath: string | null;
}

export interface RetentionRunSummary {
  runId: string;
  startedAt: string;
  completedAt: string;
  dryRun: boolean;
  replayActive: boolean;
  reconciliationActive: boolean;
  minimumReplayLedger: number | null;
  rawEvents: RetentionCategorySummary;
  auditLogs: RetentionCategorySummary;
  snapshots: RetentionCategorySummary;
}

export interface RetentionTimer {
  stop(): void;
}

export interface ReplayRetentionInspector {
  getActiveRetentionLock(): {
    active: boolean;
    minimumLedger: number | null;
    runIds: string[];
  };
}

export interface RetentionRawEventStore {
  getAllEvents(): Promise<RawEvent[]>;
  replaceEvents(events: RawEvent[]): Promise<void>;
}

export interface RetentionAuditLogStore {
  getAllEntries(): AuditEntry[];
  replaceEntries(entries: AuditEntry[]): void;
}

export interface RetentionSnapshotStore {
  getAllRetentionRecords(): Promise<SnapshotRetentionRecord[]>;
  replaceRetentionRecords(records: SnapshotRetentionRecord[]): Promise<void>;
}

export interface RetentionDependencies {
  rawEventStore: RetentionRawEventStore;
  auditLogStore: RetentionAuditLogStore;
  snapshotStore: RetentionSnapshotStore;
  auditTrail: {
    append: typeof auditService.append;
  };
  replayInspector: ReplayRetentionInspector;
  reconciliationInspector: {
    isReconciliationRunning(): boolean;
  };
  archiveWriter: (filePath: string, payload: unknown) => Promise<void>;
}

interface CategorizedRetentionState<T> {
  original: T[];
  kept: T[];
  purged: T[];
  protectedCount: number;
  summary: RetentionCategorySummary;
}

function buildDefaultDependencies(): RetentionDependencies {
  const defaultRawEventStore = new FileRawEventStore(new DefaultEventValidator());

  return {
    rawEventStore: defaultRawEventStore,
    auditLogStore: auditService,
    snapshotStore: {
      getAllRetentionRecords: async () => {
        const { SnapshotService } = await import("./snapshotService");
        return SnapshotService.getAllRetentionRecords();
      },
      replaceRetentionRecords: async (records) => {
        const { SnapshotService } = await import("./snapshotService");
        return SnapshotService.replaceRetentionRecords(records);
      },
    },
    auditTrail: auditService,
    replayInspector: replayService,
    reconciliationInspector: ReconciliationWorker,
    archiveWriter: async (filePath, payload) => {
      await fs.mkdir(path.dirname(filePath), { recursive: true });
      await fs.writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`, "utf8");
    },
  };
}

export class RetentionWorker {
  constructor(
    private readonly policy: RetentionPolicy = retentionConfig,
    private readonly dependencies: RetentionDependencies = buildDefaultDependencies()
  ) {}

  async runOnce(options: { dryRun?: boolean; now?: number } = {}): Promise<RetentionRunSummary> {
    const now = options.now ?? Date.now();
    const startedAt = new Date(now).toISOString();
    const runId = `ret_${now}_${Math.random().toString(36).slice(2, 8)}`;
    const dryRun = options.dryRun ?? false;
    const replayLock = this.dependencies.replayInspector.getActiveRetentionLock();
    const reconciliationActive =
      this.dependencies.reconciliationInspector.isReconciliationRunning();

    const [rawEvents, auditEntries, snapshots] = await Promise.all([
      this.dependencies.rawEventStore.getAllEvents(),
      Promise.resolve(this.dependencies.auditLogStore.getAllEntries()),
      this.dependencies.snapshotStore.getAllRetentionRecords(),
    ]);

    const rawState = this.planRawEventRetention(
      rawEvents,
      now,
      replayLock.minimumLedger,
      replayLock.active,
      reconciliationActive
    );
    const auditState = this.planTimestampRetention(
      auditEntries,
      now,
      this.policy.auditLogsMs,
      (entry) => entry.timestamp,
      replayLock.active || reconciliationActive
    );
    const snapshotState = this.planTimestampRetention(
      snapshots,
      now,
      this.policy.snapshotsMs,
      (snapshot) => snapshot.lastUpdated,
      replayLock.active || reconciliationActive
    );

    if (!dryRun) {
      await this.commitRetentionRun(
        runId,
        rawState,
        auditState,
        snapshotState,
        startedAt,
        replayLock.active,
        replayLock.minimumLedger,
        reconciliationActive
      );
    }

    return {
      runId,
      startedAt,
      completedAt: new Date(options.now ?? Date.now()).toISOString(),
      dryRun,
      replayActive: replayLock.active,
      reconciliationActive,
      minimumReplayLedger: replayLock.minimumLedger,
      rawEvents: rawState.summary,
      auditLogs: auditState.summary,
      snapshots: snapshotState.summary,
    };
  }

  start(): RetentionTimer {
    const timer = setInterval(() => {
      void this.runOnce().catch((error) => {
        console.error("[Retention] Scheduled run failed:", error);
      });
    }, this.policy.intervalMs);

    return {
      stop: () => clearInterval(timer),
    };
  }

  private async commitRetentionRun(
    runId: string,
    rawState: CategorizedRetentionState<RawEvent>,
    auditState: CategorizedRetentionState<AuditEntry>,
    snapshotState: CategorizedRetentionState<SnapshotRetentionRecord>,
    startedAt: string,
    replayActive: boolean,
    minimumReplayLedger: number | null,
    reconciliationActive: boolean
  ): Promise<void> {
    const mutated = {
      rawEvents: false,
      auditLogs: false,
      snapshots: false,
    };

    try {
      rawState.summary.archivePath = await this.archiveCategory(
        runId,
        "raw-events",
        rawState.purged
      );
      auditState.summary.archivePath = await this.archiveCategory(
        runId,
        "audit-logs",
        auditState.purged
      );
      snapshotState.summary.archivePath = await this.archiveCategory(
        runId,
        "snapshots",
        snapshotState.purged
      );
      rawState.summary.archived = rawState.purged.length;
      auditState.summary.archived = auditState.purged.length;
      snapshotState.summary.archived = snapshotState.purged.length;

      if (rawState.purged.length > 0) {
        await this.dependencies.rawEventStore.replaceEvents(rawState.kept);
        mutated.rawEvents = true;
      }

      if (auditState.purged.length > 0) {
        this.dependencies.auditLogStore.replaceEntries(auditState.kept);
        mutated.auditLogs = true;
      }

      if (snapshotState.purged.length > 0) {
        await this.dependencies.snapshotStore.replaceRetentionRecords(snapshotState.kept);
        mutated.snapshots = true;
      }

      const completedAt = new Date().toISOString();
      this.dependencies.auditTrail.append({
        actor: this.policy.actor,
        operation: "RETENTION_RUN",
        params: {
          runId,
          startedAt,
          completedAt,
          replayActive,
          reconciliationActive,
          minimumReplayLedger,
          rawEvents: rawState.summary,
          auditLogs: auditState.summary,
          snapshots: snapshotState.summary,
        },
        redactedParams: {
          runId,
          startedAt,
          completedAt,
          replayActive,
          reconciliationActive,
          minimumReplayLedger,
          rawEvents: rawState.summary,
          auditLogs: auditState.summary,
          snapshots: snapshotState.summary,
        },
        ip: "127.0.0.1",
        userAgent: "retention-worker",
        effect: `Purged ${rawState.summary.purged} raw events, ${auditState.summary.purged} audit logs, and ${snapshotState.summary.purged} snapshots`,
        success: true,
      });
    } catch (error) {
      await this.rollbackRetentionRun(mutated, rawState, auditState, snapshotState);
      throw error;
    }
  }

  private async rollbackRetentionRun(
    mutated: { rawEvents: boolean; auditLogs: boolean; snapshots: boolean },
    rawState: CategorizedRetentionState<RawEvent>,
    auditState: CategorizedRetentionState<AuditEntry>,
    snapshotState: CategorizedRetentionState<SnapshotRetentionRecord>
  ): Promise<void> {
    if (mutated.snapshots) {
      await this.dependencies.snapshotStore.replaceRetentionRecords(snapshotState.original);
    }
    if (mutated.auditLogs) {
      this.dependencies.auditLogStore.replaceEntries(auditState.original);
    }
    if (mutated.rawEvents) {
      await this.dependencies.rawEventStore.replaceEvents(rawState.original);
    }
  }

  private async archiveCategory<T>(runId: string, name: string, rows: T[]): Promise<string | null> {
    if (rows.length === 0) {
      return null;
    }

    const archivePath = path.join(
      this.policy.archiveDir,
      `${runId}-${name}.json`
    );
    await this.dependencies.archiveWriter(archivePath, rows);
    return archivePath;
  }

  private planRawEventRetention(
    events: RawEvent[],
    now: number,
    minimumReplayLedger: number | null,
    replayActive: boolean,
    reconciliationActive: boolean
  ): CategorizedRetentionState<RawEvent> {
    const cutoff = new Date(now - this.policy.rawEventsMs).toISOString();
    const expired = events.filter((event) => event.indexedAt <= cutoff);
    const protectedEvents = expired.filter((event) =>
      event.complianceHold ||
      reconciliationActive ||
      (replayActive && minimumReplayLedger !== null && event.ledger >= minimumReplayLedger)
    );
    const purged = expired
      .filter((event) => !event.complianceHold)
      .filter(
        (event) =>
          !reconciliationActive &&
          !(replayActive && minimumReplayLedger !== null && event.ledger >= minimumReplayLedger)
      )
      .slice(0, this.policy.batchSize);
    const purgedIds = new Set(purged.map((event) => event.id));
    const kept = events.filter((event) => !purgedIds.has(event.id));

    return {
      original: events,
      kept,
      purged,
      protectedCount: protectedEvents.length,
      summary: {
        retentionMs: this.policy.rawEventsMs,
        cutoff,
        scanned: events.length,
        eligible: expired.length,
        protected: protectedEvents.length,
        purged: purged.length,
        archived: 0,
        archivePath: null,
      },
    };
  }

  private planTimestampRetention<T>(
    rows: T[],
    now: number,
    retentionMs: number,
    getTimestamp: (row: T) => string | number,
    protectAllExpired: boolean
  ): CategorizedRetentionState<T> {
    const cutoffDate = new Date(now - retentionMs);
    const cutoff = cutoffDate.toISOString();
    const expired = rows.filter((row) => {
      const rawValue = getTimestamp(row);
      const timestamp =
        typeof rawValue === "number" ? rawValue : new Date(rawValue).getTime();
      return timestamp <= cutoffDate.getTime();
    });
    const purged = protectAllExpired ? [] : expired.slice(0, this.policy.batchSize);
    const purgedIds = new Set(purged);
    const kept = rows.filter((row) => !purgedIds.has(row));

    return {
      original: rows,
      kept,
      purged,
      protectedCount: protectAllExpired ? expired.length : 0,
      summary: {
        retentionMs,
        cutoff,
        scanned: rows.length,
        eligible: expired.length,
        protected: protectAllExpired ? expired.length : 0,
        purged: purged.length,
        archived: 0,
        archivePath: null,
      },
    };
  }
}

export function createRetentionWorker(
  policy: Partial<RetentionPolicy> = {},
  dependencies: Partial<RetentionDependencies> = {}
): RetentionWorker {
  return new RetentionWorker(
    { ...retentionConfig, ...policy },
    { ...buildDefaultDependencies(), ...dependencies }
  );
}

export const retentionWorker = createRetentionWorker();

export function createInMemoryRetentionWorker(
  rawEventStore: InMemoryRawEventStore,
  dependencies: Partial<RetentionDependencies> = {},
  policy: Partial<RetentionPolicy> = {}
): RetentionWorker {
  return createRetentionWorker(policy, {
    rawEventStore,
    ...dependencies,
  });
}

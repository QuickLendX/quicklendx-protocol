import { describe, expect, it, beforeEach, afterAll } from "@jest/globals";
import path from "path";
import { promises as fs } from "fs";
import {
  createRetentionWorker,
  RetentionDependencies,
  RetentionPolicy,
} from "../src/services/retention";
import { RawEvent } from "../src/types/replay";
import { AuditEntry } from "../src/types/audit";
import { SnapshotRetentionRecord } from "../src/services/snapshotService";

const DAY_MS = 24 * 60 * 60 * 1000;
const NOW = Date.parse("2026-05-26T12:00:00.000Z");

function rawEvent(
  id: string,
  ledger: number,
  indexedAtMs: number,
  complianceHold = false
): RawEvent {
  return {
    id,
    ledger,
    txHash: `tx-${id}`,
    type: "InvoiceCreated",
    payload: { invoiceId: id },
    timestamp: indexedAtMs,
    complianceHold,
    indexedAt: new Date(indexedAtMs).toISOString(),
  };
}

function auditEntry(id: string, timestampMs: number): AuditEntry {
  return {
    id,
    timestamp: new Date(timestampMs).toISOString(),
    actor: "tester",
    operation: "CONFIG_CHANGE",
    params: { id },
    redactedParams: { id },
    ip: "127.0.0.1",
    userAgent: "jest",
    effect: "test",
    success: true,
  };
}

function snapshotRecord(
  table: "best_bids" | "top_bids_snapshots",
  invoiceId: string,
  lastUpdated: number
): SnapshotRetentionRecord {
  if (table === "best_bids") {
    return {
      table,
      invoiceId,
      lastUpdated,
      payload: {
        invoice_id: invoiceId,
        bid_id: `bid-${invoiceId}`,
        investor: "investor",
        bid_amount: "1000",
        expected_return: "10",
        timestamp: lastUpdated,
        expiration_timestamp: lastUpdated + 1000,
        block_timestamp: lastUpdated,
        transaction_sequence: 1,
        ledger_index: 1,
        last_updated: lastUpdated,
      },
    };
  }

  return {
    table,
    invoiceId,
    lastUpdated,
    payload: {
      invoice_id: invoiceId,
      top_bids: [{ bid_id: `bid-${invoiceId}`, rank: 1 }],
      last_updated: lastUpdated,
    },
  };
}

class MemoryRawEventStore {
  constructor(public events: RawEvent[] = []) {}

  async getAllEvents(): Promise<RawEvent[]> {
    return [...this.events];
  }

  async replaceEvents(events: RawEvent[]): Promise<void> {
    this.events = [...events];
  }
}

class MemoryAuditLogStore {
  constructor(public entries: AuditEntry[] = []) {}

  getAllEntries(): AuditEntry[] {
    return [...this.entries];
  }

  replaceEntries(entries: AuditEntry[]): void {
    this.entries = [...entries];
  }
}

class MemorySnapshotStore {
  public failReplace = false;

  constructor(public records: SnapshotRetentionRecord[] = []) {}

  async getAllRetentionRecords(): Promise<SnapshotRetentionRecord[]> {
    return [...this.records];
  }

  async replaceRetentionRecords(records: SnapshotRetentionRecord[]): Promise<void> {
    if (this.failReplace) {
      throw new Error("snapshot replace failed");
    }
    this.records = [...records];
  }
}

function makeWorker(overrides: {
  policy?: Partial<RetentionPolicy>;
  replayActive?: boolean;
  minimumReplayLedger?: number | null;
  reconciliationActive?: boolean;
  rawEvents?: RawEvent[];
  auditEntries?: AuditEntry[];
  snapshots?: SnapshotRetentionRecord[];
  failSnapshotReplace?: boolean;
}) {
  const rawStore = new MemoryRawEventStore(overrides.rawEvents ?? []);
  const auditStore = new MemoryAuditLogStore(overrides.auditEntries ?? []);
  const snapshotStore = new MemorySnapshotStore(overrides.snapshots ?? []);
  snapshotStore.failReplace = overrides.failSnapshotReplace ?? false;
  const appendedAudits: Array<Record<string, unknown>> = [];
  const archivedFiles: string[] = [];

  const dependencies: Partial<RetentionDependencies> = {
    rawEventStore: rawStore,
    auditLogStore: auditStore,
    snapshotStore,
    auditTrail: {
      append: (entry) => {
        appendedAudits.push(entry as unknown as Record<string, unknown>);
        return entry as any;
      },
    },
    replayInspector: {
      getActiveRetentionLock: () => ({
        active: overrides.replayActive ?? false,
        minimumLedger: overrides.minimumReplayLedger ?? null,
        runIds: overrides.replayActive ? ["run-1"] : [],
      }),
    },
    reconciliationInspector: {
      isReconciliationRunning: () => overrides.reconciliationActive ?? false,
    },
    archiveWriter: async (filePath) => {
      archivedFiles.push(filePath);
    },
  };

  const policy: Partial<RetentionPolicy> = {
    rawEventsMs: 30 * DAY_MS,
    auditLogsMs: 90 * DAY_MS,
    snapshotsMs: 14 * DAY_MS,
    batchSize: 2,
    intervalMs: 60_000,
    archiveDir: path.join(__dirname, "fixtures", "retention-tests"),
    actor: "system:retention-worker",
    archiveEnabled: true,
    ...overrides.policy,
  };

  return {
    worker: createRetentionWorker(policy, dependencies),
    rawStore,
    auditStore,
    snapshotStore,
    appendedAudits,
    archivedFiles,
  };
}

describe("RetentionWorker", () => {
  beforeEach(async () => {
    jest.restoreAllMocks();
    try {
      await fs.rm(path.join(__dirname, "fixtures", "retention-tests"), { recursive: true, force: true });
    } catch {
      // ignore
    }
  });

  afterAll(async () => {
    try {
      await fs.rm(path.join(__dirname, "fixtures", "retention-tests"), { recursive: true, force: true });
    } catch {
      // ignore
    }
  });

  it("records a zero-purge run and leaves stores unchanged when nothing expired", async () => {
    const { worker, rawStore, auditStore, snapshotStore, appendedAudits, archivedFiles } =
      makeWorker({
        rawEvents: [rawEvent("fresh", 1, NOW - 5 * DAY_MS)],
        auditEntries: [auditEntry("audit-fresh", NOW - 5 * DAY_MS)],
        snapshots: [snapshotRecord("best_bids", "inv-1", NOW - 2 * DAY_MS)],
      });

    const result = await worker.runOnce({ now: NOW });

    expect(result.rawEvents.purged).toBe(0);
    expect(result.auditLogs.purged).toBe(0);
    expect(result.snapshots.purged).toBe(0);
    expect(rawStore.events).toHaveLength(1);
    expect(auditStore.entries).toHaveLength(1);
    expect(snapshotStore.records).toHaveLength(1);
    expect(appendedAudits).toHaveLength(1);
    expect(archivedFiles).toHaveLength(0);
  });

  it("purges records at the TTL boundary but not one millisecond before it", async () => {
    const cutoff = NOW - 30 * DAY_MS;
    const { worker, rawStore } = makeWorker({
      rawEvents: [
        rawEvent("boundary", 10, cutoff),
        rawEvent("before-boundary", 11, cutoff + 1),
      ],
    });

    const result = await worker.runOnce({ now: NOW });

    expect(result.rawEvents.purged).toBe(1);
    expect(rawStore.events.map((event) => event.id)).toEqual(["before-boundary"]);
  });

  it("keeps compliance-held and replay-protected raw events while purging older safe events", async () => {
    const old = NOW - 31 * DAY_MS;
    const { worker, rawStore, appendedAudits } = makeWorker({
      replayActive: true,
      minimumReplayLedger: 50,
      rawEvents: [
        rawEvent("held-kyc", 1, old, true),
        rawEvent("protected-replay", 99, old),
        rawEvent("purge-me", 10, old),
      ],
    });

    const result = await worker.runOnce({ now: NOW });

    expect(result.rawEvents.eligible).toBe(3);
    expect(result.rawEvents.protected).toBe(2);
    expect(result.rawEvents.purged).toBe(1);
    expect(rawStore.events.map((event) => event.id).sort()).toEqual([
      "held-kyc",
      "protected-replay",
    ]);
    expect(appendedAudits).toHaveLength(1);
  });

  it("protects all expired rows during an active reconciliation run", async () => {
    const old = NOW - 120 * DAY_MS;
    const { worker, rawStore, auditStore, snapshotStore } = makeWorker({
      reconciliationActive: true,
      rawEvents: [rawEvent("raw-1", 1, old)],
      auditEntries: [auditEntry("audit-1", old)],
      snapshots: [snapshotRecord("top_bids_snapshots", "inv-1", old)],
    });

    const result = await worker.runOnce({ now: NOW });

    expect(result.rawEvents.protected).toBe(1);
    expect(result.auditLogs.protected).toBe(1);
    expect(result.snapshots.protected).toBe(1);
    expect(rawStore.events).toHaveLength(1);
    expect(auditStore.entries).toHaveLength(1);
    expect(snapshotStore.records).toHaveLength(1);
  });

  it("limits each purge to the configured batch size", async () => {
    const old = NOW - 120 * DAY_MS;
    const { worker, rawStore, auditStore, snapshotStore } = makeWorker({
      rawEvents: [
        rawEvent("raw-1", 1, old),
        rawEvent("raw-2", 2, old),
        rawEvent("raw-3", 3, old),
      ],
      auditEntries: [
        auditEntry("audit-1", old),
        auditEntry("audit-2", old),
        auditEntry("audit-3", old),
      ],
      snapshots: [
        snapshotRecord("best_bids", "inv-1", old),
        snapshotRecord("best_bids", "inv-2", old),
        snapshotRecord("best_bids", "inv-3", old),
      ],
    });

    const result = await worker.runOnce({ now: NOW });

    expect(result.rawEvents.purged).toBe(2);
    expect(result.auditLogs.purged).toBe(2);
    expect(result.snapshots.purged).toBe(2);
    expect(rawStore.events).toHaveLength(1);
    expect(auditStore.entries).toHaveLength(1);
    expect(snapshotStore.records).toHaveLength(1);
  });

  it("rolls back raw events and audit logs if snapshot replacement fails after archiving", async () => {
    const old = NOW - 120 * DAY_MS;
    const { worker, rawStore, auditStore, snapshotStore, appendedAudits, archivedFiles } =
      makeWorker({
        failSnapshotReplace: true,
        rawEvents: [rawEvent("raw-1", 1, old)],
        auditEntries: [auditEntry("audit-1", old)],
        snapshots: [snapshotRecord("best_bids", "inv-1", old)],
      });

    await expect(worker.runOnce({ now: NOW })).rejects.toThrow(
      "snapshot replace failed"
    );

    expect(rawStore.events.map((event) => event.id)).toEqual(["raw-1"]);
    expect(auditStore.entries.map((entry) => entry.id)).toEqual(["audit-1"]);
    expect(snapshotStore.records.map((record) => record.invoiceId)).toEqual(["inv-1"]);
    expect(appendedAudits).toHaveLength(0);
    // Since raw-events now uses gzip archiving and writes directly to files,
    // only audit logs and snapshots use the old archiveWriter.
    expect(archivedFiles).toHaveLength(2);
  });

  it("does not mutate stores or write audit summaries during dry runs", async () => {
    const old = NOW - 120 * DAY_MS;
    const { worker, rawStore, auditStore, snapshotStore, appendedAudits, archivedFiles } =
      makeWorker({
        rawEvents: [rawEvent("raw-1", 1, old)],
        auditEntries: [auditEntry("audit-1", old)],
        snapshots: [snapshotRecord("best_bids", "inv-1", old)],
      });

    const result = await worker.runOnce({ now: NOW, dryRun: true });

    expect(result.dryRun).toBe(true);
    expect(result.rawEvents.purged).toBe(1);
    expect(result.auditLogs.purged).toBe(1);
    expect(result.snapshots.purged).toBe(1);
    expect(result.rawEvents.archived).toBe(0);
    expect(rawStore.events).toHaveLength(1);
    expect(auditStore.entries).toHaveLength(1);
    expect(snapshotStore.records).toHaveLength(1);
    expect(appendedAudits).toHaveLength(0);
    expect(archivedFiles).toHaveLength(0);
  });

  it("uses the strictest default window for raw events", async () => {
    const { worker } = makeWorker({});
    const result = await worker.runOnce({ now: NOW, dryRun: true });

    expect(result.rawEvents.retentionMs).toBeLessThan(result.auditLogs.retentionMs);
    expect(result.snapshots.retentionMs).toBeLessThan(result.auditLogs.retentionMs);
  });
});

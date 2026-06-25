import { describe, expect, it, beforeEach, afterAll } from "@jest/globals";
import path from "path";
import { promises as fs } from "fs";
import * as zlib from "zlib";
import { createHash } from "crypto";
import { createRetentionWorker, RetentionPolicy } from "../src/services/retention";
import { restoreArchivedEvents } from "../scripts/restore-archived-events";
import { RawEvent } from "../src/types/replay";

const DAY_MS = 24 * 60 * 60 * 1000;
const NOW = Date.parse("2026-05-26T12:00:00.000Z");
const TEST_DIR = path.join(__dirname, "fixtures", "archival-restore-tests");

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
    eventIndex: 0,
    type: "InvoiceCreated",
    payload: { invoiceId: id },
    timestamp: indexedAtMs,
    complianceHold,
    indexedAt: new Date(indexedAtMs).toISOString(),
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

  async storeEvents(events: RawEvent[]): Promise<void> {
    this.events.push(...events);
  }
}

describe("Archival and Restoration Integration Tests", () => {
  beforeEach(async () => {
    try {
      await fs.rm(TEST_DIR, { recursive: true, force: true });
    } catch {
      // ignore
    }
  });

  afterAll(async () => {
    try {
      await fs.rm(TEST_DIR, { recursive: true, force: true });
    } catch {
      // ignore
    }
  });

  it("archives expired raw events, skips compliance-hold rows, creates correct checksums, and restores successfully with idempotency", async () => {
    const expiredMs = NOW - 40 * DAY_MS;
    const freshMs = NOW - 10 * DAY_MS;

    const event1 = rawEvent("expired-1", 1, expiredMs); // expired (40 days old), indexedAt: 2026-04-16T12:00:00.000Z
    const event2 = rawEvent("expired-held", 2, expiredMs, true); // expired but complianceHold = true
    const event3 = rawEvent("fresh-3", 3, freshMs); // fresh (10 days old)
    const event4 = rawEvent("expired-4", 4, expiredMs - DAY_MS); // expired (41 days old), indexedAt: 2026-04-15T12:00:00.000Z

    const rawStore = new MemoryRawEventStore([event1, event2, event3, event4]);

    const policy: Partial<RetentionPolicy> = {
      rawEventsMs: 30 * DAY_MS,
      auditLogsMs: 90 * DAY_MS,
      snapshotsMs: 14 * DAY_MS,
      batchSize: 10,
      intervalMs: 60_000,
      archiveDir: TEST_DIR,
      actor: "system:retention-worker",
      archiveEnabled: true,
    };

    const worker = createRetentionWorker(policy, {
      rawEventStore: rawStore,
      replayInspector: {
        getActiveRetentionLock: () => ({ active: false, minimumLedger: null, runIds: [] }),
      },
      reconciliationInspector: {
        isReconciliationRunning: () => false,
      },
    });

    // 1. Run retention worker to archive and cleanup
    const result = await worker.archiveAndCleanupRawEvents({ now: NOW });

    expect(result.purged).toBe(2); // event1, event4 should be purged
    expect(result.archived).toBe(2);

    // Remaining in live store should be fresh-3 and expired-held (skipped)
    expect(rawStore.events.map((e) => e.id).sort()).toEqual(["expired-held", "fresh-3"]);

    // Check that the archive file exists
    const archiveFile = path.join(TEST_DIR, "raw-events-2026-04.jsonl.gz");
    const checksumFile = `${archiveFile}.sha256`;

    const archiveExists = await fs.stat(archiveFile).then(() => true).catch(() => false);
    const checksumExists = await fs.stat(checksumFile).then(() => true).catch(() => false);

    expect(archiveExists).toBe(true);
    expect(checksumExists).toBe(true);

    // Read and verify checksum file content
    const fileBuffer = await fs.readFile(archiveFile);
    const actualHash = createHash("sha256").update(fileBuffer).digest("hex");
    const savedHash = (await fs.readFile(checksumFile, "utf8")).trim();
    expect(actualHash).toBe(savedHash);

    // Verify gunzip content of archive
    const gunzipped = await new Promise<Buffer>((resolve, reject) => {
      zlib.gunzip(fileBuffer, (err, res) => {
        if (err) reject(err);
        else resolve(res);
      });
    });

    const lines = gunzipped.toString("utf8").split("\n").filter((l) => l.trim().length > 0);
    expect(lines).toHaveLength(2);
    const restoredFromGzip = lines.map((l) => JSON.parse(l) as RawEvent);
    expect(restoredFromGzip.map((e) => e.id).sort()).toEqual(["expired-1", "expired-4"]);

    // 2. Restore events using the restore CLI function
    // Restore events in date range
    const restoredCount = await restoreArchivedEvents({
      start: new Date(expiredMs - 2 * DAY_MS).toISOString(),
      end: new Date(expiredMs + DAY_MS).toISOString(),
      archiveDir: TEST_DIR,
      rawEventStore: rawStore,
    });

    expect(restoredCount).toBe(2);
    expect(rawStore.events.map((e) => e.id).sort()).toEqual([
      "expired-1",
      "expired-4",
      "expired-held",
      "fresh-3",
    ]);

    // 3. Verify Idempotency - running restore again should restore 0 events
    const restoredCount2 = await restoreArchivedEvents({
      start: new Date(expiredMs - 2 * DAY_MS).toISOString(),
      end: new Date(expiredMs + DAY_MS).toISOString(),
      archiveDir: TEST_DIR,
      rawEventStore: rawStore,
    });

    expect(restoredCount2).toBe(0);
    expect(rawStore.events).toHaveLength(4);
  });

  it("should fail validation and reject if a checksum mismatch occurs", async () => {
    const expiredMs = NOW - 40 * DAY_MS;
    const event = rawEvent("expired-1", 1, expiredMs);
    const rawStore = new MemoryRawEventStore([event]);

    const policy: Partial<RetentionPolicy> = {
      rawEventsMs: 30 * DAY_MS,
      auditLogsMs: 90 * DAY_MS,
      snapshotsMs: 14 * DAY_MS,
      batchSize: 10,
      intervalMs: 60_000,
      archiveDir: TEST_DIR,
      actor: "system:retention-worker",
      archiveEnabled: true,
    };

    const worker = createRetentionWorker(policy, {
      rawEventStore: rawStore,
      replayInspector: {
        getActiveRetentionLock: () => ({ active: false, minimumLedger: null, runIds: [] }),
      },
      reconciliationInspector: {
        isReconciliationRunning: () => false,
      },
    });

    await worker.archiveAndCleanupRawEvents({ now: NOW });

    const archiveFile = path.join(TEST_DIR, "raw-events-2026-04.jsonl.gz");
    const checksumFile = `${archiveFile}.sha256`;

    // Corrupt the checksum file
    await fs.writeFile(checksumFile, "wrongchecksumvalue", "utf8");

    await expect(
      restoreArchivedEvents({
        start: new Date(expiredMs - 2 * DAY_MS).toISOString(),
        end: new Date(expiredMs + DAY_MS).toISOString(),
        archiveDir: TEST_DIR,
        rawEventStore: rawStore,
      })
    ).rejects.toThrow("Checksum verification failed");
  });

  it("should reject corrupted gzip files during restore", async () => {
    const expiredMs = NOW - 40 * DAY_MS;
    const event = rawEvent("expired-1", 1, expiredMs);
    const rawStore = new MemoryRawEventStore([event]);

    const policy: Partial<RetentionPolicy> = {
      rawEventsMs: 30 * DAY_MS,
      auditLogsMs: 90 * DAY_MS,
      snapshotsMs: 14 * DAY_MS,
      batchSize: 10,
      intervalMs: 60_000,
      archiveDir: TEST_DIR,
      actor: "system:retention-worker",
      archiveEnabled: true,
    };

    const worker = createRetentionWorker(policy, {
      rawEventStore: rawStore,
      replayInspector: {
        getActiveRetentionLock: () => ({ active: false, minimumLedger: null, runIds: [] }),
      },
      reconciliationInspector: {
        isReconciliationRunning: () => false,
      },
    });

    await worker.archiveAndCleanupRawEvents({ now: NOW });

    const archiveFile = path.join(TEST_DIR, "raw-events-2026-04.jsonl.gz");
    const checksumFile = `${archiveFile}.sha256`;

    // Corrupt the gzip file contents
    await fs.writeFile(archiveFile, "corruptedgzipdata", "utf8");

    // Write correct sha256 of the corrupted content to pass checksum verification but fail decompression
    const fileBuffer = await fs.readFile(archiveFile);
    const correctHashForCorrupted = createHash("sha256").update(fileBuffer).digest("hex");
    await fs.writeFile(checksumFile, correctHashForCorrupted, "utf8");

    await expect(
      restoreArchivedEvents({
        start: new Date(expiredMs - 2 * DAY_MS).toISOString(),
        end: new Date(expiredMs + DAY_MS).toISOString(),
        archiveDir: TEST_DIR,
        rawEventStore: rawStore,
      })
    ).rejects.toThrow("Failed to decompress");
  });
});

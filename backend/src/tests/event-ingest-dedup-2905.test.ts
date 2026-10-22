import path from "path";
import crypto from "crypto";
import express from "express";
import request from "supertest";
import { promises as fs } from "fs";
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { closeDatabase, getDatabase } from "../lib/database";
import { EventIdLedger, eventIdLedger } from "../services/eventIdLedger";
import { FileRawEventStore } from "../services/rawEventStore";
import { DefaultEventValidator, SorobanEvent } from "../services/eventValidator";
import { eventProcessor, EventProcessor } from "../services/eventProcessor";
import v1Router, { rawEventStore } from "../routes/v1";

const TEST_DATA_DIR = path.resolve(__dirname, "../../.data");
const TEST_DB_PATH = path.join(
  TEST_DATA_DIR,
  `test-ingest-dedup-${crypto.randomUUID()}.db`
);
const TEST_ARCHIVE_DIR = path.join(
  TEST_DATA_DIR,
  `test-raw-events-${crypto.randomUUID()}`
);

function createSampleEvent(id: string, overrides: Partial<SorobanEvent> = {}): SorobanEvent {
  return {
    id,
    ledger: 100,
    txHash: `0x${crypto.randomBytes(16).toString("hex")}`,
    eventIndex: 0,
    type: "InvoiceSettled",
    payload: {
      invoice_id: `inv-${id}`,
      business: "GBIZ1234567890",
      investor: "GINV1234567890",
      amount: "1000",
    },
    timestamp: Date.now(),
    complianceHold: false,
    indexedAt: new Date().toISOString(),
    ...overrides,
  };
}

describe("Issue #2905: event de-duplication idempotency and bounding", () => {
  let app: express.Express;

  beforeEach(async () => {
    process.env.DATABASE_PATH = TEST_DB_PATH;
    closeDatabase();
    await fs.mkdir(TEST_DATA_DIR, { recursive: true });
    await fs.mkdir(TEST_ARCHIVE_DIR, { recursive: true });

    await eventIdLedger.reset();
    eventProcessor.reset();

    app = express();
    app.use(express.json());
    app.use("/api/v1", v1Router);
  });

  afterEach(async () => {
    vi.restoreAllMocks();
    await rawEventStore.close();
    closeDatabase();
    try {
      if (await fs.stat(TEST_DB_PATH).catch(() => null)) {
        await fs.unlink(TEST_DB_PATH);
      }
      await fs.rm(TEST_ARCHIVE_DIR, { recursive: true, force: true });
    } catch {
      // Best-effort cleanup
    }
  });

  // Acceptance Criterion 1
  it("AC 1: 20 concurrent claim('e1') calls produce exactly one 'claimed' and 19 'duplicate'", async () => {
    const ledger = new EventIdLedger();
    const calls = Array.from({ length: 20 }, () => ledger.claim("e1"));
    const results = await Promise.all(calls);

    const claimedCount = results.filter((r) => r === "claimed").length;
    const duplicateCount = results.filter((r) => r === "duplicate").length;

    expect(claimedCount).toBe(1);
    expect(duplicateCount).toBe(19);
  });

  // Acceptance Criterion 2
  it("AC 2: two concurrent identical batches -> across both responses exactly one processed and one duplicate, processEvent spy callCount === 1", async () => {
    const event = createSampleEvent("event-concurrent-1");
    const batch = [event];

    const processEventSpy = vi
      .spyOn(eventProcessor, "processEvent")
      .mockImplementation(
        async () => new Promise((resolve) => setTimeout(resolve, 30))
      );

    const [res1, res2] = await Promise.all([
      request(app).post("/api/v1/events").send(batch),
      request(app).post("/api/v1/events").send(batch),
    ]);

    expect([res1.status, res2.status]).toContain(200);

    const allResults = [
      ...(res1.body.results || []),
      ...(res2.body.results || []),
    ];
    const eventResults = allResults.filter((r) => r.id === event.id);

    const processed = eventResults.filter((r) => r.status === "processed");
    const duplicates = eventResults.filter((r) => r.status === "duplicate");

    expect(processed).toHaveLength(1);
    expect(duplicates).toHaveLength(1);
    expect(processEventSpy).toHaveBeenCalledTimes(1);
  });

  // Acceptance Criterion 3
  it("AC 3: processEvent throws on first attempt -> marked failed, re-posting same batch -> processed, spy called twice in total", async () => {
    const event = createSampleEvent("event-fail-retry-1");
    const batch = [event];

    let attempts = 0;
    const processEventSpy = vi
      .spyOn(eventProcessor, "processEvent")
      .mockImplementation(async () => {
        attempts++;
        if (attempts === 1) {
          throw new Error("Simulated processing failure");
        }
      });

    // First attempt: should fail
    const res1 = await request(app).post("/api/v1/events").send(batch);
    expect(res1.body.results[0].status).toBe("failed");
    expect(processEventSpy).toHaveBeenCalledTimes(1);

    // Second attempt: should succeed and report processed
    const res2 = await request(app).post("/api/v1/events").send(batch);
    expect(res2.status).toBe(200);
    expect(res2.body.results[0].status).toBe("processed");
    expect(processEventSpy).toHaveBeenCalledTimes(2);
  });

  // Acceptance Criterion 4
  it("AC 4: processEvent succeeds and archive write throws -> marked failed, re-posting -> duplicate, processEvent spy not called again", async () => {
    const event = createSampleEvent("event-archive-fail-1");
    const batch = [event];

    const processEventSpy = vi
      .spyOn(eventProcessor, "processEvent")
      .mockImplementation(async () => {});

    const storeEventsSpy = vi
      .spyOn(rawEventStore, "storeEvents")
      .mockRejectedValueOnce(new Error("Disk full / archive write error"));

    // First attempt: processEvent succeeds, storeEvents throws
    const res1 = await request(app).post("/api/v1/events").send(batch);
    expect(res1.body.results[0].status).toBe("failed");
    expect(processEventSpy).toHaveBeenCalledTimes(1);
    expect(storeEventsSpy).toHaveBeenCalledTimes(1);

    // Second attempt (re-post): claim is completed, so duplicate check stops reprocessing
    const res2 = await request(app).post("/api/v1/events").send(batch);
    expect(res2.status).toBe(200);
    expect(res2.body.results[0].status).toBe("duplicate");
    expect(processEventSpy).toHaveBeenCalledTimes(1); // Not called again!
  });

  // Acceptance Criterion 5
  it("AC 5: after storing 1,000 events with a 1 KB rotation threshold, checking duplicate of first event is still detected", async () => {
    const validator = new DefaultEventValidator();
    // 1 KB rotation threshold to force frequent rotations
    const store = new FileRawEventStore(validator, TEST_ARCHIVE_DIR, 1024);

    const firstEvent = createSampleEvent("first-rotated-event-0");
    const allEvents: SorobanEvent[] = [firstEvent];

    for (let i = 1; i < 1000; i++) {
      allEvents.push(createSampleEvent(`rotated-event-${i}`));
    }

    // Store in chunks so rotation triggers multiple times
    const chunkSize = 50;
    for (let i = 0; i < allEvents.length; i += chunkSize) {
      await store.storeEvents(allEvents.slice(i, i + chunkSize));
    }

    // Verify first event is detected across rotated files
    const hasFirst = await store.hasEventId(firstEvent.id);
    expect(hasFirst).toBe(true);

    // Verify a non-existent event returns false
    const hasNonExistent = await store.hasEventId("does-not-exist");
    expect(hasNonExistent).toBe(false);

    await store.close();
  });

  // Acceptance Criterion 7
  it("AC 7: after 15,000 processed events, eventProcessor retains at most 10,000 log entries, and listEvents() pages through them without error", async () => {
    const processor = new EventProcessor(10000);

    for (let i = 0; i < 15000; i++) {
      await processor.processEvent({
        id: `ev-${i}`,
        type: "InvoiceCreated",
        payload: { invoice_id: `inv-${i}` },
        timestamp: 1000000 + i,
      });
    }

    expect(processor.getEventLogLength()).toBe(10000);

    // Page through all retained events
    let pagedCount = 0;
    let cursor: string | undefined = undefined;
    let pages = 0;

    do {
      const page = await processor.listEvents(cursor, 100);
      pagedCount += page.events.length;
      cursor = page.nextCursor || undefined;
      pages++;
    } while (cursor && pages < 200);

    expect(pagedCount).toBe(10000);
  });
});

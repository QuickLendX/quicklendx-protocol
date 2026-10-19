import path from "path";
import crypto from "crypto";
import express from "express";
import request from "supertest";
import { promises as fs } from "fs";
import { describe, it, expect, beforeAll, afterAll, vi } from "vitest";
import { closeDatabase, getDatabase } from "../../lib/database";
import { ensureProcessedEventsSchema } from "../../services/eventIdLedger";
import { SorobanEvent } from "../../services/eventValidator";
import { eventProcessor } from "../../services/eventProcessor";
import v1Router, { rawEventStore } from "../../routes/v1";

const TEST_DATA_DIR = path.resolve(__dirname, "../../../.data");
const TEST_DB_PATH = path.join(
  TEST_DATA_DIR,
  `perf-idempotency-${crypto.randomUUID()}.db`
);

function createSampleEvent(id: string): SorobanEvent {
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
  };
}

describe("Performance: Ingestion Event Idempotency with 200,000 pre-seeded IDs", () => {
  let app: express.Express;

  beforeAll(async () => {
    await fs.mkdir(TEST_DATA_DIR, { recursive: true });
    process.env.DATABASE_PATH = TEST_DB_PATH;
    closeDatabase();

    const db = getDatabase();
    ensureProcessedEventsSchema(db);

    console.log("Pre-seeding 200,000 event IDs into processed_events...");
    const seedStart = performance.now();
    const insertStmt = db.prepare(`
      INSERT INTO processed_events (id, status, claimed_at, completed_at)
      VALUES (?, 'completed', ?, ?)
    `);

    const seedTx = db.transaction((count: number) => {
      const now = new Date().toISOString();
      for (let i = 0; i < count; i++) {
        insertStmt.run(`seeded-event-${i}`, now, now);
      }
    });

    seedTx(200_000);
    const seedDuration = performance.now() - seedStart;
    console.log(`Pre-seeded 200,000 events in ${seedDuration.toFixed(2)}ms`);

    app = express();
    app.use(express.json());
    app.use("/api/v1", v1Router);
  }, 30000);

  afterAll(async () => {
    await rawEventStore.close();
    closeDatabase();
    try {
      if (await fs.stat(TEST_DB_PATH).catch(() => null)) {
        await fs.unlink(TEST_DB_PATH);
      }
    } catch {
      // Best-effort cleanup
    }
  });

  // Acceptance Criterion 6
  it("AC 6: with 200,000 pre-seeded ids, ingesting a 100-event batch takes less than 250 ms", async () => {
    // 50 duplicate (matching seeded IDs) and 50 fresh events
    const batch: SorobanEvent[] = [];
    for (let i = 0; i < 50; i++) {
      batch.push(createSampleEvent(`seeded-event-${i}`));
    }
    for (let i = 50; i < 100; i++) {
      batch.push(createSampleEvent(`fresh-event-${i}`));
    }

    // Isolate ingest and idempotency performance from external notification side effects
    vi.spyOn(eventProcessor, "processEvent").mockResolvedValue();

    const start = performance.now();
    const res = await request(app).post("/api/v1/events").send(batch);
    const elapsedMs = performance.now() - start;

    console.log(`\n📊 Ingestion Performance:`);
    console.log(`   100-event batch ingested in ${elapsedMs.toFixed(2)}ms (budget: 250ms)`);

    expect(res.status).toBe(200);
    expect(res.body.success).toBe(true);
    expect(res.body.results).toHaveLength(100);

    const duplicates = res.body.results.filter(
      (r: { status: string }) => r.status === "duplicate"
    );
    const processed = res.body.results.filter(
      (r: { status: string }) => r.status === "processed"
    );

    expect(duplicates).toHaveLength(50);
    expect(processed).toHaveLength(50);
    expect(elapsedMs).toBeLessThan(250);
  });
});

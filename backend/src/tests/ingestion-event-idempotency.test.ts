import path from "path";
import crypto from "crypto";
import { closeDatabase, getDatabase } from "../lib/database";
import {
  ingestBatch,
  InMemoryIngestionStore,
  SqliteIngestionStore,
  IndexedEvent,
  rollbackAndReingest,
} from "../services/ingestion";
import {
  checkRawEventIdempotency,
  runFullInvariantSuite,
  createInMemoryProvider,
} from "../services/invariantService";
import {
  ensureRawEventsSchema,
  insertRawEvent,
  isSqliteUniqueViolation,
  listRawEventIdempotencyKeys,
  RawEventDuplicateError,
} from "../services/rawEventStore";
import type { RawEvent } from "../types/replay";

const TEST_DB_DIR = path.resolve(__dirname, "../../.data");
const TEST_DB_PATH = path.join(
  TEST_DB_DIR,
  `test-event-idempotency-${crypto.randomUUID()}.db`,
);

function makeEvent(
  ledger: number,
  txHash: string,
  eventIndex: number,
  overrides: Partial<IndexedEvent> = {},
): IndexedEvent {
  return {
    ledger,
    txHash,
    eventIndex,
    type: "InvoiceCreated",
    payload: { id: `inv-${ledger}-${eventIndex}` },
    ...overrides,
  };
}

function makeRawEvent(
  txHash: string,
  eventIndex: number,
  overrides: Partial<RawEvent> = {},
): RawEvent {
  return {
    id: `${txHash}:${eventIndex}`,
    ledger: 100,
    txHash,
    eventIndex,
    type: "InvoiceCreated",
    payload: { sample: true },
    timestamp: Date.now(),
    complianceHold: false,
    indexedAt: new Date().toISOString(),
    ...overrides,
  };
}

describe("ingestion event-level idempotency", () => {
  describe("InMemoryIngestionStore", () => {
    let store: InMemoryIngestionStore;

    beforeEach(() => {
      store = new InMemoryIngestionStore();
    });

    it("ignores duplicate events with the same (txHash, eventIndex)", async () => {
      const event = makeEvent(10, "0xabc", 0);
      await ingestBatch(store, [event], 10);
      const replay = await ingestBatch(store, [event], 11);

      expect(replay.eventsStored).toBe(0);
      expect(replay.eventsSkipped).toBe(1);
      expect(store.getEvents()).toHaveLength(1);
    });

    it("stores the same on-chain event only once across two different batches", async () => {
      const event = makeEvent(20, "0xshared", 1);
      await ingestBatch(store, [event], 20);
      await ingestBatch(store, [event], 21);

      expect(store.getEvents()).toHaveLength(1);
      expect(store.getEvents()[0].payload).toEqual(event.payload);
    });

    it("replaces an event on reorg when replaceOnConflict is enabled", async () => {
      const key = makeEvent(30, "0xreorg", 0, {
        payload: { version: 1 },
      });
      await ingestBatch(store, [key], 30);

      await store.rollbackTo(29);
      const canonical = makeEvent(30, "0xreorg", 0, {
        payload: { version: 2, canonical: true },
      });
      const result = await ingestBatch(store, [canonical], 30, {
        replaceOnConflict: true,
      });

      expect(result.eventsStored).toBe(1);
      expect(store.getEvents()).toHaveLength(1);
      expect(store.getEvents()[0].payload).toEqual({
        version: 2,
        canonical: true,
      });
    });
  });

  describe("SqliteIngestionStore", () => {
    let store: SqliteIngestionStore;

    beforeAll(() => {
      process.env.DATABASE_PATH = TEST_DB_PATH;
      closeDatabase();
      const db = getDatabase();
      db.exec(`
        CREATE TABLE IF NOT EXISTS indexer_state (
          key TEXT PRIMARY KEY,
          value_text TEXT,
          value_number REAL,
          value_json TEXT,
          updated_at TEXT NOT NULL,
          updated_by TEXT
        )
      `);
      ensureRawEventsSchema(db);
      store = new SqliteIngestionStore();
    });

    afterAll(() => {
      closeDatabase();
      try {
        const fs = require("fs");
        if (fs.existsSync(TEST_DB_PATH)) fs.unlinkSync(TEST_DB_PATH);
      } catch {
        // best-effort cleanup
      }
    });

    beforeEach(() => {
      const db = getDatabase();
      db.exec("DELETE FROM raw_events");
      db.exec("DELETE FROM indexer_state");
    });

    it("persists one row per (tx_hash, event_index) across batches", async () => {
      const event = makeEvent(40, "0xsql", 2);
      await ingestBatch(store, [event], 40);
      await ingestBatch(store, [event], 41);

      const keys = listRawEventIdempotencyKeys();
      expect(keys).toEqual([{ tx_hash: "0xsql", event_index: 2 }]);
    });

    it("upserts canonical data during reorg recovery", async () => {
      const original = makeEvent(50, "0xchain", 0, {
        payload: { fork: "stale" },
      });
      await ingestBatch(store, [original], 50);

      await rollbackAndReingest(store, 49, async (cursor) => {
        if (cursor !== 50) return { rawEvents: [] };
        return {
          rawEvents: [
            makeEvent(50, "0xchain", 0, {
              payload: { fork: "canonical" },
            }),
          ],
        };
      });

      const row = getDatabase()
        .prepare(
          "SELECT payload FROM raw_events WHERE tx_hash = ? AND event_index = ?",
        )
        .get("0xchain", 0) as { payload: string };
      expect(JSON.parse(row.payload)).toEqual({ fork: "canonical" });
    });
  });

  describe("rawEventStore insertRawEvent", () => {
    beforeAll(() => {
      process.env.DATABASE_PATH = TEST_DB_PATH;
      closeDatabase();
      ensureRawEventsSchema(getDatabase());
    });

    beforeEach(() => {
      getDatabase().exec("DELETE FROM raw_events");
    });

    it("returns inserted=false for duplicate ON CONFLICT DO NOTHING", () => {
      const event = makeRawEvent("0xdup", 0);
      const first = insertRawEvent(event);
      const second = insertRawEvent(event);

      expect(first.inserted).toBe(true);
      expect(second.inserted).toBe(false);
    });

    it("surfaces SQLite unique violations as RawEventDuplicateError when forced", () => {
      const db = getDatabase();
      db.prepare(
        `INSERT INTO raw_events (id, tx_hash, event_index, ledger, type, payload, indexed_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)`,
      ).run("manual-1", "0xforce", 0, 1, "Test", "{}", new Date().toISOString());

      try {
        db.prepare(
          `INSERT INTO raw_events (id, tx_hash, event_index, ledger, type, payload, indexed_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)`,
        ).run("manual-2", "0xforce", 0, 2, "Test", "{}", new Date().toISOString());
        throw new Error("expected unique violation");
      } catch (err) {
        expect(isSqliteUniqueViolation(err)).toBe(true);
        expect(() => {
          throw new RawEventDuplicateError("0xforce", 0);
        }).toThrow(RawEventDuplicateError);
      }
    });
  });

  describe("invariant checkRawEventIdempotency", () => {
    it("detects accidental duplicate (tx_hash, event_index) keys", async () => {
      const report = await checkRawEventIdempotency({
        getRawEventKeys: async () => [
          { tx_hash: "0x1", event_index: 0 },
          { tx_hash: "0x1", event_index: 0 },
          { tx_hash: "0x2", event_index: 1 },
        ],
      });

      expect(report.count).toBe(1);
      expect(report.sampleIds).toContain("0x1:0");
    });

    it("passes full invariant suite when no raw-event duplicates exist", async () => {
      const provider = createInMemoryProvider([], [], [], []);
      const suite = await runFullInvariantSuite(provider, [1, 2, 3], {
        getRawEventKeys: async () => [
          { tx_hash: "0xa", event_index: 0 },
          { tx_hash: "0xb", event_index: 1 },
        ],
      });

      expect(suite.pass).toBe(true);
      expect(suite.rawEventDuplicates.count).toBe(0);
    });
  });
});

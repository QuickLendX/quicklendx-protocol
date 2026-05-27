import { describe, it, expect, beforeEach, jest } from "@jest/globals";
import {
  ingestBatch,
  rollbackAndReingest,
  InMemoryIngestionStore,
  IngestionStore,
  IndexedEvent,
} from "../services/ingestion";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeEvent(ledger: number, overrides: Partial<IndexedEvent> = {}): IndexedEvent {
  return {
    ledger,
    txHash: `0x${ledger}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
    type: "test.Event",
    payload: { data: `event_${ledger}` },
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// rollbackTo on InMemoryIngestionStore
// ---------------------------------------------------------------------------

describe("IngestionStore.rollbackTo", () => {
  let store: InMemoryIngestionStore;

  beforeEach(() => {
    store = new InMemoryIngestionStore();
  });

  it("deletes events with ledger > cursor", async () => {
    // Ingest up to cursor 5
    for (let c = 1; c <= 5; c++) {
      await store.commitBatch([makeEvent(c)], c);
    }

    expect(store.getEvents()).toHaveLength(5);
    expect(await store.getCursor()).toBe(5);

    // Rollback to cursor 3
    await store.rollbackTo(3);

    expect(store.getEvents()).toHaveLength(3);
    expect(await store.getCursor()).toBe(3);

    const ledgers = store.getEvents().map(e => e.ledger).sort();
    expect(ledgers).toEqual([1, 2, 3]);
  });

  it("is idempotent", async () => {
    await store.commitBatch([makeEvent(1)], 1);
    await store.commitBatch([makeEvent(2)], 2);

    await store.rollbackTo(1);
    await store.rollbackTo(1); // second call — safe
    await store.rollbackTo(0);
    await store.rollbackTo(0); // again — safe

    expect(store.getEvents()).toHaveLength(0);
    expect(await store.getCursor()).toBe(0);
  });

  it("throws when cursor < 0 (below genesis)", async () => {
    await expect(store.rollbackTo(-1)).rejects.toThrow("below genesis");
    await expect(store.rollbackTo(-100)).rejects.toThrow("below genesis");
  });

  it("clears everything when rolling back to cursor 0", async () => {
    await store.commitBatch([makeEvent(1)], 1);
    await store.commitBatch([makeEvent(2)], 2);

    await store.rollbackTo(0);

    expect(store.getEvents()).toHaveLength(0);
    expect(await store.getCursor()).toBe(0);
  });

  it("rollback with no data is a no-op", async () => {
    // Store is empty, cursor is null
    await store.rollbackTo(5);
    expect(await store.getCursor()).toBe(5);
    expect(store.getEvents()).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// Re-ingestion after rollback
// ---------------------------------------------------------------------------

describe("Re-ingestion after rollback", () => {
  let store: InMemoryIngestionStore;

  beforeEach(() => {
    store = new InMemoryIngestionStore();
  });

  it("re-ingests canonical chain after rollback", async () => {
    // Build chain: cursor 1, 2, 3
    await ingestBatch(store, [makeEvent(1, { payload: { v: 1 } })], 1);
    await ingestBatch(store, [makeEvent(2, { payload: { v: 1 } })], 2);
    await ingestBatch(store, [makeEvent(3, { payload: { v: 1 } })], 3);

    // Simulate reorg: rollback to cursor 1
    await store.rollbackTo(1);

    // Re-ingest canonical chain where events 2 and 3 have new data
    await ingestBatch(store, [makeEvent(2, { payload: { v: 2, canonical: true } })], 2);
    await ingestBatch(store, [makeEvent(3, { payload: { v: 2, canonical: true } })], 3);

    const events = store.getEvents();
    expect(events).toHaveLength(3);
    expect(events[1].payload.canonical).toBe(true);
    expect(events[2].payload.canonical).toBe(true);
    expect(await store.getCursor()).toBe(3);
  });

  it("skips already-committed cursors during re-ingestion", async () => {
    await ingestBatch(store, [makeEvent(1)], 1);
    await store.rollbackTo(0);

    // Re-ingest cursor 1 — should succeed
    const r1 = await ingestBatch(store, [makeEvent(1, { payload: { retry: true } })], 1);
    expect(r1.committed).toBe(true);

    // Re-submit cursor 1 — should skip
    const r2 = await ingestBatch(store, [makeEvent(1)], 1);
    expect(r2.skipped).toBe(true);

    expect(store.getEvents()).toHaveLength(1);
    expect(store.getEvents()[0].payload.retry).toBe(true);
  });

  it("handles multiple rollback + re-ingest cycles", async () => {
    // Ingest 1-3
    await ingestBatch(store, [makeEvent(1)], 1);
    await ingestBatch(store, [makeEvent(2)], 2);
    await ingestBatch(store, [makeEvent(3)], 3);

    // First rollback to 1
    await store.rollbackTo(1);
    await ingestBatch(store, [makeEvent(2, { payload: { cycle: 1 } })], 2);
    await ingestBatch(store, [makeEvent(3, { payload: { cycle: 1 } })], 3);

    // Second rollback to 0
    await store.rollbackTo(0);
    await ingestBatch(store, [makeEvent(1, { payload: { cycle: 2 } })], 1);
    await ingestBatch(store, [makeEvent(2, { payload: { cycle: 2 } })], 2);
    await ingestBatch(store, [makeEvent(3, { payload: { cycle: 2 } })], 3);

    const events = store.getEvents();
    expect(events).toHaveLength(3);
    expect(events[0].payload.cycle).toBe(2);
    expect(events[1].payload.cycle).toBe(2);
    expect(events[2].payload.cycle).toBe(2);
  });
});

// ---------------------------------------------------------------------------
// rollbackAndReingest
// ---------------------------------------------------------------------------

describe("rollbackAndReingest", () => {
  let store: InMemoryIngestionStore;

  beforeEach(() => {
    store = new InMemoryIngestionStore();
  });

  it("rolls back and re-ingests from fetchBatch", async () => {
    // Pre-populate up to cursor 3
    await ingestBatch(store, [makeEvent(1)], 1);
    await ingestBatch(store, [makeEvent(2)], 2);
    await ingestBatch(store, [makeEvent(3)], 3);

    // Simulate fetchBatch that returns canonical events from cursor 2 onward
    const fetchBatch = jest.fn(async (cursor: number) => {
      if (cursor <= 3) {
        return {
          rawEvents: [makeEvent(cursor, { payload: { canonical: true } })],
        };
      }
      return { rawEvents: [] };
    });

    // Rollback to 1 and re-ingest 2 and 3
    const result = await rollbackAndReingest(store, 1, fetchBatch);

    expect(result.newCursor).toBe(3);
    expect(fetchBatch).toHaveBeenCalledTimes(2); // cursors 2 and 3
    expect(store.getEvents()).toHaveLength(3);
    expect(store.getEvents()[1].payload.canonical).toBe(true);
  });

  it("handles empty chain gracefully", async () => {
    const fetchBatch = jest.fn(async () => ({ rawEvents: [] }));

    const result = await rollbackAndReingest(store, 0, fetchBatch);

    expect(result.newCursor).toBe(0);
    expect(fetchBatch).toHaveBeenCalledTimes(1);
  });

  it("throws when re-ingestion fails mid-way", async () => {
    await ingestBatch(store, [makeEvent(1)], 1);
    await store.rollbackTo(0);

    const fetchBatch = jest.fn(async (cursor: number) => {
      if (cursor === 1) {
        return { rawEvents: [makeEvent(1)] };
      }
      // Simulate a failure fetching cursor 2
      throw new Error("RPC timeout");
    });

    await expect(rollbackAndReingest(store, 0, fetchBatch)).rejects.toThrow(
      "Re-ingestion failed at cursor 2"
    );

    // Cursor 1 should still have been committed
    expect(await store.getCursor()).toBe(1);
    expect(store.getEvents()).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// Integration: rollback + existing ingestion.test.ts behaviors
// ---------------------------------------------------------------------------

describe("Rollback preserves existing ingestion guarantees", () => {
  let store: InMemoryIngestionStore;

  beforeEach(() => {
    store = new InMemoryIngestionStore();
  });

  it("idempotency still works after rollback", async () => {
    await ingestBatch(store, [makeEvent(10)], 10);
    await store.rollbackTo(5);
    await ingestBatch(store, [makeEvent(10)], 10);

    // Re-submit cursor 10
    const result = await ingestBatch(store, [makeEvent(10)], 10);
    expect(result.skipped).toBe(true);
    expect(store.getEvents()).toHaveLength(1);
  });

  it("validation still rejects bad events after rollback", async () => {
    await ingestBatch(store, [makeEvent(5)], 5);
    await store.rollbackTo(3);

    const bad = { ledger: 10, txHash: "", type: "X", payload: {} };
    await expect(ingestBatch(store, [bad], 4)).rejects.toThrow("txHash");

    // Store must be untouched by the rejected batch
    expect(await store.getCursor()).toBe(3);
    expect(store.getEvents()).toHaveLength(0);
  });

  it("crash simulation: rollback leaves store consistent", async () => {
    await ingestBatch(store, [makeEvent(10)], 10);
    await ingestBatch(store, [makeEvent(20)], 20);

    // Simulate a crashing rollback (partial failure)
    const crashingStore: IngestionStore = {
      getCursor: () => store.getCursor(),
      commitBatch: (events, cursor) => store.commitBatch(events, cursor),
      rollbackTo: async () => {
        throw new Error("simulated rollback crash");
      },
    };

    await expect(
      rollbackAndReingest(crashingStore, 5, async () => ({ rawEvents: [] }))
    ).rejects.toThrow("simulated rollback crash");

    // The original store should be unchanged
    expect(await store.getCursor()).toBe(20);
    expect(store.getEvents()).toHaveLength(2);
  });
});

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

describe("Rollback edge cases", () => {
  let store: InMemoryIngestionStore;

  beforeEach(() => {
    store = new InMemoryIngestionStore();
  });

  it("rollback to a cursor higher than current advances cursor (no data lost)", async () => {
    await ingestBatch(store, [makeEvent(5)], 5);

    // Rollback to cursor 10 — nothing to delete, cursor advances to 10
    await store.rollbackTo(10);

    expect(await store.getCursor()).toBe(10);
    expect(store.getEvents()).toHaveLength(1); // event at ledger 5 stays
  });

  it("rollback to the same cursor deletes nothing", async () => {
    await ingestBatch(store, [makeEvent(10)], 10);
    const before = store.getEvents().length;

    await store.rollbackTo(10);

    expect(store.getEvents()).toHaveLength(before);
    expect(await store.getCursor()).toBe(10);
  });
});
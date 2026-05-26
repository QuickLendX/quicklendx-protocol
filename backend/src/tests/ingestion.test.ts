import {
  ingestBatch,
  InMemoryIngestionStore,
  IngestionStore,
  IndexedEvent,
} from "../services/ingestion";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeEvent(ledger: number, i = 0): IndexedEvent {
  return {
    ledger,
    txHash: `0xabc${ledger}${i}`,
    type: "InvoiceCreated",
    payload: { id: `inv-${ledger}-${i}` },
  };
}

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

describe("ingestBatch — happy path", () => {
  it("commits a batch and advances the cursor", async () => {
    const store = new InMemoryIngestionStore();
    const events = [makeEvent(100), makeEvent(100, 1)];

    const result = await ingestBatch(store, events, 100);

    expect(result.committed).toBe(true);
    expect(result.cursor).toBe(100);
    expect(result.eventsProcessed).toBe(2);
    expect(result.skipped).toBe(false);
    expect(await store.getCursor()).toBe(100);
    expect(store.getEvents()).toHaveLength(2);
  });

  it("commits an empty batch (no events, cursor advances)", async () => {
    const store = new InMemoryIngestionStore();
    const result = await ingestBatch(store, [], 50);

    expect(result.committed).toBe(true);
    expect(result.eventsProcessed).toBe(0);
    expect(await store.getCursor()).toBe(50);
  });

  it("processes sequential batches correctly", async () => {
    const store = new InMemoryIngestionStore();

    await ingestBatch(store, [makeEvent(10)], 10);
    await ingestBatch(store, [makeEvent(20), makeEvent(20, 1)], 20);

    expect(await store.getCursor()).toBe(20);
    expect(store.getEvents()).toHaveLength(3);
  });
});

// ---------------------------------------------------------------------------
// Idempotency
// ---------------------------------------------------------------------------

describe("ingestBatch — idempotency", () => {
  it("skips a batch whose cursor is already committed", async () => {
    const store = new InMemoryIngestionStore();
    await ingestBatch(store, [makeEvent(100)], 100);

    const result = await ingestBatch(store, [makeEvent(100)], 100);

    expect(result.skipped).toBe(true);
    expect(result.committed).toBe(false);
    expect(result.eventsProcessed).toBe(0);
    // Store must not have grown
    expect(store.getEvents()).toHaveLength(1);
  });

  it("skips a batch with a cursor behind the current cursor", async () => {
    const store = new InMemoryIngestionStore();
    await ingestBatch(store, [makeEvent(200)], 200);

    const result = await ingestBatch(store, [makeEvent(150)], 150);

    expect(result.skipped).toBe(true);
    expect(await store.getCursor()).toBe(200); // cursor must not regress
  });

  it("replaying the same batch twice produces identical store state", async () => {
    const store = new InMemoryIngestionStore();
    const events = [makeEvent(300), makeEvent(300, 1)];

    await ingestBatch(store, events, 300);
    const snapshotAfterFirst = store.getEvents();

    await ingestBatch(store, events, 300); // replay
    const snapshotAfterReplay = store.getEvents();

    expect(snapshotAfterReplay).toEqual(snapshotAfterFirst);
  });
});

// ---------------------------------------------------------------------------
// Validation / partial-failure rollback
// ---------------------------------------------------------------------------

describe("ingestBatch — validation and rollback", () => {
  it("rejects a batch containing an event with no txHash", async () => {
    const store = new InMemoryIngestionStore();
    const bad = { ledger: 10, txHash: "", type: "X", payload: {} };

    await expect(ingestBatch(store, [bad], 10)).rejects.toThrow("txHash");

    // Store must be untouched
    expect(await store.getCursor()).toBeNull();
    expect(store.getEvents()).toHaveLength(0);
  });

  it("rejects a batch containing an event with a negative ledger", async () => {
    const store = new InMemoryIngestionStore();
    const bad = { ledger: -1, txHash: "0xabc", type: "X", payload: {} };

    await expect(ingestBatch(store, [bad], 10)).rejects.toThrow("ledger");

    expect(await store.getCursor()).toBeNull();
  });

  it("rejects a batch containing an event with no type", async () => {
    const store = new InMemoryIngestionStore();
    const bad = { ledger: 10, txHash: "0xabc", type: "", payload: {} };

    await expect(ingestBatch(store, [bad], 10)).rejects.toThrow("type");
  });

  it("rolls back all events when one event in the middle is invalid", async () => {
    const store = new InMemoryIngestionStore();
    const events: IndexedEvent[] = [
      makeEvent(10, 0),
      { ledger: 10, txHash: "", type: "Bad", payload: {} }, // invalid
      makeEvent(10, 2),
    ];

    await expect(ingestBatch(store, events, 10)).rejects.toThrow();

    // No partial write — store is still empty
    expect(store.getEvents()).toHaveLength(0);
    expect(await store.getCursor()).toBeNull();
  });

  it("does not advance cursor when validation fails", async () => {
    const store = new InMemoryIngestionStore();
    await ingestBatch(store, [makeEvent(5)], 5); // good batch first

    const bad = { ledger: 10, txHash: "", type: "X", payload: {} };
    await expect(ingestBatch(store, [bad], 10)).rejects.toThrow();

    // Cursor must stay at 5, not advance to 10
    expect(await store.getCursor()).toBe(5);
  });
});

// ---------------------------------------------------------------------------
// Crash simulation
// ---------------------------------------------------------------------------

describe("ingestBatch — crash simulation", () => {
  it("leaves store unchanged when commitBatch throws (simulated crash)", async () => {
    const store = new InMemoryIngestionStore();
    await ingestBatch(store, [makeEvent(10)], 10); // baseline

    // Wrap the store so commitBatch throws on the next call
    const crashingStore: IngestionStore = {
      getCursor: () => store.getCursor(),
      commitBatch: async () => {
        throw new Error("simulated disk failure");
      },
    };

    await expect(ingestBatch(crashingStore, [makeEvent(20)], 20)).rejects.toThrow(
      "simulated disk failure"
    );

    // The in-memory store (which represents durable state) is unchanged
    expect(await store.getCursor()).toBe(10);
    expect(store.getEvents()).toHaveLength(1);
  });

  it("is safe to retry after a crash — second attempt succeeds", async () => {
    const store = new InMemoryIngestionStore();
    let callCount = 0;

    const flakyStore: IngestionStore = {
      getCursor: () => store.getCursor(),
      commitBatch: async (events, cursor) => {
        callCount++;
        if (callCount === 1) throw new Error("transient failure");
        return store.commitBatch(events, cursor);
      },
    };

    const events = [makeEvent(30)];

    // First attempt fails
    await expect(ingestBatch(flakyStore, events, 30)).rejects.toThrow("transient failure");
    expect(await store.getCursor()).toBeNull();

    // Second attempt (retry) succeeds
    const result = await ingestBatch(flakyStore, events, 30);
    expect(result.committed).toBe(true);
    expect(await store.getCursor()).toBe(30);
  });

  it("cursor does not drift after multiple crash-retry cycles", async () => {
    const store = new InMemoryIngestionStore();
    let failNext = false;

    const flakyStore: IngestionStore = {
      getCursor: () => store.getCursor(),
      commitBatch: async (events, cursor) => {
        if (failNext) {
          failNext = false;
          throw new Error("crash");
        }
        return store.commitBatch(events, cursor);
      },
    };

    // Batch 1: succeeds
    await ingestBatch(flakyStore, [makeEvent(10)], 10);

    // Batch 2: crashes, then retries successfully
    failNext = true;
    await expect(ingestBatch(flakyStore, [makeEvent(20)], 20)).rejects.toThrow();
    await ingestBatch(flakyStore, [makeEvent(20)], 20); // retry

    // Batch 3: succeeds
    await ingestBatch(flakyStore, [makeEvent(30)], 30);

    expect(await store.getCursor()).toBe(30);
    expect(store.getEvents()).toHaveLength(3);
  });
});

// ---------------------------------------------------------------------------
// InMemoryIngestionStore unit tests
// ---------------------------------------------------------------------------

describe("InMemoryIngestionStore", () => {
  it("starts with null cursor", async () => {
    const store = new InMemoryIngestionStore();
    expect(await store.getCursor()).toBeNull();
  });

  it("reset clears cursor and events", async () => {
    const store = new InMemoryIngestionStore();
    await store.commitBatch([makeEvent(1)], 1);
    store.reset();
    expect(await store.getCursor()).toBeNull();
    expect(store.getEvents()).toHaveLength(0);
  });

  it("getEvents returns a copy, not the internal array", async () => {
    const store = new InMemoryIngestionStore();
    await store.commitBatch([makeEvent(1)], 1);
    const copy = store.getEvents();
    copy.push(makeEvent(99));
    expect(store.getEvents()).toHaveLength(1); // internal state unchanged
  });
});

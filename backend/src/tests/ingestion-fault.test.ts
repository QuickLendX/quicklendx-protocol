import { ingestBatch, IndexedEvent, InMemoryIngestionStore } from "../services/ingestion";
import { FaultyIngestionStore } from "./helpers/faultInjector";

function makeEvent(ledger: number, i = 0): IndexedEvent {
  return {
    ledger,
    txHash: `0xabc${ledger}${i}`,
    type: "InvoiceCreated",
    payload: { id: `inv-${ledger}-${i}` },
  };
}

/**
 * DeduplicatingInMemoryStore extends InMemoryIngestionStore to simulate
 * a database with UNIQUE constraints on events (e.g. transaction hash / type),
 * which would ignore/on-conflict-do-nothing duplicate writes.
 */
class DeduplicatingInMemoryStore extends InMemoryIngestionStore {
  async commitBatch(events: IndexedEvent[], newCursor: number): Promise<void> {
    const existingEvents = this.getEvents();
    const existingHashes = new Set(existingEvents.map((e) => e.txHash));
    const uniqueNewEvents = events.filter((e) => !existingHashes.has(e.txHash));

    // To simulate partial commit correctly: if target has cursor updated, we use it.
    // InMemoryIngestionStore commitBatch appends events and updates cursor.
    await super.commitBatch(uniqueNewEvents, newCursor);
  }
}

describe("Ingestion Service - Fault Injection Tests", () => {
  let underlyingStore: DeduplicatingInMemoryStore;
  let faultyStore: FaultyIngestionStore;

  beforeEach(() => {
    underlyingStore = new DeduplicatingInMemoryStore();
    faultyStore = new FaultyIngestionStore(underlyingStore);
  });

  it("handles a complete DB commit failure mid-batch without advancing cursor", async () => {
    const events = [makeEvent(100, 1), makeEvent(100, 2)];
    faultyStore.setShouldFailCommit(true, new Error("DB Connection Lost"));

    await expect(ingestBatch(faultyStore, events, 100)).rejects.toThrow("DB Connection Lost");

    // Cursor must not have advanced and no events must be persisted
    expect(await faultyStore.getCursor()).toBeNull();
    expect(underlyingStore.getEvents()).toHaveLength(0);
  });

  it("handles partial commit and successful idempotency replay", async () => {
    const events = [makeEvent(200, 1), makeEvent(200, 2)];
    
    // Simulate a partial commit where 1 event gets written but the cursor is not updated and transaction fails
    faultyStore.setShouldFailCommit(true, new Error("Partial commit failed"));
    faultyStore.setPartialCommitCount(1);

    await expect(ingestBatch(faultyStore, events, 200)).rejects.toThrow("Partial commit failed");

    // Cursor is not updated
    expect(await faultyStore.getCursor()).toBeNull();
    // But one event was written due to the partial write
    expect(underlyingStore.getEvents()).toHaveLength(1);
    expect(underlyingStore.getEvents()[0].txHash).toBe(events[0].txHash);

    // Disable fault injection and replay the exact same batch
    faultyStore.setShouldFailCommit(false);
    faultyStore.setPartialCommitCount(0);

    const result = await ingestBatch(faultyStore, events, 200);

    // Replay must succeed and advance the cursor
    expect(result.committed).toBe(true);
    expect(result.cursor).toBe(200);
    expect(await faultyStore.getCursor()).toBe(200);

    // The store should contain exactly the 2 events, no duplicates
    const storedEvents = underlyingStore.getEvents();
    expect(storedEvents).toHaveLength(2);
    expect(storedEvents.map((e) => e.txHash)).toEqual(events.map((e) => e.txHash));
  });

  it("handles getCursor failures correctly", async () => {
    faultyStore.setShouldFailGetCursor(true, new Error("Unable to fetch cursor"));
    await expect(ingestBatch(faultyStore, [makeEvent(100)], 100)).rejects.toThrow("Unable to fetch cursor");
  });

  it("handles rollback failures and success in FaultyIngestionStore", async () => {
    faultyStore.setShouldFailRollback(true, new Error("Rollback failed"));
    await expect(faultyStore.rollbackTo(50)).rejects.toThrow("Rollback failed");

    faultyStore.setShouldFailRollback(false);
    await faultyStore.rollbackTo(50); // should succeed
  });
});

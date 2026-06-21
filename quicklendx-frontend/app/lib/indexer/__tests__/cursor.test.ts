/**
 * Tests for Chain Cursor Model & Monotonic Ingestion (#876)
 *
 * Run with: npx jest app/lib/indexer/__tests__/cursor.test.ts
 */

import {
  ChainCursor,
  compareCursors,
  InMemoryCursorStore,
  MonotonicIngester,
  parseCursor,
  serializeCursor,
} from "../cursor";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function cur(
  ledger_seq: number,
  tx_hash: string,
  event_index: number
): ChainCursor {
  return { ledger_seq, tx_hash, event_index };
}

// ---------------------------------------------------------------------------
// compareCursors
// ---------------------------------------------------------------------------

describe("compareCursors", () => {
  const base = cur(100, "aaa", 3);

  it("returns 'after' for the next sequential event in same ledger", () => {
    expect(compareCursors(base, cur(100, "aaa", 4))).toBe("after");
  });

  it("returns 'after' for first event in next ledger (event_index=0)", () => {
    expect(compareCursors(base, cur(101, "bbb", 0))).toBe("after");
  });

  it("returns 'equal' for an identical cursor (duplicate)", () => {
    expect(compareCursors(base, cur(100, "aaa", 3))).toBe("equal");
  });

  it("returns 'before' when next ledger_seq < current", () => {
    expect(compareCursors(base, cur(99, "zzz", 0))).toBe("before");
  });

  it("returns 'before' when same ledger but event_index < current", () => {
    expect(compareCursors(base, cur(100, "aaa", 2))).toBe("before");
  });

  it("returns 'gap' when event_index skips within same ledger", () => {
    // event_index jumps from 3 to 5, skipping 4
    expect(compareCursors(base, cur(100, "aaa", 5))).toBe("gap");
  });

  it("returns 'gap' when next ledger event_index != 0", () => {
    expect(compareCursors(base, cur(101, "bbb", 1))).toBe("gap");
  });

  it("returns 'gap' when ledger jumps by more than 1", () => {
    expect(compareCursors(base, cur(103, "ccc", 0))).toBe("gap");
  });
});

// ---------------------------------------------------------------------------
// serializeCursor / parseCursor
// ---------------------------------------------------------------------------

describe("serializeCursor / parseCursor", () => {
  const c = cur(12345, "deadbeef", 7);

  it("round-trips correctly", () => {
    expect(parseCursor(serializeCursor(c))).toEqual(c);
  });

  it("throws on malformed input", () => {
    expect(() => parseCursor("bad-format")).toThrow(/invalid cursor/i);
  });

  it("throws on NaN parts", () => {
    expect(() => parseCursor("abc:deadbeef:xyz")).toThrow(/invalid cursor/i);
  });
});

// ---------------------------------------------------------------------------
// InMemoryCursorStore
// ---------------------------------------------------------------------------

describe("InMemoryCursorStore", () => {
  it("returns null before any cursor is set", async () => {
    const store = new InMemoryCursorStore();
    expect(await store.getLastCursor()).toBeNull();
  });

  it("persists and retrieves the last cursor", async () => {
    const store = new InMemoryCursorStore();
    const c = cur(50, "ff00", 0);
    await store.setLastCursor(c);
    expect(await store.getLastCursor()).toEqual(c);
  });
});

// ---------------------------------------------------------------------------
// MonotonicIngester
// ---------------------------------------------------------------------------

describe("MonotonicIngester", () => {
  function makeIngester(
    store?: InMemoryCursorStore,
    onGap?: jest.Mock,
    onDuplicate?: jest.Mock
  ) {
    const s = store ?? new InMemoryCursorStore();
    const accepted: ChainCursor[] = [];
    const ingester = new MonotonicIngester(s, {
      onAccept: (c) => {
        accepted.push(c);
      },
      onGap,
      onDuplicate,
    });
    return { ingester, accepted, store: s };
  }

  it("accepts the first event unconditionally", async () => {
    const { ingester, accepted } = makeIngester();
    await ingester.resume();
    const result = await ingester.ingest(cur(1, "aa", 0), {});
    expect(result.status).toBe("accepted");
    expect(accepted).toHaveLength(1);
  });

  it("advances correctly through sequential events", async () => {
    const { ingester, accepted } = makeIngester();
    await ingester.resume();
    await ingester.ingest(cur(1, "aa", 0), {});
    await ingester.ingest(cur(1, "aa", 1), {});
    const result = await ingester.ingest(cur(2, "bb", 0), {});
    expect(result.status).toBe("accepted");
    expect(accepted).toHaveLength(3);
  });

  it("skips duplicate cursors without advancing", async () => {
    const onDuplicate = jest.fn();
    const { ingester } = makeIngester(undefined, undefined, onDuplicate);
    await ingester.resume();
    await ingester.ingest(cur(1, "aa", 0), {});
    const result = await ingester.ingest(cur(1, "aa", 0), {});
    expect(result.status).toBe("duplicate");
    expect(onDuplicate).toHaveBeenCalledTimes(1);
  });

  it("halts and calls onGap when a gap is detected", async () => {
    const onGap = jest.fn();
    const { ingester } = makeIngester(undefined, onGap);
    await ingester.resume();
    await ingester.ingest(cur(1, "aa", 0), {});
    // Skip event_index 1 and jump to 3
    const result = await ingester.ingest(cur(1, "aa", 3), {});
    expect(result.status).toBe("gap");
    expect(ingester.isHalted).toBe(true);
    expect(onGap).toHaveBeenCalledTimes(1);
  });

  it("throws if ingest is called while halted", async () => {
    const { ingester } = makeIngester();
    await ingester.resume();
    await ingester.ingest(cur(1, "aa", 0), {});
    await ingester.ingest(cur(1, "aa", 5), {}); // gap → halt
    await expect(ingester.ingest(cur(1, "aa", 6), {})).rejects.toThrow(
      /halted/i
    );
  });

  it("resumes safely from a persisted cursor", async () => {
    const store = new InMemoryCursorStore();
    await store.setLastCursor(cur(10, "cc", 5));

    const { ingester, accepted } = makeIngester(store);
    const resumed = await ingester.resume();
    expect(resumed).toEqual(cur(10, "cc", 5));

    // Next valid event is (10, *, 6)
    await ingester.ingest(cur(10, "cc", 6), {});
    expect(accepted).toHaveLength(1);
  });

  it("commits cursor to store on each accepted event", async () => {
    const store = new InMemoryCursorStore();
    const { ingester } = makeIngester(store);
    await ingester.resume();
    const c = cur(5, "dd", 0);
    await ingester.ingest(c, {});
    expect(await store.getLastCursor()).toEqual(c);
  });
});

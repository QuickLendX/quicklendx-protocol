import { describe, expect, it, beforeEach, afterAll } from "@jest/globals";
import path from "path";
import { promises as fs, constants as fsConstants } from "fs";
import * as zlib from "zlib";
import { createHash } from "crypto";
import { restoreArchivedEvents } from "../scripts/restore-archived-events";
import { RawEvent } from "../src/types/replay";

const DAY_MS = 24 * 60 * 60 * 1000;
const BASE_MS = Date.parse("2026-01-15T00:00:00.000Z");
const TEST_DIR = path.join(__dirname, "fixtures", "restore-archived-events-tests");

const START = new Date(BASE_MS - 2 * DAY_MS).toISOString();
const END = new Date(BASE_MS + 2 * DAY_MS).toISOString();

function rawEvent(id: string, dayOffset: number, ledger = 1): RawEvent {
  const indexedAtMs = BASE_MS + dayOffset * DAY_MS;
  return {
    id,
    ledger,
    txHash: `tx-${id}`,
    eventIndex: 0,
    type: "InvoiceCreated",
    payload: { invoiceId: id },
    timestamp: indexedAtMs,
    complianceHold: false,
    indexedAt: new Date(indexedAtMs).toISOString(),
  };
}

function sha256(buffer: Buffer | string): string {
  return createHash("sha256").update(buffer).digest("hex");
}

type ArchiveOptions = {
  /** Write the `.sha256` sidecar (default true). */
  withChecksum?: boolean;
  /** Override checksum content (default: real digest). */
  checksum?: string;
  /** Write raw bytes instead of gzipping `lines`. */
  rawBytes?: Buffer | string;
};

async function writeArchive(
  dir: string,
  fileName: string,
  lines: unknown[],
  options: ArchiveOptions = {}
): Promise<string> {
  const filePath = path.join(dir, fileName);
  const payload =
    options.rawBytes !== undefined
      ? Buffer.from(options.rawBytes)
      : zlib.gzipSync(
          lines
            .map((l) => (typeof l === "string" ? l : JSON.stringify(l)))
            .join("\n")
            .concat(lines.length > 0 ? "\n" : ""),
          { level: 9 }
        );

  await fs.writeFile(filePath, payload);

  if (options.withChecksum !== false) {
    const checksum =
      options.checksum !== undefined ? options.checksum : sha256(payload);
    await fs.writeFile(`${filePath}.sha256`, `${checksum}\n`, "utf8");
  }

  return filePath;
}

interface StoreCall {
  events: RawEvent[];
}

class FakeStore {
  readonly storeCalls: StoreCall[] = [];
  getAllEventsCalls = 0;

  constructor(
    public events: RawEvent[] = [],
    private readonly options: {
      failOnStoreCall?: number;
      failOnGetAll?: boolean;
    } = {}
  ) {}

  async getAllEvents(): Promise<RawEvent[]> {
    this.getAllEventsCalls += 1;
    if (this.options.failOnGetAll) {
      throw new Error("raw event store unavailable");
    }
    return [...this.events];
  }

  async storeEvents(events: RawEvent[]): Promise<void> {
    this.storeCalls.push({ events: [...events] });
    if (
      this.options.failOnStoreCall !== undefined &&
      this.storeCalls.length === this.options.failOnStoreCall
    ) {
      throw new Error("disk full while persisting raw events");
    }
    this.events.push(...events);
  }

  ids(): string[] {
    return this.events.map((e) => e.id);
  }
}

function restore(
  store: FakeStore,
  overrides: Partial<{ start: string; end: string; archiveDir: string }> = {}
): Promise<number> {
  return restoreArchivedEvents({
    start: overrides.start ?? START,
    end: overrides.end ?? END,
    archiveDir: overrides.archiveDir ?? TEST_DIR,
    rawEventStore: store,
  });
}

describe("restoreArchivedEvents failure-boundary coverage", () => {
  beforeEach(async () => {
    await fs.rm(TEST_DIR, { recursive: true, force: true });
    await fs.mkdir(TEST_DIR, { recursive: true });
  });

  afterAll(async () => {
    await fs.rm(TEST_DIR, { recursive: true, force: true });
  });

  describe("input validation", () => {
    it.each([
      ["start", { start: "not-a-date" }],
      ["end", { end: "31-31-2026" }],
      ["both", { start: "today", end: "tomorrow" }],
    ])("rejects an invalid %s date before touching the archive", async (
      _label,
      overrides
    ) => {
      const store = new FakeStore();

      await expect(restore(store, overrides)).rejects.toThrow(
        "Invalid start or end date format."
      );
      expect(store.getAllEventsCalls).toBe(0);
      expect(store.storeCalls).toHaveLength(0);
    });
  });

  describe("archive discovery", () => {
    it("returns 0 without reading the store when the archive directory is missing", async () => {
      const store = new FakeStore();

      const count = await restore(store, {
        archiveDir: path.join(TEST_DIR, "does-not-exist"),
      });

      expect(count).toBe(0);
      expect(store.getAllEventsCalls).toBe(0);
      expect(store.storeCalls).toHaveLength(0);
    });

    it("propagates non-ENOENT directory read failures", async () => {
      const notADirectory = path.join(TEST_DIR, "archive-is-a-file.jsonl.gz");
      await fs.writeFile(notADirectory, "not a directory", "utf8");
      const store = new FakeStore();

      await expect(restore(store, { archiveDir: notADirectory })).rejects.toThrow(
        /ENOTDIR/
      );
      expect(store.getAllEventsCalls).toBe(0);
    });

    it("ignores archive files that do not match the raw-events naming contract", async () => {
      await writeArchive(TEST_DIR, "events-2026-01.jsonl.gz", [rawEvent("a", 0)]);
      await writeArchive(TEST_DIR, "raw-events-2026-1.jsonl.gz", [rawEvent("b", 0)]);
      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl", [rawEvent("c", 0)]);
      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz.sha256", [rawEvent("d", 0)]);
      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz.bak", [rawEvent("e", 0)]);
      const store = new FakeStore();

      expect(await restore(store)).toBe(0);
      expect(store.storeCalls).toHaveLength(0);
    });

    it("propagates store read failures instead of restoring blindly", async () => {
      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz", [rawEvent("a", 0)]);
      const store = new FakeStore([], { failOnGetAll: true });

      await expect(restore(store)).rejects.toThrow(
        "raw event store unavailable"
      );
      expect(store.storeCalls).toHaveLength(0);
    });
  });

  describe("successful restoration", () => {
    it("restores in-range events, skips out-of-range events, and batches one store call per file", async () => {
      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz", [
        rawEvent("in-range-early", -1, 1),
        rawEvent("in-range-late", 1, 2),
        rawEvent("before-range", -10, 3),
        rawEvent("after-range", 10, 4),
      ]);
      await writeArchive(TEST_DIR, "raw-events-2026-02.jsonl.gz", [
        rawEvent("other-month", 40, 5),
      ]);
      const store = new FakeStore();

      expect(await restore(store)).toBe(2);
      expect(store.storeCalls).toHaveLength(1);
      expect(store.storeCalls[0].events.map((e) => e.id)).toEqual([
        "in-range-early",
        "in-range-late",
      ]);
      expect(store.ids().sort()).toEqual(["in-range-early", "in-range-late"]);
    });

    it("treats the start and end boundaries as inclusive", async () => {
      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz", [
        rawEvent("on-start", -2, 1),
        rawEvent("on-end", 2, 2),
        rawEvent("just-before-start", -3, 3),
        rawEvent("just-after-end", 3, 4),
      ]);
      const store = new FakeStore();

      expect(await restore(store)).toBe(2);
      expect(store.ids().sort()).toEqual(["on-end", "on-start"]);
    });

    it("ignores blank lines and empty archive files", async () => {
      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz", [
        JSON.stringify(rawEvent("padded", 0, 1)),
        "",
        "   ",
        JSON.stringify(rawEvent("padded-2", 1, 2)),
        "",
      ]);
      await writeArchive(TEST_DIR, "raw-events-2026-02.jsonl.gz", []);
      const store = new FakeStore();

      expect(await restore(store)).toBe(2);
      expect(store.storeCalls).toHaveLength(1);
    });

    it("excludes events with unparseable indexedAt values instead of restoring them", async () => {
      const broken = {
        ...rawEvent("broken-timestamp", 0),
        indexedAt: "not-a-timestamp",
      } as unknown as RawEvent;
      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz", [
        rawEvent("healthy", 0, 1),
        broken,
      ]);
      const store = new FakeStore();

      expect(await restore(store)).toBe(1);
      expect(store.ids()).toEqual(["healthy"]);
    });

    it("returns 0 and never writes when the requested range is inverted", async () => {
      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz", [
        rawEvent("a", 0, 1),
      ]);
      const store = new FakeStore();

      expect(await restore(store, { start: END, end: START })).toBe(0);
      expect(store.storeCalls).toHaveLength(0);
    });
  });

  describe("idempotency and retry guards", () => {
    it("restores nothing on a repeated run against the same store", async () => {
      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz", [
        rawEvent("idempotent-1", 0, 1),
        rawEvent("idempotent-2", 1, 2),
      ]);
      const store = new FakeStore([rawEvent("already-live", 0, 99)]);

      expect(await restore(store)).toBe(2);
      expect(await restore(store)).toBe(0);
      expect(await restore(store)).toBe(0);
      expect(store.storeCalls).toHaveLength(1);
      expect(store.ids().sort()).toEqual([
        "already-live",
        "idempotent-1",
        "idempotent-2",
      ]);
    });

    it("restores an event present in two archive files only once", async () => {
      const shared = rawEvent("shared", 0, 1);
      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz", [
        rawEvent("only-january", 0, 2),
        shared,
      ]);
      await writeArchive(TEST_DIR, "raw-events-2026-02.jsonl.gz", [
        shared,
        rawEvent("only-february", 0, 3),
      ]);
      const store = new FakeStore();

      expect(await restore(store)).toBe(3);
      expect(store.ids().sort()).toEqual([
        "only-february",
        "only-january",
        "shared",
      ]);
      expect(store.ids()).toHaveLength(
        new Set(store.ids()).size // no duplicate ids persisted
      );
    });

    it("does not re-persist duplicates within a single archive file", async () => {
      const dup = rawEvent("duplicate-in-file", 0, 1);
      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz", [dup, dup]);
      const store = new FakeStore();

      expect(await restore(store)).toBe(1);
      expect(store.ids()).toEqual(["duplicate-in-file"]);
    });
  });

  describe("malformed and tampered archive files", () => {
    it("fails closed when the checksum sidecar is missing", async () => {
      const filePath = await writeArchive(
        TEST_DIR,
        "raw-events-2026-01.jsonl.gz",
        [rawEvent("a", 0)],
        { withChecksum: false }
      );
      const store = new FakeStore();

      await expect(restore(store)).rejects.toThrow(
        `Checksum file missing for ${filePath}`
      );
      expect(store.storeCalls).toHaveLength(0);
    });

    it("fails closed on a truncated or wrong checksum value", async () => {
      const filePath = await writeArchive(
        TEST_DIR,
        "raw-events-2026-01.jsonl.gz",
        [rawEvent("a", 0)],
        { checksum: `${"0".repeat(64)}` }
      );
      const store = new FakeStore();

      await expect(restore(store)).rejects.toThrow(
        `Checksum verification failed for ${filePath}`
      );
      expect(store.storeCalls).toHaveLength(0);
    });

    it("accepts a checksum sidecar with surrounding whitespace", async () => {
      const filePath = await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz", [
        rawEvent("a", 0),
      ]);
      await fs.writeFile(
        `${filePath}.sha256`,
        `\n  ${sha256(await fs.readFile(filePath))}  \n\n`,
        "utf8"
      );
      const store = new FakeStore();

      expect(await restore(store)).toBe(1);
    });

    it("reports decompression failures for content that is not valid gzip", async () => {
      await writeArchive(
        TEST_DIR,
        "raw-events-2026-01.jsonl.gz",
        [],
        { rawBytes: "definitely-not-gzip" }
      );
      const store = new FakeStore();

      await expect(restore(store)).rejects.toThrow(/Failed to decompress/);
      expect(store.storeCalls).toHaveLength(0);
    });

    it("rejects truncated gzip streams", async () => {
      const full = zlib.gzipSync(`${JSON.stringify(rawEvent("a", 0))}\n`, {
        level: 9,
      });
      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz", [], {
        rawBytes: full.subarray(0, Math.floor(full.length / 2)),
      });
      const store = new FakeStore();

      await expect(restore(store)).rejects.toThrow(/Failed to decompress/);
      expect(store.storeCalls).toHaveLength(0);
    });

    it("rejects a malformed JSON line and writes nothing from that file", async () => {
      const filePath = path.join(TEST_DIR, "raw-events-2026-01.jsonl.gz");
      const good = JSON.stringify(rawEvent("good", 0, 1));
      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz", [
        good,
        "{not-json",
        JSON.stringify(rawEvent("later", 1, 2)),
      ]);
      const store = new FakeStore();

      await expect(restore(store)).rejects.toThrow(
        `Failed to parse JSON line from ${filePath}`
      );
      // The whole file is rejected atomically: no partial file is persisted.
      expect(store.storeCalls).toHaveLength(0);
      expect(store.ids()).toHaveLength(0);
    });

    it("rejects a JSON line that is not a valid event object", async () => {
      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz", [
        "null",
        JSON.stringify(rawEvent("a", 0)),
      ]);
      const store = new FakeStore();

      await expect(restore(store)).rejects.toThrow(
        /Failed to parse JSON line/
      );
      expect(store.ids()).toHaveLength(0);
    });
  });

  describe("partial failure recovery", () => {
    it("keeps earlier successful files and completes on retry after a later file is repaired", async () => {
      const january = await writeArchive(
        TEST_DIR,
        "raw-events-2026-01.jsonl.gz",
        [rawEvent("january-1", 0, 1), rawEvent("january-2", 1, 2)]
      );
      const february = await writeArchive(
        TEST_DIR,
        "raw-events-2026-02.jsonl.gz",
        [rawEvent("february-1", 0, 3)]
      );
      const store = new FakeStore();

      // Corrupt only the February archive; January must already be restored.
      await fs.writeFile(february, "corrupted", "utf8");
      await fs.writeFile(
        `${february}.sha256`,
        sha256(await fs.readFile(february)),
        "utf8"
      );

      await expect(restore(store)).rejects.toThrow(/Failed to decompress/);
      expect(store.ids().sort()).toEqual(["january-1", "january-2"]);

      // Repair the archive and retry: the run completes without duplicating January.
      await writeArchive(TEST_DIR, "raw-events-2026-02.jsonl.gz", [
        rawEvent("february-1", 0, 3),
      ]);
      expect(await restore(store)).toBe(1);
      expect(store.ids().sort()).toEqual([
        "february-1",
        "january-1",
        "january-2",
      ]);

      // A further retry is a no-op.
      expect(await restore(store)).toBe(0);
      expect(january).toContain("raw-events-2026-01.jsonl.gz");
    });

    it("propagates store write failures and recovers cleanly on retry", async () => {
      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz", [
        rawEvent("january-1", 0, 1),
      ]);
      await writeArchive(TEST_DIR, "raw-events-2026-02.jsonl.gz", [
        rawEvent("february-1", 0, 2),
      ]);

      const failing = new FakeStore([], { failOnStoreCall: 2 });
      await expect(restore(failing)).rejects.toThrow(
        "disk full while persisting raw events"
      );
      // First file persisted, second file failed: no event is lost or duplicated.
      expect(failing.ids()).toEqual(["january-1"]);

      const recovered = new FakeStore(failing.events);
      expect(await restore(recovered)).toBe(1);
      expect(recovered.ids().sort()).toEqual(["february-1", "january-1"]);
      expect(await restore(recovered)).toBe(0);
    });

    it("recovers when a corrupt archive is quarantined and the run is retried", async () => {
      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz", [
        rawEvent("january-1", 0, 1),
      ]);
      const corrupt = await writeArchive(
        TEST_DIR,
        "raw-events-2026-02.jsonl.gz",
        [rawEvent("february-1", 0, 2)],
        { checksum: `${"0".repeat(64)}` }
      );
      const store = new FakeStore();

      await expect(restore(store)).rejects.toThrow(
        `Checksum verification failed for ${corrupt}`
      );
      expect(store.ids()).toEqual(["january-1"]);

      await fs.rm(corrupt, { force: true });
      await fs.rm(`${corrupt}.sha256`, { force: true });
      expect(await restore(store)).toBe(0);
      expect(store.ids()).toEqual(["january-1"]);
    });
  });

  describe("deterministic output", () => {
    it("produces byte-identical archives and identical restore results across repeated runs", async () => {
      const events = [
        rawEvent("det-1", 0, 1),
        rawEvent("det-2", 1, 2),
        rawEvent("det-3", 2, 3),
      ];

      const first = await writeArchive(
        TEST_DIR,
        "raw-events-2026-01.jsonl.gz",
        events
      );
      const firstBytes = await fs.readFile(first);

      await writeArchive(TEST_DIR, "raw-events-2026-01.jsonl.gz", events);
      const secondBytes = await fs.readFile(
        path.join(TEST_DIR, "raw-events-2026-01.jsonl.gz")
      );
      expect(sha256(firstBytes)).toBe(sha256(secondBytes));

      const storeA = new FakeStore();
      const storeB = new FakeStore([rawEvent("pre-existing", 0, 9)]);
      expect(await restore(storeA)).toBe(3);
      expect(await restore(storeB)).toBe(3);
      expect(storeB.ids()).toHaveLength(4);
      expect(storeB.ids().filter((id) => id === "det-1")).toHaveLength(1);
    });

    it("is a no-op on an archive directory that only holds unrelated files", async () => {
      await fs.writeFile(
        path.join(TEST_DIR, "README.txt"),
        "archives are stored as raw-events-YYYY-MM.jsonl.gz",
        "utf8"
      );
      await fs.access(
        path.join(TEST_DIR, "README.txt"),
        fsConstants.F_OK
      );
      const store = new FakeStore();

      expect(await restore(store)).toBe(0);
      expect(store.storeCalls).toHaveLength(0);
    });
  });
});

import fs from "fs";
import path from "path";
import crypto from "crypto";
import { getDatabase, closeDatabase } from "../lib/database";
import { freshnessService, buildCursor } from "../services/freshnessService";

const TEST_DB_DIR = path.resolve(__dirname, "../../.data");
const TEST_DB_PATH = path.join(TEST_DB_DIR, `test-freshness-persistence-${crypto.randomUUID()}.db`);

beforeAll(() => {
  process.env.DATABASE_PATH = TEST_DB_PATH;
  closeDatabase();
  if (!fs.existsSync(TEST_DB_DIR)) {
    fs.mkdirSync(TEST_DB_DIR, { recursive: true });
  }
  const db = getDatabase();
  db.exec(`
    CREATE TABLE IF NOT EXISTS freshness_state (
      id INTEGER PRIMARY KEY CHECK(id = 1),
      cursor TEXT NOT NULL,
      timestamp TEXT NOT NULL
    )
  `);
});

afterAll(() => {
  closeDatabase();
  try {
    if (fs.existsSync(TEST_DB_PATH)) {
      fs.unlinkSync(TEST_DB_PATH);
    }
    fs.unlinkSync(TEST_DB_PATH + "-wal");
    fs.unlinkSync(TEST_DB_PATH + "-shm");
  } catch {
    /* ignore cleanup failures */
  }
});

beforeEach(async () => {
  freshnessService.resetForTests();
  const db = getDatabase();
  db.exec("DELETE FROM freshness_state");
});

function wait(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

describe("freshness persistence", () => {
  test("initial boot from empty table falls back gracefully", async () => {
    freshnessService.setMockNowMs(1710000000000);
    await freshnessService.initialize();

    const freshness = freshnessService.getFreshness();

    expect(freshness.cursor).toMatch(/^\d+_0$/);
    expect(freshness.lastIndexedLedger).toBeGreaterThanOrEqual(100000);
    expect(freshness.indexLagSeconds).toBeGreaterThanOrEqual(0);
    expect(new Date(freshness.lastUpdatedAt).getTime()).toBeGreaterThan(0);
  });

  test("restart preserves last cursor and timestamp", async () => {
    await freshnessService.initialize();
    freshnessService.updateFreshness("100010_0", "2026-06-23T12:00:00Z");
    await freshnessService.flush();

    closeDatabase();
    freshnessService.resetForTests();
    process.env.DATABASE_PATH = TEST_DB_PATH;

    await freshnessService.initialize();
    const persisted = freshnessService.getFreshness();

    expect(persisted.cursor).toBe("100010_0");
    expect(persisted.lastIndexedLedger).toBe(100010);
    expect(persisted.lastUpdatedAt).toBe("2026-06-23T12:00:00Z");
  });

  test("debounced writes do not thrash the database", async () => {
    await freshnessService.initialize();
    const db = getDatabase();
    const prepareSpy = jest.spyOn(db, "prepare");

    freshnessService.updateFreshness("100020_0", "2026-06-23T12:05:00Z");
    freshnessService.updateFreshness("100021_0", "2026-06-23T12:05:10Z");
    freshnessService.updateFreshness("100022_0", "2026-06-23T12:05:20Z");

    await wait(200);

    const rows = db.prepare("SELECT cursor, timestamp FROM freshness_state WHERE id = 1").get();
    expect(rows.cursor).toBe("100022_0");
    expect(rows.timestamp).toBe("2026-06-23T12:05:20Z");
    expect(prepareSpy.mock.calls.filter(([sql]) => typeof sql === "string" && sql.includes("INSERT INTO freshness_state")).length).toBe(1);

    prepareSpy.mockRestore();
  });

  test("concurrent updates use the latest cursor", async () => {
    await freshnessService.initialize();

    freshnessService.updateFreshness("100030_0", "2026-06-23T12:10:00Z");
    freshnessService.updateFreshness("100031_0", "2026-06-23T12:10:05Z");
    freshnessService.updateFreshness("100032_0", "2026-06-23T12:10:10Z");

    await freshnessService.flush();

    const db = getDatabase();
    const persisted = db.prepare("SELECT cursor, timestamp FROM freshness_state WHERE id = 1").get();

    expect(persisted.cursor).toBe("100032_0");
    expect(persisted.timestamp).toBe("2026-06-23T12:10:10Z");
  });

  test("persisted value is reflected in responses within 100ms of boot", async () => {
    const db = getDatabase();
    db.prepare(
      "INSERT INTO freshness_state (id, cursor, timestamp) VALUES (1, ?, ?) ON CONFLICT(id) DO UPDATE SET cursor = excluded.cursor, timestamp = excluded.timestamp"
    ).run("100040_0", "2026-06-23T12:15:00Z");

    const start = Date.now();
    await freshnessService.initialize();
    const result = freshnessService.getFreshness();
    const elapsed = Date.now() - start;

    expect(result.cursor).toBe("100040_0");
    expect(result.lastUpdatedAt).toBe("2026-06-23T12:15:00Z");
    expect(elapsed).toBeLessThanOrEqual(100);
  });
});

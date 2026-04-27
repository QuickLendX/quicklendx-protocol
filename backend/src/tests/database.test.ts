import { getDatabase, closeDatabase } from "../lib/database";

describe("Database", () => {
  beforeEach(() => {
    closeDatabase();
  });

  test("sets WAL journal mode", () => {
    const db = getDatabase();
    const row = db.pragma("journal_mode");
    expect(row).toBe("wal");
  });

  test("enables foreign key constraints", () => {
    const db = getDatabase();
    const row = db.pragma("foreign_keys");
    expect(row).toBe(1);
  });

  test("sets busy timeout to 5000ms", () => {
    const db = getDatabase();
    const row = db.pragma("busy_timeout");
    expect(row).toBe(5000);
  });

  test("executes CREATE TABLE successfully", () => {
    const db = getDatabase();
    db.exec("CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)");
    const tables = db.prepare<{ name: string }[]>(
      "SELECT name FROM sqlite_master WHERE type='table' AND name='test'"
    ).all();
    expect(tables).toHaveLength(1);
    expect(tables[0].name).toBe("test");
  });

  test("inserts and retrieves rows", () => {
    const db = getDatabase();
    db.run("INSERT INTO test (name) VALUES (?)", ["alice"]);
    const row = db.prepare<{ id: number; name: string }>(
      "SELECT * FROM test WHERE name = ?"
    ).get("alice");
    expect(row).toBeDefined();
    expect(row!.name).toBe("alice");
  });

  test("transaction wraps atomic operations", () => {
    const db = getDatabase();
    db.transaction(() => {
      db.run("INSERT INTO test (name) VALUES (?)", ["tx1"]);
      db.run("INSERT INTO test (name) VALUES (?)", ["tx2"]);
    })();

    const rows = db.prepare<{ count: number }>(
      "SELECT COUNT(*) as count FROM test WHERE name IN ('tx1','tx2')"
    ).get();
    expect(rows!.count).toBe(2);
  });

  test("transaction rolls back on error", () => {
    const db = getDatabase();
    const initialCount = db.prepare<{ count: number }>(
      "SELECT COUNT(*) as count FROM test"
    ).get()!.count;

    expect(() => {
      db.transaction(() => {
        db.run("INSERT INTO test (name) VALUES (?)", ["rollback1"]);
        throw new Error("boom");
      })();
    }).toThrow("boom");

    const afterCount = db.prepare<{ count: number }>(
      "SELECT COUNT(*) as count FROM test"
    ).get()!.count;
    expect(afterCount).toBe(initialCount);
  });
});

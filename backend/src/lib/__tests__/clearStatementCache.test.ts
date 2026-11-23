// Deterministic failure-boundary coverage for `clearStatementCache`.
//
// `clearStatementCache` is the cache lifecycle boundary of `getPreparedStatement`:
// it must reset the statement cache and *all* observability counters without
// touching the underlying connection, and it must be safe to call repeatedly and
// when the cache is already empty. These tests exercise that contract directly
// against a real in-memory SQLite database so the assertions stay deterministic.
import {
  clearStatementCache,
  closeDatabase,
  getDatabase,
  getPreparedStatement,
  getStatementCacheStats,
  pingDatabase,
} from "../database";

beforeAll(() => {
  // A fresh in-memory database keeps this suite hermetic and side-effect free.
  process.env.DATABASE_PATH = ":memory:";
  closeDatabase();
  clearStatementCache();
});

beforeEach(() => {
  clearStatementCache();
});

afterAll(() => {
  clearStatementCache();
  closeDatabase();
  delete process.env.DATABASE_PATH;
});

describe("clearStatementCache deterministic failure boundaries", () => {
  it("empties the statement cache populated by getPreparedStatement", () => {
    getPreparedStatement("SELECT 1");
    getPreparedStatement("SELECT 2");

    const populated = getStatementCacheStats();
    expect(populated.size).toBe(2);
    expect(populated.statements.sort()).toEqual(["SELECT 1", "SELECT 2"]);

    clearStatementCache();

    const cleared = getStatementCacheStats();
    expect(cleared.size).toBe(0);
    expect(cleared.statements).toEqual([]);
  });

  it("resets hits, misses and evicts counters to zero", () => {
    getPreparedStatement("SELECT 100"); // miss
    getPreparedStatement("SELECT 100"); // hit

    const before = getStatementCacheStats();
    expect(before.misses).toBeGreaterThanOrEqual(1);
    expect(before.hits).toBeGreaterThanOrEqual(1);

    clearStatementCache();

    expect(getStatementCacheStats()).toEqual({
      size: 0,
      statements: [],
      hits: 0,
      misses: 0,
      evicts: 0,
    });
  });

  it("is idempotent and never throws when called repeatedly or on an empty cache", () => {
    expect(() => clearStatementCache()).not.toThrow();
    expect(() => clearStatementCache()).not.toThrow();
    expect(() => clearStatementCache()).not.toThrow();

    expect(getStatementCacheStats()).toMatchObject({
      size: 0,
      hits: 0,
      misses: 0,
      evicts: 0,
    });
  });

  it("forces the next lookup to re-prepare instead of serving a stale entry", () => {
    getPreparedStatement("SELECT 42");
    expect(getStatementCacheStats().misses).toBe(1);

    clearStatementCache();
    expect(getStatementCacheStats().misses).toBe(0);

    const reprepared = getPreparedStatement("SELECT 42");
    expect(reprepared).toBeDefined();
    // The cleared cache produced a miss again rather than a hit.
    expect(getStatementCacheStats().misses).toBe(1);
    expect(getStatementCacheStats().hits).toBe(0);
  });

  it("preserves the underlying database connection and its health", () => {
    const before = getDatabase();
    getPreparedStatement("SELECT 7");

    clearStatementCache();

    const after = getDatabase();
    expect(after).toBe(before);
    expect(pingDatabase()).toBe(true);
  });

  it("stays deterministic across repeated clear/prepare cycles", () => {
    for (let cycle = 0; cycle < 5; cycle++) {
      expect(() => getPreparedStatement("SELECT 9")).not.toThrow();
      expect(getStatementCacheStats().size).toBe(1);

      clearStatementCache();

      expect(getStatementCacheStats()).toEqual({
        size: 0,
        statements: [],
        hits: 0,
        misses: 0,
        evicts: 0,
      });
    }
  });

  it("remains safe after the database has been closed", () => {
    getPreparedStatement("SELECT 1");
    closeDatabase();

    expect(() => clearStatementCache()).not.toThrow();
    expect(getStatementCacheStats().size).toBe(0);
  });
});

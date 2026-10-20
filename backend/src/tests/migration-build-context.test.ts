/**
 * Deterministic failure-boundary coverage for `buildContext`
 * (and the `DatabaseClient` adapter it depends on) in
 * `src/lib/migrations/runner.ts`.
 *
 * Invariants under test are documented on `buildContext` itself. The suite is
 * split into two layers:
 *
 *  - A driver-free layer that drives `buildContext` with a recording fake
 *    `DatabaseClient`, so every routing/normalisation/transaction decision is
 *    observable without depending on SQLite behaviour.
 *  - A real better-sqlite3 layer that runs the fixture migrations end to end
 *    through `runMigrations`, proving the invariants hold against the actual
 *    driver used in production.
 */

import Database from "better-sqlite3";
import {
  buildContext,
  toDatabaseClient,
  assertDatabaseClient,
  REQUIRED_CLIENT_METHODS,
  runMigrations,
  loadMigrationsFromFS,
  validateMigrationFiles,
  verifyAppliedChecksums,
  getAppliedVersions,
  isDatabaseInitialized,
  type DatabaseClient,
  type DatabaseStatement,
} from "../lib/migrations/runner";
import { config } from "../config";
import type { MigrationContext } from "../lib/migrations/types";

const DatabaseConstructor = Database as any;
const FIXTURE_FILES: Record<string, string> = {
  probe: "v001_build_context_probe.ts",
  boom: "v002_build_context_boom.ts",
  surface: "v003_build_context_surface.ts",
  hotfix: "v004_build_context_hotfix.ts",
  forwardOnly: "v005_build_context_forward_only.ts",
  mismatch: "v007_build_context_mismatch.ts",
  unloadable: "v008_build_context_unloadable.ts",
  gap: "v009_build_context_gap.ts",
};

jest.mock("fs/promises", () => ({
  ...jest.requireActual("fs/promises"),
  readdir: jest.fn(async () => []),
}));
jest.mock("path", () => ({
  ...jest.requireActual("path"),
  resolve: jest.fn(() => jest.requireActual("path").join(__dirname, "fixtures", "migrations")),
}));

const mockedFs = jest.requireMock("fs/promises") as { readdir: jest.Mock };

const BEGIN = "BEGIN IMMEDIATE";
const COMMIT = "COMMIT";
const ROLLBACK = "ROLLBACK";

/** Only the listed fixture files are visible to the loader. */
function exposeFixtures(...names: (keyof typeof FIXTURE_FILES)[]): void {
  mockedFs.readdir.mockResolvedValue(names.map((n) => FIXTURE_FILES[n]));
}

/** Expose the same fixture twice, so the loader sees a duplicate version. */
function exposeDuplicate(name: keyof typeof FIXTURE_FILES): void {
  mockedFs.readdir.mockResolvedValue([FIXTURE_FILES[name], FIXTURE_FILES[name]]);
}

/**
 * Run `fn` with `config.NODE_ENV` forced to `value`.
 *
 * `runMigrations` reads `config.NODE_ENV` on every call (rather than caching it
 * at import time), so the production guard rails can be exercised without
 * re-importing the module graph.
 */
async function withNodeEnv(value: string, fn: () => Promise<void>): Promise<void> {
  const original = config.NODE_ENV;
  (config as { NODE_ENV: string }).NODE_ENV = value;
  try {
    await fn();
  } finally {
    (config as { NODE_ENV: string }).NODE_ENV = original;
  }
}

/**
 * Stub the approval-file probe used by the production guard rails: only the
 * listed file names resolve, everything else fails as if absent. The spy is
 * undone by the `afterEach` `jest.restoreAllMocks()`.
 */
function approve(files: string[]): void {
  jest.spyOn(mockedFs, "access").mockImplementation(async (target: any) => {
    if (files.some((f) => String(target).endsWith(f))) return undefined;
    const err: NodeJS.ErrnoException = new Error("ENOENT");
    err.code = "ENOENT";
    throw err;
  });
}

interface StatementScript {
  reader?: boolean;
  all?: unknown;
  get?: unknown;
  run?: unknown;
}

interface FakeClient {
  client: DatabaseClient;
  /** Every statement execution, in order: `${method} ${sql}`. */
  log: string[];
  /** Transaction-control statements (`BEGIN`/`COMMIT`/`SAVEPOINT`/...). */
  control: string[];
  /** Binds passed to each execution, keyed by log index. */
  binds: unknown[][];
  inFlight: number;
  maxInFlight: number;
  setScript(sql: string, script: StatementScript): void;
}

/**
 * Build a recording `DatabaseClient`. `transaction` is deliberately a *runner*
 * (invokes immediately) as the `DatabaseClient` contract requires, which is
 * what makes the old `buildContext` bug reproducible.
 *
 * `maxInFlight` counts simultaneously open transactions, derived from the
 * transaction-control statements the context emits (the context manages the
 * transaction itself with explicit BEGIN/COMMIT rather than via this method).
 */
function createFakeClient(options: { failOnControl?: RegExp } = {}): FakeClient {
  const scripts = new Map<string, StatementScript>();

  // `fake` is mutated in place (never re-spread) so that counters observed by
  // the test always reflect the latest call.
  const fake: FakeClient = {
    client: undefined as unknown as DatabaseClient,
    log: [],
    control: [],
    binds: [],
    inFlight: 0,
    maxInFlight: 0,
    setScript: (sql, script) => {
      scripts.set(sql, script);
    },
  };

  const statementFor = (sql: string): DatabaseStatement => {
    const script = scripts.get(sql) ?? {};
    const record = (method: string, result: unknown) => {
      fake.log.push(`${method} ${sql}`);
      return result;
    };
    return {
      all: () => record("all", script.all ?? []),
      get: () => record("get", script.get ?? undefined),
      run: () => record("run", script.run ?? { changes: 0, lastInsertRowId: 0 }),
      reader: script.reader,
    };
  };

  fake.client = {
    exec: (sql: string) => {
      fake.control.push(sql);
      if (sql === BEGIN) {
        fake.inFlight += 1;
        fake.maxInFlight = Math.max(fake.maxInFlight, fake.inFlight);
      }
      if (sql === COMMIT || sql === ROLLBACK) fake.inFlight -= 1;
      if (options.failOnControl?.test(sql)) throw new Error(`control failure: ${sql}`);
    },
    prepare: statementFor,
    transaction: (fn: () => void) => fn(),
  };

  return fake;
}

/** A real in-memory SQLite handle wrapped as a `DatabaseClient`. */
function createRealClient(): { client: DatabaseClient; raw: any } {
  const raw = new DatabaseConstructor(":memory:");
  raw.pragma("foreign_keys = ON");
  return { client: toDatabaseClient(raw), raw };
}

function readRows(client: DatabaseClient, sql: string): any[] {
  return (client.prepare(sql).all() as any[]) ?? [];
}

beforeEach(() => {
  jest.clearAllMocks();
  mockedFs.readdir.mockResolvedValue([]);
  jest.spyOn(console, "log").mockImplementation(() => undefined);
  jest.spyOn(console, "warn").mockImplementation(() => undefined);
  jest.spyOn(console, "error").mockImplementation(() => undefined);
});

afterEach(() => {
  jest.restoreAllMocks();
});

describe("buildContext — context contract", () => {
  test("exposes a complete db handle plus environment flags", () => {
    const { client } = createFakeClient();
    const ctx = buildContext(client, false);

    expect(Object.keys(ctx.db).sort()).toEqual(["exec", "get", "run", "transaction"]);
    expect(typeof ctx.db.exec).toBe("function");
    expect(typeof ctx.db.get).toBe("function");
    expect(typeof ctx.db.run).toBe("function");
    expect(typeof ctx.db.transaction).toBe("function");
    expect(ctx.env).toBe(process.env);
    expect(ctx.isProduction).toBe(false);
    expect(ctx.isTest).toBe(true);
  });

  test("isProduction is taken from the argument, not from ambient NODE_ENV", () => {
    const { client } = createFakeClient();
    expect(buildContext(client, true).isProduction).toBe(true);
    expect(buildContext(client, false).isProduction).toBe(false);
  });

  test("runs no SQL while constructing the context (fail fast, no side effects)", () => {
    const fake = createFakeClient();
    buildContext(fake.client, false);
    expect(fake.log).toEqual([]);
    expect(fake.control).toEqual([]);
  });

  test("each buildContext call yields an independent handle", async () => {
    const fake = createFakeClient();
    const a = buildContext(fake.client, false);
    const b = buildContext(fake.client, false);

    await a.db.exec("CREATE TABLE t (x INTEGER)");
    expect(fake.log).toEqual(["run CREATE TABLE t (x INTEGER)"]);
    await b.db.exec("CREATE TABLE t (x INTEGER)");
    expect(fake.log).toHaveLength(2);
    expect(a.db).not.toBe(b.db);
  });
});

describe("buildContext — statement routing is deterministic and single-execution", () => {
  test("DDL through exec uses run() once and resolves to an empty array", async () => {
    const fake = createFakeClient();
    const ctx = buildContext(fake.client, false);

    await expect(ctx.db.exec("CREATE TABLE t (x INTEGER)")).resolves.toEqual([]);
    expect(fake.log).toEqual(["run CREATE TABLE t (x INTEGER)"]);
  });

  test.each([
    ["SELECT 1 AS one", "SELECT"],
    ["PRAGMA table_info(t)", "PRAGMA"],
    ["VALUES (1)", "VALUES"],
    ["EXPLAIN SELECT 1", "EXPLAIN"],
    ["WITH x AS (SELECT 1 AS one) SELECT * FROM x", "WITH"],
  ])("row-returning SQL %s routes to all()", async (sql) => {
    const fake = createFakeClient();
    fake.setScript(sql, { all: [{ one: 1 }] });
    const ctx = buildContext(fake.client, false);

    await expect(ctx.db.exec(sql)).resolves.toEqual([{ one: 1 }]);
    expect(fake.log).toEqual([`all ${sql}`]);
  });

  test("leading comments and whitespace do not change routing", async () => {
    const fake = createFakeClient();
    // DDL behind a leading comment must still route to run().
    const ddl = "-- nightly backfill\n  ALTER TABLE t ADD COLUMN y TEXT";
    fake.setScript(ddl, { all: [{ should: "not run" }] });
    // A read behind a leading comment (and indentation) must still route to all().
    const select = "-- nightly report\n  SELECT 1 AS one";
    fake.setScript(select, { all: [{ one: 1 }] });
    const ctx = buildContext(fake.client, false);

    await expect(ctx.db.exec(ddl)).resolves.toEqual([]);
    await expect(ctx.db.exec(select)).resolves.toEqual([{ one: 1 }]);
    expect(fake.log).toEqual([`run ${ddl}`, `all ${select}`]);
  });

  test("whitespace and blank lines between comment and keyword are tolerated", async () => {
    const fake = createFakeClient();
    const sql = "/* report */\n\n   -- indented\n\tSELECT 1 AS one";
    fake.setScript(sql, { all: [{ one: 1 }] });
    const ctx = buildContext(fake.client, false);

    await expect(ctx.db.exec(sql)).resolves.toEqual([{ one: 1 }]);
    expect(fake.log).toEqual([`all ${sql}`]);
  });

  test("the driver `reader` flag wins over keyword classification", async () => {
    const fake = createFakeClient();
    // A driver that reports a SELECT as non-reading must not be second-guessed.
    fake.setScript("SELECT 1 AS one", { reader: false, run: { changes: 3, lastInsertRowId: 7 } });
    const ctx = buildContext(fake.client, false);

    await expect(ctx.db.exec("SELECT 1 AS one")).resolves.toEqual([]);
    expect(fake.log).toEqual(["run SELECT 1 AS one"]);
  });

  test("a non-array result from all() normalizes to an empty array", async () => {
    const fake = createFakeClient();
    fake.setScript("SELECT 1 AS one", { all: undefined });
    const ctx = buildContext(fake.client, false);

    await expect(ctx.db.exec("SELECT 1 AS one")).resolves.toEqual([]);
  });

  test("get returns the row, and null (no row) normalizes to undefined", async () => {
    const fake = createFakeClient();
    fake.setScript("SELECT 1 AS one", { get: { one: 1 } });
    fake.setScript("SELECT 2 AS two", { get: null });
    const ctx = buildContext(fake.client, false);

    await expect(ctx.db.get("SELECT 1 AS one")).resolves.toEqual({ one: 1 });
    await expect(ctx.db.get("SELECT 2 AS two")).resolves.toBeUndefined();
  });

  test("run normalizes missing or non-numeric driver counters to 0", async () => {
    const fake = createFakeClient();
    fake.setScript("UPDATE t SET x = 1", { run: { changes: 2, lastInsertRowId: 9 } });
    fake.setScript("UPDATE t SET x = 2", { run: {} });
    fake.setScript("UPDATE t SET x = 3", { run: undefined });
    fake.setScript("UPDATE t SET x = 4", { run: { changes: "5", lastInsertRowId: null } });
    const ctx = buildContext(fake.client, false);

    await expect(ctx.db.run("UPDATE t SET x = 1")).resolves.toEqual({ changes: 2, lastInsertRowId: 9 });
    await expect(ctx.db.run("UPDATE t SET x = 2")).resolves.toEqual({ changes: 0, lastInsertRowId: 0 });
    await expect(ctx.db.run("UPDATE t SET x = 3")).resolves.toEqual({ changes: 0, lastInsertRowId: 0 });
    await expect(ctx.db.run("UPDATE t SET x = 4")).resolves.toEqual({ changes: 0, lastInsertRowId: 0 });
  });

  test("bind parameters are forwarded verbatim, and omitted params become none", async () => {
    const seen: unknown[][] = [];
    const client: DatabaseClient = {
      exec: () => undefined,
      prepare: (sql: string) => ({
        all: (params?: unknown[]) => {
          seen.push(params ?? []);
          return [];
        },
        get: (params?: unknown[]) => {
          seen.push(params ?? []);
          return undefined;
        },
        run: (params?: unknown[]) => {
          seen.push(params ?? []);
          return { changes: 0, lastInsertRowId: 0 };
        },
      }),
      transaction: (fn: () => void) => fn(),
    };
    const ctx = buildContext(client, false);

    await ctx.db.run("INSERT INTO t VALUES (?, ?)", ["a", 1]);
    await ctx.db.run("INSERT INTO t VALUES (?)");
    await ctx.db.get("SELECT 1");
    await ctx.db.exec("SELECT 1");

    expect(seen).toEqual([["a", 1], [], [], []]);
  });
});

describe("buildContext — invalid clients are rejected deterministically", () => {
  test.each([
    ["null", null],
    ["undefined", undefined],
    ["a string", "not-a-client"],
    ["a number", 42],
    ["an empty object", {}],
  ])("buildContext rejects %s", (_label, value) => {
    expect(() => buildContext(value as unknown as DatabaseClient, false)).toThrow(TypeError);
  });

  test("the error names every missing member exactly once", () => {
    let caught: TypeError | undefined;
    try {
      buildContext({} as unknown as DatabaseClient, false);
    } catch (err) {
      caught = err as TypeError;
    }

    expect(caught).toBeInstanceOf(TypeError);
    for (const member of REQUIRED_CLIENT_METHODS) {
      expect(caught!.message).toContain(member);
      expect(caught!.message.split(member).length - 1).toBe(1);
    }
  });

  test("a partially valid client reports only what is actually missing", () => {
    const partial = { exec: () => undefined, prepare: () => undefined } as unknown as DatabaseClient;
    expect(() => buildContext(partial, false)).toThrow(/transaction/);
    expect(() => buildContext(partial, false)).not.toThrow(/exec/);
  });

  test.each(["exec", "prepare", "transaction"])(
    "a non-callable %s is rejected",
    (member) => {
      const broken: Record<string, unknown> = {
        exec: () => undefined,
        prepare: () => undefined,
        transaction: () => undefined,
      };
      broken[member] = "definitely-not-a-function";
      expect(() => buildContext(broken as unknown as DatabaseClient, false)).toThrow(
        new RegExp(member)
      );
    }
  );

  test("validation errors never leak environment values or SQL text", () => {
    process.env.QUICKLENDX_TEST_SECRET = "super-secret-value-1234";
    try {
      let message = "";
      try {
        buildContext({} as unknown as DatabaseClient, false);
      } catch (err) {
        message = (err as Error).message;
      }
      expect(message).not.toContain("super-secret-value-1234");
      expect(message).not.toMatch(/CREATE TABLE|SELECT|INSERT/);
    } finally {
      delete process.env.QUICKLENDX_TEST_SECRET;
    }
  });

  test("assertDatabaseClient narrows valid clients and rejects invalid ones", () => {
    const { client } = createFakeClient();
    expect(() => assertDatabaseClient(client)).not.toThrow();
    expect(() => assertDatabaseClient(null)).toThrow(TypeError);
  });
});

describe("buildContext — driver errors propagate unchanged", () => {
  const boom = new Error("SQLITE_CONSTRAINT: UNIQUE constraint failed");

  test("errors from exec/get/run surface to the caller", async () => {
    const client: DatabaseClient = {
      exec: () => {
        throw boom;
      },
      prepare: () => ({
        all: () => {
          throw boom;
        },
        get: () => {
          throw boom;
        },
        run: () => {
          throw boom;
        },
      }),
      transaction: (fn: () => void) => fn(),
    };
    const ctx = buildContext(client, false);

    await expect(ctx.db.exec("SELECT 1")).rejects.toBe(boom);
    await expect(ctx.db.get("SELECT 1")).rejects.toBe(boom);
    await expect(ctx.db.run("SELECT 1")).rejects.toBe(boom);
  });

  test("an error thrown by prepare() is not swallowed", async () => {
    const client: DatabaseClient = {
      exec: () => undefined,
      prepare: () => {
        throw boom;
      },
      transaction: (fn: () => void) => fn(),
    };
    await expect(buildContext(client, false).db.get("SELECT 1")).rejects.toBe(boom);
  });
});

describe("buildContext — transaction scope survives await and rolls back on failure", () => {
  test("commits only after the async body settles", async () => {
    const fake = createFakeClient();
    const ctx = buildContext(fake.client, false);
    const order: string[] = [];

    const result = await ctx.db.transaction(async () => {
      order.push("body-start");
      await new Promise((resolve) => setTimeout(resolve, 5));
      order.push("body-end");
      return "done";
    });

    expect(result).toBe("done");
    expect(order).toEqual(["body-start", "body-end"]);
    expect(fake.control).toEqual([BEGIN, COMMIT]);
  });

  test("a throwing body rolls back, never commits, and rethrows the original error", async () => {
    const fake = createFakeClient();
    const ctx = buildContext(fake.client, false);
    const boom = new Error("migration exploded");

    await expect(
      ctx.db.transaction(async () => {
        await Promise.resolve();
        throw boom;
      })
    ).rejects.toBe(boom);

    expect(fake.control).toEqual([BEGIN, ROLLBACK]);
    expect(fake.control).not.toContain(COMMIT);
  });

  test("a synchronously throwing body also rolls back", async () => {
    const fake = createFakeClient();
    const ctx = buildContext(fake.client, false);
    const boom = new Error("sync failure");

    // `transaction` is always async: a synchronous throw inside the callback is
    // still reported as a rejection, after the rollback has run.
    await expect(
      ctx.db.transaction(() => {
        throw boom;
      })
    ).rejects.toBe(boom);
    expect(fake.control).toEqual([BEGIN, ROLLBACK]);
  });

  test("a failing ROLLBACK does not mask the original error", async () => {
    const fake = createFakeClient({ failOnControl: /^ROLLBACK$/ });
    const ctx = buildContext(fake.client, false);
    const boom = new Error("root cause");

    await expect(
      ctx.db.transaction(async () => {
        throw boom;
      })
).rejects.toBe(boom);
  });

  test("a rejected COMMIT is rolled back so no transaction is left open", async () => {
    const fake = createFakeClient({ failOnControl: /^COMMIT$/ });
    const ctx = buildContext(fake.client, false);

    await expect(ctx.db.transaction(async () => "value")).rejects.toThrow(/COMMIT/);
    expect(fake.control).toEqual([BEGIN, COMMIT, ROLLBACK]);
  });

  test("a failing rollback after a rejected COMMIT does not mask the commit error", async () => {
    const fake = createFakeClient({ failOnControl: /^(COMMIT|ROLLBACK)$/ });
    const ctx = buildContext(fake.client, false);

    await expect(ctx.db.transaction(async () => "value")).rejects.toThrow(/COMMIT/);
    expect(fake.control).toEqual([BEGIN, COMMIT, ROLLBACK]);
  });

  test("the per-client lock is released even when COMMIT fails", async () => {
    const fake = createFakeClient({ failOnControl: /^COMMIT$/ });
    const ctx = buildContext(fake.client, false);

    await expect(ctx.db.transaction(async () => "value")).rejects.toThrow(/COMMIT/);
    // A later transaction must not deadlock on a lock the failed run already
    // released: it reaches its own BEGIN and its own (here failing) COMMIT.
    await expect(ctx.db.transaction(async () => "second")).rejects.toThrow(/COMMIT/);
    expect(fake.control).toEqual([BEGIN, COMMIT, ROLLBACK, BEGIN, COMMIT, ROLLBACK]);
    expect(fake.maxInFlight).toBe(1);
  });

  test("the callback receives the adapted handle, not the raw client", async () => {
    const fake = createFakeClient();
    const ctx = buildContext(fake.client, false);
    let received: unknown;

    await ctx.db.transaction(async (tx) => {
      received = tx;
    });

    expect(received).toBe(ctx.db);
    await (received as MigrationContext["db"]).exec("CREATE TABLE nested (x INTEGER)");
    expect(fake.log).toEqual(["run CREATE TABLE nested (x INTEGER)"]);
  });

  test("nested transactions use savepoints and keep the outer scope intact", async () => {
    const fake = createFakeClient();
    const ctx = buildContext(fake.client, false);

    await ctx.db.transaction(async () => {
      await ctx.db.transaction(async () => {
        await ctx.db.exec("CREATE TABLE inner (x INTEGER)");
      });
    });

    const control = fake.control;
    expect(control[0]).toBe(BEGIN);
    expect(control.filter((c) => c.startsWith("SAVEPOINT"))).toHaveLength(1);
    expect(control.filter((c) => c.startsWith("RELEASE"))).toHaveLength(1);
    expect(control[control.length - 1]).toBe(COMMIT);
  });

  test("a failing nested transaction rolls back to its savepoint only", async () => {
    const fake = createFakeClient();
    const ctx = buildContext(fake.client, false);
    const boom = new Error("inner failure");

    await expect(
      ctx.db.transaction(async () => {
        await ctx.db.exec("CREATE TABLE outer_work (x INTEGER)");
        await ctx.db.transaction(async () => {
          await ctx.db.exec("CREATE TABLE inner_work (x INTEGER)");
          throw boom;
        });
      })
    ).rejects.toBe(boom);

    expect(fake.control[0]).toBe(BEGIN);
    expect(fake.control).toContain("ROLLBACK TO qlx_migration_sp_1");
    expect(fake.control).not.toContain(COMMIT);
  });

  test("savepoint names are derived from nesting depth, never from caller input", async () => {
    const fake = createFakeClient();
    const ctx = buildContext(fake.client, false);

    await ctx.db.transaction(async () => {
      await ctx.db.transaction(async () => {
        await ctx.db.transaction(async () => undefined);
      });
    });

    const savepoints = fake.control.filter((c) => c.startsWith("SAVEPOINT"));
    expect(savepoints).toEqual(["SAVEPOINT qlx_migration_sp_1", "SAVEPOINT qlx_migration_sp_2"]);
    for (const name of savepoints) {
      expect(name).toMatch(/^SAVEPOINT qlx_migration_sp_[1-9][0-9]*$/);
    }
  });

  test("sibling nested transactions reuse the freed savepoint name safely", async () => {
    const fake = createFakeClient();
    const ctx = buildContext(fake.client, false);

    await ctx.db.transaction(async () => {
      await ctx.db.transaction(async () => undefined);
      await ctx.db.transaction(async () => undefined);
    });

    expect(fake.control.filter((c) => c.startsWith("SAVEPOINT"))).toHaveLength(2);
    expect(fake.control.filter((c) => c === COMMIT)).toHaveLength(1);
  });
});

describe("buildContext — concurrency boundaries", () => {
  test("concurrent transactions on the same client are serialized", async () => {
    const fake = createFakeClient();
    const ctx = buildContext(fake.client, false);
    const order: string[] = [];

    const slow = async (label: string, delay: number) =>
      ctx.db.transaction(async () => {
        order.push(`${label}:enter`);
        await new Promise((resolve) => setTimeout(resolve, delay));
        order.push(`${label}:exit`);
      });

    await Promise.all([slow("a", 20), slow("b", 1)]);

    // `b` is faster but must not interleave with `a`.
    expect(order).toEqual(["a:enter", "a:exit", "b:enter", "b:exit"]);
    expect(fake.maxInFlight).toBe(1);
  });

  test("transactions on different clients run in parallel", async () => {
    const first = createFakeClient();
    const second = createFakeClient();
    const ctxA = buildContext(first.client, false);
    const ctxB = buildContext(second.client, false);

    await Promise.all([
      ctxA.db.transaction(async () => {
        await new Promise((resolve) => setTimeout(resolve, 5));
      }),
      ctxB.db.transaction(async () => {
        await Promise.resolve();
      }),
    ]);

    expect(first.maxInFlight).toBe(1);
    expect(second.maxInFlight).toBe(1);
  });

  test("the lock is released after a failure so later runs still proceed", async () => {
    const fake = createFakeClient();
    const ctx = buildContext(fake.client, false);

    await expect(
      ctx.db.transaction(async () => {
        throw new Error("first fails");
      })
    ).rejects.toThrow("first fails");
    await expect(ctx.db.transaction(async () => "second ok")).resolves.toBe("second ok");
    expect(fake.control).toEqual([BEGIN, ROLLBACK, BEGIN, COMMIT]);
  });
});

describe("toDatabaseClient — adapts a raw better-sqlite3 handle", () => {
  test("bridges variadic statements and the transaction factory", async () => {
    const { client, raw } = createRealClient();
    const ctx = buildContext(client, false);

    await ctx.db.exec("CREATE TABLE t (id INTEGER PRIMARY KEY, label TEXT NOT NULL)");
    await expect(ctx.db.run("INSERT INTO t (id, label) VALUES (?, ?)", [1, "one"])).resolves.toEqual({
      changes: 1,
      lastInsertRowId: 1,
    });
    await expect(ctx.db.get("SELECT label FROM t WHERE id = ?", [1])).resolves.toEqual({ label: "one" });
    await expect(ctx.db.exec("PRAGMA table_info(t)")).resolves.toHaveLength(2);
    await expect(ctx.db.get("SELECT label FROM t WHERE id = ?", [999])).resolves.toBeUndefined();
    expect(raw.prepare("SELECT 1").reader).toBe(true);
  });

  test("DDL through exec does not throw 'This statement does not return data'", async () => {
    const { client } = createRealClient();
    const ctx = buildContext(client, false);

    await ctx.db.exec("CREATE TABLE t (id INTEGER)");
    await expect(ctx.db.exec("ALTER TABLE t ADD COLUMN extra TEXT")).resolves.toEqual([]);
    await expect(ctx.db.exec("UPDATE t SET extra = 'x' WHERE id IS NULL")).resolves.toEqual([]);
  });

  test("transaction control maps onto BEGIN/COMMIT/ROLLBACK", async () => {
    const { client, raw } = createRealClient();
    const ctx = buildContext(client, false);

    await ctx.db.transaction(async () => {
      await ctx.db.exec("CREATE TABLE committed (x INTEGER)");
    });
    expect(
      raw.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='committed'").get()
    ).toEqual({ name: "committed" });

    await expect(
      ctx.db.transaction(async () => {
        await ctx.db.exec("CREATE TABLE rolled_back (x INTEGER)");
        throw new Error("nope");
      })
    ).rejects.toThrow("nope");
    expect(
      raw.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='rolled_back'").get()
    ).toBeUndefined();
  });

  test("rejects handles that are not raw databases", () => {
    expect(() => toDatabaseClient(null as any)).toThrow(TypeError);
    expect(() => toDatabaseClient({ exec: () => undefined } as any)).toThrow(/prepare|transaction/);
  });
});

describe("runMigrations — end-to-end over real SQLite", () => {
  test("applies a migration and records it exactly once", async () => {
    exposeFixtures("probe");
    const { client, raw } = createRealClient();

    const result = await runMigrations({ db: client, verbose: true });

    expect(result.applied).toHaveLength(1);
    expect(result.applied[0]).toMatchObject({ version: 1, name: "build_context_probe" });
    expect(result.applied[0].checksum).toMatch(/^[a-f0-9]{64}$/);
    expect(readRows(client, "SELECT COUNT(*) AS c FROM _migrations WHERE version = 1")[0].c).toBe(1);
    expect(raw.prepare("SELECT label FROM probe_table WHERE id = 1").get()).toEqual({
      label: "row-1",
    });
  });

  test("re-running is idempotent: the second run skips and never duplicates", async () => {
    exposeFixtures("probe");
    const { client } = createRealClient();

    await runMigrations({ db: client });
    const second = await runMigrations({ db: client, verbose: true });

    expect(second.applied).toHaveLength(0);
    // Already-applied versions are filtered out before the loop, so nothing is
    // re-evaluated and no second bookkeeping row can be written.
    expect(second.skipped).toBe(0);
    expect(readRows(client, "SELECT COUNT(*) AS c FROM _migrations")[0].c).toBe(1);
  });

  test("a failing migration leaves no _migrations row and no partial schema", async () => {
    exposeFixtures("boom");
    const { client, raw } = createRealClient();

    await expect(runMigrations({ db: client })).rejects.toThrow("deliberate migration failure");

    expect(readRows(client, "SELECT COUNT(*) AS c FROM _migrations")).toEqual([{ c: 0 }]);
    expect(
      raw.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='boom_table'").get()
    ).toBeUndefined();
  });

  test("failure recovery: a retry after a fix applies cleanly", async () => {
    exposeFixtures("boom");
    const { client } = createRealClient();

    await expect(runMigrations({ db: client })).rejects.toThrow("deliberate migration failure");
    exposeFixtures("probe");
    const recovered = await runMigrations({ db: client });

    expect(recovered.applied.map((m) => m.version)).toEqual([1]);
    expect(readRows(client, "SELECT COUNT(*) AS c FROM _migrations")[0].c).toBe(1);
  });

  test("the verbose path reports a real duration (regression: out-of-scope binding)", async () => {
    exposeFixtures("probe");
    const { client } = createRealClient();

    const result = await runMigrations({ db: client, verbose: true });

    const logged = (console.log as jest.Mock).mock.calls.map((c) => String(c[0]));
    expect(logged.some((line) => /Applied migration 1_build_context_probe \(\d+ms\)/.test(line))).toBe(
      true
    );
    expect(result.applied[0].durationMs).toBeGreaterThanOrEqual(0);
  });

  test("dry-run reports migrations without touching the database", async () => {
    exposeFixtures("probe");
    const { client, raw } = createRealClient();

    const result = await runMigrations({ db: client, dryRun: true, verbose: true });

    expect(result.applied).toHaveLength(1);
    expect(result.applied[0].checksum).toBe("(dry-run)");
    expect(readRows(client, "SELECT COUNT(*) AS c FROM _migrations")[0].c).toBe(0);
    expect(
      raw.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='probe_table'").get()
    ).toBeUndefined();
  });

  test("concurrent runs on the same client apply the version once, without corrupting state", async () => {
    exposeFixtures("probe");
    const { client } = createRealClient();

    const results = await Promise.all([runMigrations({ db: client }), runMigrations({ db: client })]);

    const appliedCount = results.reduce((sum, r) => sum + r.applied.length, 0);
    expect(appliedCount).toBe(1);
    expect(readRows(client, "SELECT COUNT(*) AS c FROM _migrations")[0].c).toBe(1);
    expect(readRows(client, "SELECT COUNT(*) AS c FROM probe_table")[0].c).toBe(1);
  });

  test("a migration sees a fully working context: handle, flags and explicit transactions", async () => {
    exposeFixtures("surface");
    const { client } = createRealClient();

    await runMigrations({ db: client });

    const values = new Map(
      readRows(client, "SELECT key, value FROM context_surface").map((r: any) => [r.key, r.value])
    );
    expect(values.get("handle_keys")).toBe("exec,get,run,transaction");
    expect(values.get("is_production")).toBe("false");
    expect(values.get("is_test")).toBe("true");
    expect(values.get("has_env")).toBe("true");
    expect(values.get("nested")).toBe("ok");
    expect(values.get("pragma_columns")).toBe("2");
    // The read happens after 4 inserts; rows_in_tx, nested and pragma_columns
    // are written afterwards, for 7 rows in total.
    expect(values.get("rows_in_tx")).toBe("4");
    expect(readRows(client, "SELECT COUNT(*) AS c FROM context_surface")[0].c).toBe(7);
  });

  test("down migrations roll back and remove the bookkeeping row", async () => {
    exposeFixtures("probe");
    const { client, raw } = createRealClient();

    await runMigrations({ db: client });
    const result = await runMigrations({ db: client, allowDown: true, verbose: true });

    expect(result.applied.map((m) => m.version)).toEqual([1]);
    expect(readRows(client, "SELECT COUNT(*) AS c FROM _migrations")[0].c).toBe(0);
    expect(
      raw.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='probe_table'").get()
    ).toBeUndefined();
  });

  test("dry-run of a down migration leaves both schema and bookkeeping intact", async () => {
    exposeFixtures("probe");
    const { client, raw } = createRealClient();

    await runMigrations({ db: client });
    const result = await runMigrations({ db: client, allowDown: true, dryRun: true, verbose: true });

    expect(result.applied).toHaveLength(1);
    expect(readRows(client, "SELECT COUNT(*) AS c FROM _migrations")[0].c).toBe(1);
    expect(
      raw.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='probe_table'").get()
    ).toEqual({ name: "probe_table" });
  });

  test("an invalid injected client is rejected before any SQL is issued", async () => {
    exposeFixtures("probe");
    let caught: unknown;
    try {
      await runMigrations({ db: {} as DatabaseClient });
    } catch (err) {
      caught = err;
    }
    expect(caught).toBeInstanceOf(TypeError);
    expect((caught as Error).message).toContain("exec");
  });
});

describe("runMigrations — production guard rails", () => {
  test("checksum verification cannot be bypassed in production", async () => {
    exposeFixtures("probe");
    const { client } = createRealClient();

    await withNodeEnv("production", async () => {
      await expect(runMigrations({ db: client, skipChecksumVerify: true })).rejects.toThrow(
        /cannot be bypassed in production/
      );
    });

    // The guard fires before any migration is applied.
    expect(readRows(client, "SELECT COUNT(*) AS c FROM _migrations")).toEqual([{ c: 0 }]);
  });

  test("a non-hotfix migration needs no approval to run in production", async () => {
    exposeFixtures("probe");
    const { client } = createRealClient();

    await withNodeEnv("production", async () => {
      const result = await runMigrations({ db: client });
      expect(result.applied.map((m) => m.version)).toEqual([1]);
    });
  });

  test("a hotfix without production approval is refused and applied nothing", async () => {
    exposeFixtures("hotfix");
    approve([]);
    const { client, raw } = createRealClient();

    await withNodeEnv("production", async () => {
      await expect(runMigrations({ db: client })).rejects.toThrow(/lacks production approval/);
    });

    expect(readRows(client, "SELECT COUNT(*) AS c FROM _migrations")).toEqual([{ c: 0 }]);
    expect(
      raw.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='hotfix_table'").get()
    ).toBeUndefined();
  });

  test("a hotfix with production approval applies and reports validate warnings", async () => {
    exposeFixtures("hotfix");
    approve(["4_build_context_hotfix.approval"]);
    const { client } = createRealClient();

    await withNodeEnv("production", async () => {
      const result = await runMigrations({ db: client, verbose: true });
      expect(result.applied.map((m) => m.version)).toEqual([4]);
    });

    // `validate` ran before `up` and its warning was surfaced, not thrown.
    const warned = (console.warn as jest.Mock).mock.calls.map((c) => String(c[0]));
    expect(warned.some((line) => /validation warnings/.test(line))).toBe(true);
    expect(readRows(client, "SELECT COUNT(*) AS c FROM hotfix_table")).toEqual([{ c: 1 }]);
  });

  test("a production rollback without approval is refused and keeps the data", async () => {
    exposeFixtures("probe");
    approve([]);
    const { client } = createRealClient();

    await runMigrations({ db: client });
    await withNodeEnv("production", async () => {
      await expect(runMigrations({ db: client, allowDown: true })).rejects.toThrow(
        /requires production approval/
      );
    });

    // Nothing was rolled back: both the schema and the bookkeeping survive.
    expect(readRows(client, "SELECT COUNT(*) AS c FROM _migrations")).toEqual([{ c: 1 }]);
    expect(readRows(client, "SELECT COUNT(*) AS c FROM probe_table")).toEqual([{ c: 1 }]);
  });

  test("a production rollback with approval succeeds", async () => {
    exposeFixtures("probe");
    approve(["rollback_1_build_context_probe.approval"]);
    const { client } = createRealClient();

    await runMigrations({ db: client });
    await withNodeEnv("production", async () => {
      const result = await runMigrations({ db: client, allowDown: true });
      expect(result.applied.map((m) => m.version)).toEqual([1]);
    });

    expect(readRows(client, "SELECT COUNT(*) AS c FROM _migrations")).toEqual([{ c: 0 }]);
  });

  test("a migration with no down function is refused, not silently skipped", async () => {
    exposeFixtures("forwardOnly");
    const { client } = createRealClient();

    await runMigrations({ db: client });
    await expect(runMigrations({ db: client, allowDown: true })).rejects.toThrow(/has no down function/);

    // The guard fires before the transaction opens, so state is untouched.
    expect(readRows(client, "SELECT COUNT(*) AS c FROM _migrations")).toEqual([{ c: 1 }]);
    expect(readRows(client, "SELECT COUNT(*) AS c FROM forward_only_table")).toEqual([{ c: 0 }]);
  });
});

describe("runMigrations — checksum integrity boundaries", () => {
  test("a tampered bookkeeping row fails the run before applying anything", async () => {
    exposeFixtures("probe");
    const { client } = createRealClient();

    await runMigrations({ db: client });
    client
      .prepare("UPDATE _migrations SET checksum = ? WHERE version = ?")
      .run(["0".repeat(64), 1]);

    await expect(runMigrations({ db: client })).rejects.toThrow(/checksum verification failed/i);
    expect(readRows(client, "SELECT COUNT(*) AS c FROM _migrations")).toEqual([{ c: 1 }]);
  });

  test("verifyAppliedChecksums names the migration and both checksums", async () => {
    exposeFixtures("probe");
    const { client } = createRealClient();
    await runMigrations({ db: client });
    client.prepare("UPDATE _migrations SET checksum = ? WHERE version = ?").run(["abc123", 1]);

    const result = await verifyAppliedChecksums(client);

    expect(result.valid).toBe(false);
    expect(result.errors).toHaveLength(1);
    expect(result.errors[0]).toContain("1_build_context_probe");
    expect(result.errors[0]).toContain("abc123");
  });

  test("verifyAppliedChecksums reports a migration that vanished from disk", async () => {
    exposeFixtures("probe");
    const { client } = createRealClient();
    await runMigrations({ db: client });

    // The bookkeeping row survives a deploy that dropped the migration file.
    exposeFixtures();
    const result = await verifyAppliedChecksums(client);

    expect(result.valid).toBe(false);
    expect(result.errors[0]).toMatch(/not found in filesystem/);
  });

  test("verifyAppliedChecksums passes for an untouched, consistently applied set", async () => {
    exposeFixtures("probe");
    const { client } = createRealClient();
    await runMigrations({ db: client });

    await expect(verifyAppliedChecksums(client)).resolves.toEqual({ valid: true, errors: [] });
  });
});

describe("loadMigrationsFromFS — malformed migration sources", () => {
  test("a missing migrations directory yields an empty list rather than throwing", async () => {
    const enoent: NodeJS.ErrnoException = new Error("ENOENT");
    enoent.code = "ENOENT";
    mockedFs.readdir.mockRejectedValue(enoent);

    await expect(loadMigrationsFromFS()).resolves.toEqual([]);
  });

  test("an unexpected readdir error propagates", async () => {
    const denied: NodeJS.ErrnoException = new Error("EACCES");
    denied.code = "EACCES";
    mockedFs.readdir.mockRejectedValue(denied);

    await expect(loadMigrationsFromFS()).rejects.toThrow("EACCES");
  });

  test("a file that cannot be loaded reports the offending file name", async () => {
    exposeFixtures("unloadable");

    await expect(loadMigrationsFromFS()).rejects.toThrow(
      /Failed to load migration v008_build_context_unloadable\.ts/
    );
  });

  test("a version that disagrees with the filename is rejected", async () => {
    exposeFixtures("mismatch");

    await expect(loadMigrationsFromFS()).rejects.toThrow(/Version mismatch in .*filename 7 but export 8/);
  });

  test("files that are not migrations are ignored, and versions are ordered", async () => {
    mockedFs.readdir.mockResolvedValue([
      "README.md",
      FIXTURE_FILES.surface,
      FIXTURE_FILES.probe,
      "not-a-migration.ts",
    ]);

    const migrations = await loadMigrationsFromFS();

    expect(migrations.map((m) => m.version)).toEqual([1, 3]);
  });
});

describe("validateMigrationFiles — version-set boundaries", () => {
  test("a contiguous set is valid", async () => {
    exposeFixtures("probe", "boom", "surface");

    await expect(validateMigrationFiles()).resolves.toEqual({ valid: true, errors: [] });
  });

  test("a gap is reported with the missing version", async () => {
    exposeFixtures("probe", "gap");

    const result = await validateMigrationFiles();

    expect(result.valid).toBe(false);
    expect(result.errors).toEqual(["Gap detected: migration 2 is missing"]);
  });

  test("a duplicate version is reported", async () => {
    exposeDuplicate("probe");

    const result = await validateMigrationFiles();

    expect(result.valid).toBe(false);
    expect(result.errors).toContain("Duplicate version numbers detected");
  });
});

describe("applied-migration introspection", () => {
  test("getAppliedVersions returns applied versions in ascending order", async () => {
    exposeFixtures("surface", "probe");
    const { client } = createRealClient();

    await expect(getAppliedVersions(client)).resolves.toEqual([]);
    await runMigrations({ db: client });

    await expect(getAppliedVersions(client)).resolves.toEqual([1, 3]);
  });

  test("isDatabaseInitialized flips from false to true once a migration lands", async () => {
    exposeFixtures("probe");
    const { client } = createRealClient();

    await expect(isDatabaseInitialized(client)).resolves.toBe(false);
    await runMigrations({ db: client });
    await expect(isDatabaseInitialized(client)).resolves.toBe(true);
  });

  test("a rolled-back database reports itself uninitialised again", async () => {
    exposeFixtures("probe");
    const { client } = createRealClient();

    await runMigrations({ db: client });
    await runMigrations({ db: client, allowDown: true });

    await expect(isDatabaseInitialized(client)).resolves.toBe(false);
  });
});

describe("toDatabaseClient — DatabaseClient.transaction bridge", () => {
  test("invokes the callback immediately instead of returning a wrapper", () => {
    const raw = {
      exec: jest.fn(),
      prepare: jest.fn(),
      transaction: jest.fn((fn: () => void) => fn),
    };

    const client = toDatabaseClient(raw as any);
    const callback = jest.fn();

    const result = client.transaction(callback);

    // The raw driver would have returned a function here; the contract requires
    // an immediate invocation returning nothing.
    expect(callback).toHaveBeenCalledTimes(1);
    expect(result).toBeUndefined();
  });
});

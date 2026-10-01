/**
 * Deterministic failure-boundary coverage for `main` in backend/seed.ts (issue #2630).
 *
 * Each describe block maps to one boundary named in the issue:
 *   success      -> a clean, fully-applied run
 *   permission   -> NODE_ENV=production must be refused without touching data
 *   validation   -> valid / invalid / duplicate / boundary-case datasets
 *   atomicity    -> partial failure rolls back instead of leaving partial state
 *   retry        -> transient faults recover, deterministic faults do not
 *   concurrency  -> a second overlapping run is rejected, not interleaved
 *   stale        -> a lease from a crashed run expires and is reclaimed
 *   observability-> structured logs/errors, no secrets
 *   regression   -> previously-observed behaviours stay intact
 */

import {
  main,
  seedScript,
  InMemorySeedStore,
  SeedError,
  isSeedError,
  isTransientError,
  validateDataset,
  buildDefaultDataset,
  createDeterministicIdFactory,
  acquireLease,
  releaseLease,
  resetLeases,
  runCli,
  withRetry,
  SEED_EPOCH,
  DEFAULT_MAX_ATTEMPTS,
  DEFAULT_LEASE_MS,
} from "../seed";
import type { SeedDataset, SeedLogger, SeedStore, User, Invoice, Bid } from "../seed";

/* -------------------------------------------------------------------------- */
/* Helpers                                                                    */
/* -------------------------------------------------------------------------- */

/** Collects structured log output so assertions can inspect diagnostics. */
function recordingLogger(): SeedLogger & { entries: Array<{ level: string; message: string; fields?: Record<string, unknown> }> } {
  const entries: Array<{ level: string; message: string; fields?: Record<string, unknown> }> = [];
  return {
    entries,
    info: (message, fields) => entries.push({ level: "info", message, fields }),
    warn: (message, fields) => entries.push({ level: "warn", message, fields }),
    error: (message, fields) => entries.push({ level: "error", message, fields }),
  };
}

const noSleep = async (): Promise<void> => {};

/**
 * A store whose first `times` calls to `failOn` throw, after which it behaves
 * normally. Pass `Number.POSITIVE_INFINITY` for an always-failing store.
 */
class FaultyStore extends InMemorySeedStore {
  private calls = 0;

  constructor(
    private readonly failOn: keyof SeedStore,
    private readonly failure: () => unknown,
    private readonly times: number
  ) {
    super();
  }

  private async maybeFail(method: keyof SeedStore): Promise<void> {
    if (method !== this.failOn) return;
    if (this.calls >= this.times) return;
    this.calls += 1;
    throw this.failure();
  }

  /** Number of failure injections that fired. */
  failureCount(): number {
    return this.calls;
  }

  /** Populate the store directly, bypassing the configured fault. */
  async prime(state: { users: User[]; invoices: Invoice[]; bids: Bid[] }): Promise<void> {
    this.users = state.users.map((u) => ({ ...u }));
    this.invoices = state.invoices.map((i) => ({ ...i }));
    this.bids = state.bids.map((b) => ({ ...b }));
  }

  async deleteAllBids(): Promise<void> {
    await this.maybeFail("deleteAllBids");
    await super.deleteAllBids();
  }
  async deleteAllInvoices(): Promise<void> {
    await this.maybeFail("deleteAllInvoices");
    await super.deleteAllInvoices();
  }
  async deleteAllUsers(): Promise<void> {
    await this.maybeFail("deleteAllUsers");
    await super.deleteAllUsers();
  }
  async createUsers(users: User[]): Promise<void> {
    await this.maybeFail("createUsers");
    await super.createUsers(users);
  }
  async createInvoices(invoices: Invoice[]): Promise<void> {
    await this.maybeFail("createInvoices");
    await super.createInvoices(invoices);
  }
  async createBids(bids: Bid[]): Promise<void> {
    await this.maybeFail("createBids");
    await super.createBids(bids);
  }
}

/** Records the order in which the seed touches the store. */
class OrderRecordingStore extends InMemorySeedStore {
  constructor(private readonly order: string[]) {
    super();
  }
  async deleteAllBids(): Promise<void> {
    this.order.push("deleteAllBids");
    await super.deleteAllBids();
  }
  async deleteAllInvoices(): Promise<void> {
    this.order.push("deleteAllInvoices");
    await super.deleteAllInvoices();
  }
  async deleteAllUsers(): Promise<void> {
    this.order.push("deleteAllUsers");
    await super.deleteAllUsers();
  }
  async createUsers(users: User[]): Promise<void> {
    this.order.push("createUsers");
    await super.createUsers(users);
  }
  async createInvoices(invoices: Invoice[]): Promise<void> {
    this.order.push("createInvoices");
    await super.createInvoices(invoices);
  }
  async createBids(bids: Bid[]): Promise<void> {
    this.order.push("createBids");
    await super.createBids(bids);
  }
}

const transient = () => Object.assign(new Error("SQLITE_BUSY: database is locked"), { code: "SQLITE_BUSY" });
const permanent = () =>
  Object.assign(new Error("SQLITE_CONSTRAINT: UNIQUE constraint failed: users.email"), {
    code: "SQLITE_CONSTRAINT",
  });

function dataset(): SeedDataset {
  return buildDefaultDataset(SEED_EPOCH, createDeterministicIdFactory());
}

beforeEach(() => {
  resetLeases();
});

afterEach(() => {
  releaseLease();
});

/* -------------------------------------------------------------------------- */
/* Success                                                                    */
/* -------------------------------------------------------------------------- */

describe("main - success path", () => {
  it("applies the deterministic dataset and reports what it created", async () => {
    const store = new InMemorySeedStore();
    const logger = recordingLogger();

    const result = await main({
      store,
      logger,
      env: { NODE_ENV: "development" },
      sleep: noSleep,
    });

    expect(result.status).toBe("completed");
    expect(result.created).toEqual({ users: 2, invoices: 1, bids: 1 });
    expect(result.attempts).toBe(1);
    expect(result.completedAt).toBe(SEED_EPOCH);

    const state = store.snapshot();
    expect(state.users).toHaveLength(2);
    expect(state.invoices).toHaveLength(1);
    expect(state.bids).toHaveLength(1);
    expect(store.commitCount).toBe(1);
    expect(store.rollbackCount).toBe(0);
    expect(logger.entries.length).toBeGreaterThan(0);
  });

  it("is deterministic: two runs produce byte-identical rows", async () => {
    const first = new InMemorySeedStore();
    const second = new InMemorySeedStore();
    const base = { env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() };

    await main({ ...base, store: first });
    await main({ ...base, store: second });

    expect(JSON.stringify(first.snapshot())).toBe(JSON.stringify(second.snapshot()));
  });

  it("is idempotent: re-running does not duplicate rows", async () => {
    const store = new InMemorySeedStore();
    const base = { env: { NODE_ENV: "development" }, sleep: noSleep, logger: recordingLogger() };

    await main({ ...base, store });
    await main({ ...base, store });
    await main({ ...base, store });

    const state = store.snapshot();
    expect(state.users).toHaveLength(2);
    expect(state.invoices).toHaveLength(1);
    expect(state.bids).toHaveLength(1);
  });

  it("clears in child-first order so no dangling references survive", async () => {
    const order: string[] = [];
    const store = new OrderRecordingStore(order);

    await main({ store, env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() });

    expect(order).toEqual([
      "deleteAllBids",
      "deleteAllInvoices",
      "deleteAllUsers",
      "createUsers",
      "createInvoices",
      "createBids",
    ]);
  });

  it("keeps the legacy seedScript export working", async () => {
    expect(seedScript).toBe(main);
  });

  it("runCli returns exit code 0 on success", async () => {
    const logSpy = jest.spyOn(console, "log").mockImplementation(() => undefined);
    try {
      const code = await runCli();
      expect(code).toBe(0);
      expect(logSpy).toHaveBeenCalled();
    } finally {
      logSpy.mockRestore();
    }
  });
});

/* -------------------------------------------------------------------------- */
/* Permission boundary                                                        */
/* -------------------------------------------------------------------------- */

describe("main - permission boundary", () => {
  it("refuses to run in production and reports PRODUCTION_FORBIDDEN", async () => {
    const store = new InMemorySeedStore();
    await expect(
      main({ store, env: { NODE_ENV: "production" }, sleep: noSleep, logger: recordingLogger() })
    ).rejects.toMatchObject({ code: "PRODUCTION_FORBIDDEN" });
  });

  it("performs no writes when the permission guard trips", async () => {
    const store = new InMemorySeedStore();
    await expect(
      main({ store, env: { NODE_ENV: "production" }, sleep: noSleep, logger: recordingLogger() })
    ).rejects.toBeInstanceOf(SeedError);

    expect(store.snapshot()).toEqual({ users: [], invoices: [], bids: [] });
    expect(store.commitCount).toBe(0);
  });

  it("does not call process.exit, leaving process lifetime to the caller", async () => {
    const exitSpy = jest.spyOn(process, "exit").mockImplementation((() => undefined) as never);
    try {
      await expect(
        main({ env: { NODE_ENV: "production" }, sleep: noSleep, logger: recordingLogger() })
      ).rejects.toBeInstanceOf(SeedError);
      expect(exitSpy).not.toHaveBeenCalled();
    } finally {
      exitSpy.mockRestore();
    }
  });

  it("allows non-production environments including an unset NODE_ENV", async () => {
    for (const nodeEnv of ["development", "test", undefined]) {
      const store = new InMemorySeedStore();
      const result = await main({ store, env: { NODE_ENV: nodeEnv }, sleep: noSleep, logger: recordingLogger() });
      expect(result.status).toBe("completed");
    }
  });

  it("never retries a permission rejection", async () => {
    const logger = recordingLogger();
    await expect(
      main({ env: { NODE_ENV: "production" }, sleep: noSleep, logger })
    ).rejects.toBeInstanceOf(SeedError);
    expect(logger.entries.filter((e) => e.level === "warn")).toHaveLength(0);
  });
});

/* -------------------------------------------------------------------------- */
/* Validation boundary                                                        */
/* -------------------------------------------------------------------------- */

describe("main - validation boundary", () => {
  it("rejects an empty dataset before any mutation", async () => {
    const store = new InMemorySeedStore();
    await expect(
      main({
        store,
        dataset: { users: [], invoices: [], bids: [] },
        env: { NODE_ENV: "test" },
        sleep: noSleep,
        logger: recordingLogger(),
      })
    ).rejects.toMatchObject({ code: "VALIDATION_FAILED" });

    expect(store.commitCount).toBe(0);
    expect(store.snapshot()).toEqual({ users: [], invoices: [], bids: [] });
  });

  it("rejects a bid that references an invoice outside the dataset", async () => {
    const bad = dataset();
    bad.bids[0].invoiceId = "seed-invoice-9999";

    await expect(
      main({ store: new InMemorySeedStore(), dataset: bad, env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() })
    ).rejects.toMatchObject({ code: "VALIDATION_FAILED" });
  });

  it("rejects a bid whose investor is not part of the dataset", async () => {
    const bad = dataset();
    bad.bids[0].investorId = "seed-user-9999";

    await expect(
      main({ store: new InMemorySeedStore(), dataset: bad, env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() })
    ).rejects.toMatchObject({ code: "VALIDATION_FAILED" });
  });

  it("enforces the expectedReturn >= bidAmount state invariant", async () => {
    const bad = dataset();
    bad.bids[0].expectedReturn = bad.bids[0].bidAmount - 1;

    await expect(
      main({ store: new InMemorySeedStore(), dataset: bad, env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() })
    ).rejects.toMatchObject({ code: "VALIDATION_FAILED" });
  });

  it("accepts expectedReturn exactly equal to bidAmount (boundary)", async () => {
    const edge = dataset();
    edge.bids[0].expectedReturn = edge.bids[0].bidAmount;

    const result = await main({
      store: new InMemorySeedStore(),
      dataset: edge,
      env: { NODE_ENV: "test" },
      sleep: noSleep,
      logger: recordingLogger(),
    });
    expect(result.status).toBe("completed");
  });

  it("rejects duplicate user emails within a single run", async () => {
    const dup = dataset();
    dup.users[1].email = dup.users[0].email;

    const err = await main({
      store: new InMemorySeedStore(),
      dataset: dup,
      env: { NODE_ENV: "test" },
      sleep: noSleep,
      logger: recordingLogger(),
    }).catch((e) => e);

    expect(isSeedError(err)).toBe(true);
    expect((err as SeedError).reasons?.join(" ")).toMatch(/duplicates an earlier user email/);
  });

  it("rejects duplicate primary keys within a single run", async () => {
    const dup = dataset();
    dup.users[1].id = dup.users[0].id;

    const err = await main({
      store: new InMemorySeedStore(),
      dataset: dup,
      env: { NODE_ENV: "test" },
      sleep: noSleep,
      logger: recordingLogger(),
    }).catch((e) => e);

    expect((err as SeedError).reasons?.join(" ")).toMatch(/duplicates an earlier user id/);
  });

  it("rejects an invoice owned by a user that is not seeded", async () => {
    const orphan = dataset();
    orphan.invoices[0].ownerId = "seed-user-9999";

    await expect(
      main({ store: new InMemorySeedStore(), dataset: orphan, env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() })
    ).rejects.toMatchObject({ code: "VALIDATION_FAILED" });
  });

  it("rejects non-positive and non-finite amounts", async () => {
    for (const amount of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
      const bad = dataset();
      bad.invoices[0].amount = amount;
      await expect(
        main({ store: new InMemorySeedStore(), dataset: bad, env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() })
      ).rejects.toMatchObject({ code: "VALIDATION_FAILED" });
    }
  });

  it("rejects invalid Date fields", async () => {
    const badDue = dataset();
    badDue.invoices[0].dueDate = new Date("not-a-date");
    await expect(
      main({ store: new InMemorySeedStore(), dataset: badDue, env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() })
    ).rejects.toMatchObject({ code: "VALIDATION_FAILED" });

    const badCreated = dataset();
    badCreated.bids[0].createdAt = new Date("nope");
    await expect(
      main({ store: new InMemorySeedStore(), dataset: badCreated, env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() })
    ).rejects.toMatchObject({ code: "VALIDATION_FAILED" });
  });

  it("rejects an unknown role", async () => {
    const bad = dataset();
    (bad.users[0] as { role: string }).role = "ADMIN";

    await expect(
      main({ store: new InMemorySeedStore(), dataset: bad, env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() })
    ).rejects.toMatchObject({ code: "VALIDATION_FAILED" });
  });

  it("reports every problem at once instead of one per run", () => {
    const reasons = validateDataset({
      users: [],
      invoices: [],
      bids: [],
    });
    expect(reasons.length).toBeGreaterThanOrEqual(3);
  });

  it("survives a structurally invalid dataset object", () => {
    expect(validateDataset({} as SeedDataset)).toEqual([
      "dataset must provide users, invoices and bids arrays",
    ]);
  });

  it("accepts a large dataset without dropping rows", async () => {
    const many = buildDefaultDataset(SEED_EPOCH, createDeterministicIdFactory("bulk"));
    many.invoices = Array.from({ length: 50 }, (_, i) => ({
      ...many.invoices[0],
      id: `bulk-invoice-${String(i).padStart(4, "0")}`,
    }));
    many.bids = many.invoices.map((invoice, i) => ({
      ...many.bids[0],
      id: `bulk-bid-${String(i).padStart(4, "0")}`,
      invoiceId: invoice.id,
    }));

    const store = new InMemorySeedStore();
    const result = await main({ store, dataset: many, env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() });

    expect(result.created).toEqual({ users: 2, invoices: 50, bids: 50 });
    expect(store.snapshot().bids).toHaveLength(50);
  });
});

/* -------------------------------------------------------------------------- */
/* Atomicity / partial failure                                                */
/* -------------------------------------------------------------------------- */

describe("main - atomicity and partial failure", () => {
  it("rolls back and leaves no partial state when a late write fails", async () => {
    // times = 0 -> the final create step always fails permanently.
    const store = new FaultyStore("createBids", permanent, Number.POSITIVE_INFINITY);

    await expect(
      main({ store, env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() })
    ).rejects.toBeInstanceOf(SeedError);

    expect(store.rollbackCount).toBeGreaterThan(0);
    expect(store.commitCount).toBe(0);
    // Nothing partially applied: the store is back to empty, not half-seeded.
    expect(store.snapshot()).toEqual({ users: [], invoices: [], bids: [] });
  });

  it("restores pre-existing rows when a run against a populated store fails", async () => {
    const d = dataset();
    // A populated store whose final write then fails permanently.
    const store = new FaultyStore("createBids", permanent, Number.POSITIVE_INFINITY);
    await store.prime({ users: d.users, invoices: d.invoices, bids: d.bids });
    const populated = JSON.stringify(store.snapshot());

    await expect(
      main({ store, env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() })
    ).rejects.toBeInstanceOf(SeedError);

    // The rows the failed run deleted were restored, not lost.
    expect(JSON.stringify(store.snapshot())).toBe(populated);
  });

  it("never leaves a bid without its invoice after a rollback", async () => {
    const store = new FaultyStore("createInvoices", permanent, Number.POSITIVE_INFINITY);
    await expect(
      main({ store, env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() })
    ).rejects.toBeInstanceOf(SeedError);

    const state = store.snapshot();
    expect(state.bids).toHaveLength(0);
    expect(state.invoices).toHaveLength(0);
  });

  it("wraps an unexpected error as SEED_FAILED with the commit phase", async () => {
    const store = new InMemorySeedStore();
    store.createUsers = async () => {
      throw new Error("boom");
    };

    const err = await main({
      store,
      env: { NODE_ENV: "test" },
      maxAttempts: 1,
      sleep: noSleep,
      logger: recordingLogger(),
    }).catch((e) => e);

    expect(isSeedError(err)).toBe(true);
    expect((err as SeedError).code).toBe("SEED_FAILED");
    expect((err as SeedError).phase).toBe("committing");
  });

  it("preserves the original error as cause for diagnosis", async () => {
    const store = new InMemorySeedStore();
    const original = new Error("disk exploded");
    store.createUsers = async () => {
      throw original;
    };

    const err = await main({
      store,
      env: { NODE_ENV: "test" },
      maxAttempts: 1,
      sleep: noSleep,
      logger: recordingLogger(),
    }).catch((e) => e as SeedError);

    expect(err.cause).toBe(original);
  });

  it("releases the writer lease after a failure so a retry can proceed", async () => {
    const store = new InMemorySeedStore();
    store.createBids = async () => {
      throw new Error("nope");
    };

    await expect(
      main({ store, env: { NODE_ENV: "test" }, maxAttempts: 1, sleep: noSleep, logger: recordingLogger() })
    ).rejects.toBeInstanceOf(SeedError);

    // A fresh, healthy run must not be blocked by the previous failure.
    const healthy = new InMemorySeedStore();
    await expect(
      main({ store: healthy, env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() })
    ).resolves.toMatchObject({ status: "completed" });
  });
});

/* -------------------------------------------------------------------------- */
/* Retry boundary                                                             */
/* -------------------------------------------------------------------------- */

describe("main - retry boundary", () => {
  it("recovers from a transient failure and reports the attempt count", async () => {
    const store = new FaultyStore("createBids", transient, 1);
    const logger = recordingLogger();

    const result = await main({
      store,
      env: { NODE_ENV: "test" },
      maxAttempts: 3,
      retryBaseMs: 0,
      sleep: noSleep,
      logger,
    });

    expect(result.status).toBe("completed");
    expect(result.attempts).toBe(2);
    expect(store.failureCount()).toBe(1);
    expect(logger.entries.some((e) => e.level === "warn" && /retrying/.test(e.message))).toBe(true);
  });

  it("gives up with RETRY_EXHAUSTED after exhausting the attempt budget", async () => {
    const store = new FaultyStore("createUsers", transient, Number.POSITIVE_INFINITY);
    const logger = recordingLogger();

    const err = await main({
      store,
      env: { NODE_ENV: "test" },
      maxAttempts: 3,
      retryBaseMs: 0,
      sleep: noSleep,
      logger,
    }).catch((e) => e as SeedError);

    expect(err.code).toBe("RETRY_EXHAUSTED");
    expect(store.failureCount()).toBe(3);
  });

  it("does not retry a permanent failure", async () => {
    const store = new FaultyStore("createUsers", permanent, Number.POSITIVE_INFINITY);
    const logger = recordingLogger();

    await expect(
      main({ store, env: { NODE_ENV: "test" }, maxAttempts: 5, retryBaseMs: 0, sleep: noSleep, logger })
    ).rejects.toBeInstanceOf(SeedError);

    // Exactly one attempt: a unique-constraint violation cannot heal itself.
    expect(store.failureCount()).toBe(1);
    expect(logger.entries.filter((e) => e.level === "warn")).toHaveLength(0);
  });

  it("uses deterministic exponential backoff", async () => {
    const store = new FaultyStore("createUsers", transient, Number.POSITIVE_INFINITY);
    const delays: number[] = [];

    await main({
      store,
      env: { NODE_ENV: "test" },
      maxAttempts: 4,
      retryBaseMs: 10,
      sleep: async (ms) => {
        delays.push(ms);
      },
      logger: recordingLogger(),
    }).catch(() => undefined);

    expect(delays).toEqual([10, 20, 40]);
  });

  it("honours maxAttempts=1 (no retry at all)", async () => {
    const store = new FaultyStore("createUsers", transient, Number.POSITIVE_INFINITY);

    await expect(
      main({ store, env: { NODE_ENV: "test" }, maxAttempts: 1, retryBaseMs: 0, sleep: noSleep, logger: recordingLogger() })
    ).rejects.toMatchObject({ code: "RETRY_EXHAUSTED" });

    expect(store.failureCount()).toBe(1);
  });

  it("defaults to a bounded attempt budget", () => {
    expect(DEFAULT_MAX_ATTEMPTS).toBe(3);
  });

  it("classifies transient and permanent datastore errors", () => {
    expect(isTransientError(transient())).toBe(true);
    expect(isTransientError(permanent())).toBe(false);
    expect(isTransientError(new Error("database is locked"))).toBe(true);
    expect(isTransientError(new Error("SQLITE_CORRUPT: file is not a database"))).toBe(false);
    expect(isTransientError(new Error("unrelated"))).toBe(false);
    expect(isTransientError(undefined)).toBe(false);
  });

  it("withRetry returns the attempt count alongside the value", async () => {
    const logger = recordingLogger();
    let calls = 0;
    const out = await withRetry(
      async () => {
        calls += 1;
        if (calls < 2) throw transient();
        return "ok";
      },
      { maxAttempts: 3, retryBaseMs: 0, sleep: noSleep, logger, runId: "run-test" }
    );

    expect(out).toEqual({ value: "ok", attempts: 2 });
  });

  it("a retry does not duplicate rows already written by the failed attempt", async () => {
    // The transaction is re-entered from scratch on retry, so the clearing step
    // runs again. This is what keeps a retried seed from doubling up.
    const store = new FaultyStore("createBids", transient, 1);
    const result = await main({
      store,
      env: { NODE_ENV: "test" },
      maxAttempts: 3,
      retryBaseMs: 0,
      sleep: noSleep,
      logger: recordingLogger(),
    });

    const state = store.snapshot();
    expect(state.users).toHaveLength(result.created.users);
    expect(state.invoices).toHaveLength(result.created.invoices);
    expect(state.bids).toHaveLength(result.created.bids);
  });
});

/* -------------------------------------------------------------------------- */
/* Concurrency boundary                                                       */
/* -------------------------------------------------------------------------- */

describe("main - concurrency boundary", () => {
  it("rejects a second overlapping run with CONCURRENT_RUN", async () => {
    const store = new InMemorySeedStore();
    let releaseFirst: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });

    const slowStore = new Proxy(store, {
      get(target, prop, receiver) {
        if (prop === "transaction") {
          return async (fn: () => Promise<unknown>) => {
            await gate;
            return target.transaction(fn as never);
          };
        }
        const value = Reflect.get(target, prop, receiver);
        return typeof value === "function" ? value.bind(target) : value;
      },
    });

    const first = main({
      store: slowStore as unknown as SeedStore,
      env: { NODE_ENV: "test" },
      sleep: noSleep,
      logger: recordingLogger(),
    });

    // Let the first run take the lease before the second one starts.
    await new Promise((r) => setImmediate(r));
    const second = main({
      store: new InMemorySeedStore(),
      env: { NODE_ENV: "test" },
      sleep: noSleep,
      logger: recordingLogger(),
    });

    releaseFirst();
    await expect(first).resolves.toMatchObject({ status: "completed" });
    await expect(second).rejects.toMatchObject({ code: "CONCURRENT_RUN" });
  });

  it("releasing the lease allows the next sequential run to proceed", async () => {
    await main({ store: new InMemorySeedStore(), env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() });
    await expect(
      main({ store: new InMemorySeedStore(), env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() })
    ).resolves.toMatchObject({ status: "completed" });
  });

  it("runs fully in parallel without interleaving or corruption", async () => {
    // Each run gets its own store; the shared lease serialises them, so all
    // must either complete or be rejected - never partially applied.
    const results = await Promise.allSettled(
      Array.from({ length: 5 }, () =>
        main({ store: new InMemorySeedStore(), env: { NODE_ENV: "test" }, sleep: noSleep, logger: recordingLogger() })
      )
    );

    const codes = results.map((r) =>
      r.status === "fulfilled" ? "completed" : (r.reason as SeedError).code
    );
    // Every outcome is one of the two defined terminal states.
    for (const code of codes) {
      expect(["completed", "CONCURRENT_RUN"]).toContain(code);
    }
    expect(codes.filter((c) => c === "completed").length).toBeGreaterThan(0);
  });
});

/* -------------------------------------------------------------------------- */
/* Stale boundary                                                             */
/* -------------------------------------------------------------------------- */

describe("main - stale lease boundary", () => {
  it("reclaims a lease older than the lease window", async () => {
    const t0 = 1_000_000;
    const first = acquireLease(t0, DEFAULT_LEASE_MS);
    expect(first.reclaimedStale).toBe(false);

    // A crashed run leaves the lease behind; the next run at a later clock
    // reading is allowed to reclaim it.
    const second = acquireLease(t0 + DEFAULT_LEASE_MS + 1, DEFAULT_LEASE_MS);
    expect(second.reclaimedStale).toBe(true);
    expect(second.runId).not.toBe(first.runId);
  });

  it("refuses to reclaim a lease that is still fresh", () => {
    const t0 = 1_000_000;
    acquireLease(t0, DEFAULT_LEASE_MS);
    expect(() => acquireLease(t0 + DEFAULT_LEASE_MS - 1, DEFAULT_LEASE_MS)).toThrow(SeedError);
  });

  it("treats the lease boundary as exclusive: age === leaseMs reclaims", () => {
    const t0 = 1_000_000;
    acquireLease(t0, 100);
    expect(() => acquireLease(t0 + 100, 100)).not.toThrow();
  });

  it("a run after a reclaimed stale lease completes and flags reclamation", async () => {
    const t0 = 1_000_000;
    const logger = recordingLogger();
    acquireLease(t0, 10);

    // now is far beyond the previous lease, so it is reclaimed.
    const result = await main({
      store: new InMemorySeedStore(),
      env: { NODE_ENV: "test" },
      now: t0 + 10_000,
      leaseMs: 10,
      sleep: noSleep,
      logger,
    });

    expect(result.reclaimedStaleLease).toBe(true);
    expect(logger.entries.some((e) => e.level === "warn" && /stale/.test(e.message))).toBe(true);
  });

  it("a fresh run does not report stale reclamation", async () => {
    const result = await main({
      store: new InMemorySeedStore(),
      env: { NODE_ENV: "test" },
      now: SEED_EPOCH,
      sleep: noSleep,
      logger: recordingLogger(),
    });
    expect(result.reclaimedStaleLease).toBe(false);
  });
});

/* -------------------------------------------------------------------------- */
/* Observability                                                              */
/* -------------------------------------------------------------------------- */

describe("main - observability", () => {
  it("records a phase-tagged entry for every stage of the run", async () => {
    const logger = recordingLogger();
    await main({ store: new InMemorySeedStore(), env: { NODE_ENV: "test" }, sleep: noSleep, logger });

    const phases = new Set(
      logger.entries.map((e) => e.fields?.phase).filter((p): p is string => typeof p === "string")
    );
    for (const phase of ["clearing", "creating_users", "creating_invoices", "creating_bids", "committing"]) {
      expect(phases).toContain(phase);
    }
  });

  it("every failure carries a stable code and a phase", async () => {
    const cases: Array<[Promise<unknown>, string]> = [
      [main({ env: { NODE_ENV: "production" }, sleep: noSleep, logger: recordingLogger() }), "PRODUCTION_FORBIDDEN"],
      [
        main({
          store: new InMemorySeedStore(),
          dataset: { users: [], invoices: [], bids: [] },
          env: { NODE_ENV: "test" },
          sleep: noSleep,
          logger: recordingLogger(),
        }),
        "VALIDATION_FAILED",
      ],
    ];

    for (const [promise, code] of cases) {
      const err = (await promise.catch((e) => e)) as SeedError;
      expect(isSeedError(err)).toBe(true);
      expect(err.code).toBe(code);
      expect(err.phase).toBeTruthy();
    }
  });

  it("does not leak secrets from a datastore error into the SeedError message", async () => {
    const store = new InMemorySeedStore();
    store.createUsers = async () => {
      throw new Error("connect ECONNREFUSED postgres://admin:hunter2@10.0.0.5:5432/quicklendx");
    };

    const err = (await main({
      store,
      env: { NODE_ENV: "test" },
      maxAttempts: 1,
      sleep: noSleep,
      logger: recordingLogger(),
    }).catch((e) => e)) as SeedError;

    expect(err.message).not.toMatch(/hunter2/);
    expect(err.message).not.toMatch(/postgres:\/\//);
    expect(err.message).toMatch(/SEED_FAILED|rolled back/);
  });

  it("keeps the underlying error as a non-enumerated cause for operators", async () => {
    const store = new InMemorySeedStore();
    const original = new Error("token=abc123 rejected");
    store.createBids = async () => {
      throw original;
    };

    const err = (await main({
      store,
      env: { NODE_ENV: "test" },
      maxAttempts: 1,
      sleep: noSleep,
      logger: recordingLogger(),
    }).catch((e) => e)) as SeedError;

    // The raw message is available on `cause` for a human, but the SeedError's
    // own message stays generic.
    expect((err.cause as Error).message).toBe("token=abc123 rejected");
    expect(err.message).not.toMatch(/abc123/);
  });

  it("runCli returns 1 and prints the code on failure", async () => {
    const errSpy = jest.spyOn(console, "error").mockImplementation(() => undefined);
    const logSpy = jest.spyOn(console, "log").mockImplementation(() => undefined);
    const originalEnv = process.env.NODE_ENV;
    try {
      process.env.NODE_ENV = "production";
      const code = await runCli();
      expect(code).toBe(1);
      const printed = errSpy.mock.calls.map((c) => String(c[0])).join("\n");
      expect(printed).toMatch(/PRODUCTION_FORBIDDEN/);
      expect(printed).not.toMatch(/hunter2/);
    } finally {
      process.env.NODE_ENV = originalEnv;
      errSpy.mockRestore();
      logSpy.mockRestore();
    }
  });

  it("runCli prints the created summary line on success", async () => {
    const logSpy = jest.spyOn(console, "log").mockImplementation(() => undefined);
    const originalEnv = process.env.NODE_ENV;
    try {
      process.env.NODE_ENV = "test";
      expect(await runCli()).toBe(0);
      const printed = logSpy.mock.calls.map((c) => String(c[0])).join("\n");
      expect(printed).toMatch(/2 Users, 1 Invoice, 1 Bid/);
    } finally {
      process.env.NODE_ENV = originalEnv;
      logSpy.mockRestore();
    }
  });
});

/* -------------------------------------------------------------------------- */
/* Regression                                                                 */
/* -------------------------------------------------------------------------- */

describe("main - regression coverage", () => {
  it("still exports the same public entry point as before the change", () => {
    expect(typeof main).toBe("function");
    expect(typeof seedScript).toBe("function");
    expect(main.length).toBe(0); // all parameters are optional
  });

  it("keeps the documented default dataset shape (2 users, 1 invoice, 1 bid)", () => {
    const d = dataset();
    expect(d.users).toHaveLength(2);
    expect(d.invoices).toHaveLength(1);
    expect(d.bids).toHaveLength(1);
    expect(d.users[0].role).toBe("BUSINESS");
    expect(d.users[1].role).toBe("INVESTOR");
    expect(d.invoices[0].currency).toBe("USDC");
    expect(d.invoices[0].status).toBe("VERIFIED");
    expect(d.bids[0].status).toBe("PLACED");
  });

  it("preserves the original sample identities and amounts", () => {
    const d = dataset();
    expect(d.users.map((u) => u.email).sort()).toEqual([
      "business@example.com",
      "investor@example.com",
    ]);
    expect(d.users[0].name).toBe("Acme Services Corp");
    expect(d.users[0].taxId).toBe("TX-998877");
    expect(d.users[1].name).toBe("Stellar Capital");
    expect(d.invoices[0].amount).toBe(500_000);
    expect(d.invoices[0].description).toBe("Q3 Software Consulting Services");
    expect(d.invoices[0].category).toBe("SERVICES");
    expect(d.bids[0].bidAmount).toBe(485_000);
    expect(d.bids[0].expectedReturn).toBe(515_000);
  });

  it("anchors dueDate 30 days after the injected clock", () => {
    const d = buildDefaultDataset(1_000_000_000_000, createDeterministicIdFactory());
    const delta = (d.invoices[0].dueDate.getTime() - 1_000_000_000_000) / (24 * 60 * 60 * 1000);
    expect(delta).toBe(30);
  });

  it("ids are stable for a given prefix and index", () => {
    const f = createDeterministicIdFactory("abc");
    expect(f("user", 0)).toBe("abc-user-0001");
    expect(f("bid", 41)).toBe("abc-bid-0042");
  });

  it("a no-argument main() call still succeeds (CLI compatibility)", async () => {
    // The truly zero-argument path: defaults for store/env/clock/ids/logger.
    const logSpy = jest.spyOn(console, "log").mockImplementation(() => undefined);
    const originalEnv = process.env.NODE_ENV;
    try {
      process.env.NODE_ENV = "test";
      const result = await main();
      expect(result.status).toBe("completed");
    } finally {
      process.env.NODE_ENV = originalEnv;
      logSpy.mockRestore();
    }
  });

  it("every terminal state is a SeedError, never a raw driver error", async () => {
    const attempts: Array<Promise<unknown>> = [
      main({ env: { NODE_ENV: "production" }, sleep: noSleep, logger: recordingLogger() }),
      main({
        store: new InMemorySeedStore(),
        dataset: { users: [], invoices: [], bids: [] },
        env: { NODE_ENV: "test" },
        sleep: noSleep,
        logger: recordingLogger(),
      }),
      main({
        store: new FaultyStore("createUsers", permanent, Number.POSITIVE_INFINITY),
        env: { NODE_ENV: "test" },
        maxAttempts: 2,
        retryBaseMs: 0,
        sleep: noSleep,
        logger: recordingLogger(),
      }),
    ];

    for (const p of attempts) {
      const err = (await p.catch((e) => e)) as SeedError;
      expect(isSeedError(err)).toBe(true);
      expect(err).toBeInstanceOf(Error);
      expect(typeof err.stack).toBe("string");
    }
  });
});

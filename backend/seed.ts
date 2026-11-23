/**
 * QuickLendX Protocol - Local Development Seed Script
 * Populates the datastore with sample data for local development.
 *
 * DESIGN INVARIANTS (see "Failure boundaries" below)
 * --------------------------------------------------
 * 1. DESTRUCTIVE BY DEFAULT, PRODUCTION-PROHIBITED.
 *    The seed truncates the seeded tables. It refuses to run when
 *    NODE_ENV === "production" and raises a typed SeedError instead of
 *    calling process.exit(), so callers (CLI and tests) stay in control of
 *    process lifetime.
 *
 * 2. DETERMINISTIC.
 *    Identifiers and timestamps are derived from a fixed epoch and a
 *    deterministic generator, never from Date.now() or a random source.
 *    Two runs against the same injected clock/id factory produce byte-identical
 *    rows. Callers may inject their own clock/id factory to opt out.
 *
 * 3. ATOMIC.
 *    Clearing and re-creating happen inside a single transaction. Any failure
 *    rolls the whole run back, so a partial seed can never be observed and
 *    pre-existing rows are restored rather than lost.
 *
 * 4. VALIDATED BEFORE MUTATION.
 *    The dataset is fully validated before the first write. A malformed or
 *    duplicated dataset is rejected without touching the datastore.
 *
 * 5. BOUNDED, DETERMINISTIC RETRY.
 *    Only transient failures are retried, with a fixed attempt budget and an
 *    injectable sleep. Retry exhaustion is a distinct, diagnosable error.
 *
 * 6. SINGLE WRITER.
 *    A run holds an in-process lease. A concurrent run is rejected rather than
 *    interleaved. A lease left behind by a crashed process expires and is
 *    reclaimed, so a stale lock cannot wedge seeding forever.
 *
 * 7. DIAGNOSABLE, NOT LEAKY.
 *    Failures carry a stable `code`, the `phase` they occurred in, and
 *    row-level context. No secrets or connection strings are ever emitted.
 *
 * Failure boundaries
 * ------------------
 *   loading    -> phases: validating | clearing | creating_users |
 *                 creating_invoices | creating_bids | committing
 *   error      -> SeedError.code, one of the codes enumerated in SeedErrorCode
 *   retry      -> transient datastore errors retried up to `maxAttempts`
 *   stale      -> a lease older than `leaseMs` is reclaimed by the next run
 *   permission -> production guard above
 * No boundary can leave the datastore in a partially seeded state, because
 * every write is performed inside one transaction (invariant 3).
 */

export const SEED_EPOCH = Date.UTC(2026, 0, 1, 0, 0, 0);
const DAY_MS = 24 * 60 * 60 * 1000;

export const DEFAULT_MAX_ATTEMPTS = 3;
export const DEFAULT_LEASE_MS = 30_000;
export const DEFAULT_RETRY_BASE_MS = 10;

/* -------------------------------------------------------------------------- */
/* Domain types                                                               */
/* -------------------------------------------------------------------------- */

export type UserRole = "BUSINESS" | "INVESTOR";

export interface User {
  id: string;
  email: string;
  name: string;
  role: UserRole;
  isVerified: boolean;
  taxId?: string;
}

export type InvoiceStatus =
  | "PENDING"
  | "VERIFIED"
  | "FUNDED"
  | "PAID"
  | "CANCELLED";

export interface Invoice {
  id: string;
  ownerId: string;
  amount: number;
  currency: string;
  status: InvoiceStatus;
  description: string;
  dueDate: Date;
  category: "SERVICES" | "GOODS";
}

export type BidStatus = "PLACED" | "ACCEPTED" | "REJECTED" | "CANCELLED";

export interface Bid {
  id: string;
  investorId: string;
  invoiceId: string;
  bidAmount: number;
  expectedReturn: number;
  status: BidStatus;
  createdAt: Date;
}

export interface SeedDataset {
  users: User[];
  invoices: Invoice[];
  bids: Bid[];
}

/* -------------------------------------------------------------------------- */
/* Datastore contract                                                        */
/* -------------------------------------------------------------------------- */

/**
 * The narrow surface the seed script needs. Keeping it this small lets tests
 * supply a failing or slow implementation to exercise the failure boundaries.
 */
export interface SeedStore {
  /**
   * Run `fn` inside a transaction. If `fn` throws, every write performed by
   * `fn` must be rolled back before the rejection propagates.
   */
  transaction<T>(fn: () => Promise<T> | T): Promise<T>;
  deleteAllBids(): Promise<void>;
  deleteAllInvoices(): Promise<void>;
  deleteAllUsers(): Promise<void>;
  createUsers(users: User[]): Promise<void>;
  createInvoices(invoices: Invoice[]): Promise<void>;
  createBids(bids: Bid[]): Promise<void>;
}

/* -------------------------------------------------------------------------- */
/* Errors                                                                     */
/* -------------------------------------------------------------------------- */

export type SeedErrorCode =
  | "PRODUCTION_FORBIDDEN"
  | "VALIDATION_FAILED"
  | "CONCURRENT_RUN"
  | "STALE_RUN_RECLAIMED"
  | "RETRY_EXHAUSTED"
  | "SEED_FAILED";

export type SeedPhase =
  | "validating"
  | "clearing"
  | "creating_users"
  | "creating_invoices"
  | "creating_bids"
  | "committing";

/**
 * Every failure surfaced by `main` is a SeedError, so callers can branch on a
 * stable `code` instead of matching message strings.
 *
 * `message` is safe to log: it is assembled from the code, the phase, and
 * validation reasons, never from datastore payloads or credentials.
 */
export class SeedError extends Error {
  readonly code: SeedErrorCode;
  readonly phase: SeedPhase | "guard";
  readonly reasons?: string[];
  readonly cause?: unknown;

  constructor(
    code: SeedErrorCode,
    message: string,
    options: { phase?: SeedPhase | "guard"; reasons?: string[]; cause?: unknown } = {}
  ) {
    super(message);
    this.name = "SeedError";
    this.code = code;
    this.phase = options.phase ?? "guard";
    this.reasons = options.reasons;
    this.cause = options.cause;
    // Preserve `instanceof` across the ES5/ES2020 target boundary.
    Object.setPrototypeOf(this, SeedError.prototype);
  }
}

export function isSeedError(err: unknown): err is SeedError {
  return err instanceof SeedError;
}

/** Transient failures are the only ones eligible for a retry. */
export function isTransientError(err: unknown): boolean {
  if (isSeedError(err)) {
    // A validation or permission rejection is deterministic: retrying it
    // cannot change the outcome, so it must never consume the retry budget.
    return err.code === "RETRY_EXHAUSTED" || err.code === "SEED_FAILED";
  }
  const code = (err as { code?: unknown } | null | undefined)?.code;
  if (typeof code === "string") {
    if (TRANSIENT_CODES.has(code)) return true;
    if (PERMANENT_CODES.has(code)) return false;
  }
  const message = (err as { message?: unknown } | null | undefined)?.message;
  if (typeof message === "string") {
    return TRANSIENT_MESSAGE_FRAGMENTS.some((fragment) => message.includes(fragment));
  }
  return false;
}

const TRANSIENT_CODES = new Set([
  "SQLITE_BUSY",
  "SQLITE_LOCKED",
  "ECONNRESET",
  "ETIMEDOUT",
  "EPIPE",
  "CONNECTION_CLOSED",
]);
const PERMANENT_CODES = new Set([
  "SQLITE_CONSTRAINT",
  "SQLITE_READONLY",
  "SQLITE_CORRUPT",
  "SQLITE_NOTADB",
  "SQLITE_PERM",
  "SQLITE_CANTOPEN",
]);
const TRANSIENT_MESSAGE_FRAGMENTS = [
  "database is locked",
  "database table is locked",
  "connection terminated",
  "connection closed",
  "timeout exceeded",
  "too many connections",
  "server closed the connection",
  "ECONNRESET",
  "ETIMEDOUT",
];

/* -------------------------------------------------------------------------- */
/* Deterministic default dataset                                              */
/* -------------------------------------------------------------------------- */

/**
 * Deterministic id factory: `prefix-index`, zero padded.
 *
 * Replaces the previous random `createId()` (cuid2) call so that repeated runs
 * produce identical primary keys, which is what makes the seed idempotent and
 * its tests reproducible.
 */
export function createDeterministicIdFactory(seed: string = "seed") {
  return function nextId(kind: string, index: number): string {
    return `${seed}-${kind}-${String(index + 1).padStart(4, "0")}`;
  };
}

/** The dataset written by the default `npm run seed` invocation. */
export function buildDefaultDataset(
  now: number = SEED_EPOCH,
  createId: (kind: string, index: number) => string = createDeterministicIdFactory()
): SeedDataset {
  const businessUser: User = {
    id: createId("user", 0),
    email: "business@example.com",
    name: "Acme Services Corp",
    role: "BUSINESS",
    isVerified: true,
    taxId: "TX-998877",
  };

  const investorUser: User = {
    id: createId("user", 1),
    email: "investor@example.com",
    name: "Stellar Capital",
    role: "INVESTOR",
    isVerified: true,
  };

  const invoice: Invoice = {
    id: createId("invoice", 0),
    ownerId: businessUser.id,
    amount: 500_000,
    currency: "USDC",
    status: "VERIFIED",
    description: "Q3 Software Consulting Services",
    dueDate: new Date(now + 30 * DAY_MS),
    category: "SERVICES",
  };

  const bid: Bid = {
    id: createId("bid", 0),
    investorId: investorUser.id,
    invoiceId: invoice.id,
    bidAmount: 485_000,
    expectedReturn: 515_000,
    status: "PLACED",
    createdAt: new Date(now),
  };

  return { users: [businessUser, investorUser], invoices: [invoice], bids: [bid] };
}

/* -------------------------------------------------------------------------- */
/* Validation                                                                 */
/* -------------------------------------------------------------------------- */

/**
 * Fully validates the dataset before any mutation happens.
 *
 * Returns every reason the dataset is unusable, so an operator sees all the
 * problems at once instead of fixing them one run at a time.
 */
export function validateDataset(dataset: SeedDataset): string[] {
  const reasons: string[] = [];
  const { users, invoices, bids } = dataset;

  if (!Array.isArray(users) || !Array.isArray(invoices) || !Array.isArray(bids)) {
    return ["dataset must provide users, invoices and bids arrays"];
  }

  if (users.length === 0) {
    reasons.push("dataset must contain at least one user");
  }
  if (invoices.length === 0) {
    reasons.push("dataset must contain at least one invoice");
  }
  if (bids.length === 0) {
    reasons.push("dataset must contain at least one bid");
  }

  const userIds = new Set<string>();
  const userEmails = new Set<string>();
  users.forEach((user, i) => {
    const where = `users[${i}]`;
    if (!user || typeof user.id !== "string" || user.id.length === 0) {
      reasons.push(`${where}.id must be a non-empty string`);
    } else if (userIds.has(user.id)) {
      reasons.push(`${where}.id duplicates an earlier user id`);
    } else {
      userIds.add(user.id);
    }

    if (!user || typeof user.email !== "string" || user.email.length === 0) {
      reasons.push(`${where}.email must be a non-empty string`);
    } else if (userEmails.has(user.email)) {
      // A duplicate email is the boundary case that turns a re-run into a
      // silent upsert-or-fail; reject it rather than let the store decide.
      reasons.push(`${where}.email duplicates an earlier user email`);
    } else {
      userEmails.add(user.email);
    }

    if (!user || (user.role !== "BUSINESS" && user.role !== "INVESTOR")) {
      reasons.push(`${where}.role must be BUSINESS or INVESTOR`);
    }
  });

  const invoiceIds = new Set<string>();
  invoices.forEach((invoice, i) => {
    const where = `invoices[${i}]`;
    if (!invoice || typeof invoice.id !== "string" || invoice.id.length === 0) {
      reasons.push(`${where}.id must be a non-empty string`);
    } else if (invoiceIds.has(invoice.id)) {
      reasons.push(`${where}.id duplicates an earlier invoice id`);
    } else {
      invoiceIds.add(invoice.id);
    }

    if (!invoice || !userIds.has(invoice.ownerId)) {
      reasons.push(`${where}.ownerId must reference a seeded user`);
    }
    if (!invoice || !Number.isFinite(invoice.amount) || invoice.amount <= 0) {
      reasons.push(`${where}.amount must be a positive finite number`);
    }
    if (!invoice || !(invoice.dueDate instanceof Date) || Number.isNaN(invoice.dueDate.getTime())) {
      reasons.push(`${where}.dueDate must be a valid Date`);
    }
  });

  const bidIds = new Set<string>();
  bids.forEach((bid, i) => {
    const where = `bids[${i}]`;
    if (!bid || typeof bid.id !== "string" || bid.id.length === 0) {
      reasons.push(`${where}.id must be a non-empty string`);
    } else if (bidIds.has(bid.id)) {
      reasons.push(`${where}.id duplicates an earlier bid id`);
    } else {
      bidIds.add(bid.id);
    }

    // Referential integrity: a bid may only target an invoice in this run.
    if (!bid || !invoiceIds.has(bid.invoiceId)) {
      reasons.push(`${where}.invoiceId must reference a seeded invoice`);
    }
    if (!bid || !userIds.has(bid.investorId)) {
      reasons.push(`${where}.investorId must reference a seeded user`);
    }
    if (!bid || !Number.isFinite(bid.bidAmount) || bid.bidAmount <= 0) {
      reasons.push(`${where}.bidAmount must be a positive finite number`);
    }
    // State-transition invariant, mirroring BidStore.createBid.
    if (
      !bid ||
      !Number.isFinite(bid.expectedReturn) ||
      !Number.isFinite(bid.bidAmount) ||
      bid.expectedReturn < bid.bidAmount
    ) {
      reasons.push(`${where}.expectedReturn must be greater than or equal to bidAmount`);
    }
    if (!bid || !(bid.createdAt instanceof Date) || Number.isNaN(bid.createdAt.getTime())) {
      reasons.push(`${where}.createdAt must be a valid Date`);
    }
  });

  return reasons;
}

/* -------------------------------------------------------------------------- */
/* In-memory store                                                            */
/* -------------------------------------------------------------------------- */

/**
 * Transactional in-memory SeedStore.
 *
 * This is the default adapter, replacing the previous no-op `MockPrismaClient`
 * whose `deleteMany`/`createMany` reported success without persisting anything.
 * Writes are staged and only applied on commit, so a throwing callback leaves
 * the store byte-identical to its pre-transaction state.
 */
export class InMemorySeedStore implements SeedStore {
  protected users: User[] = [];
  protected invoices: Invoice[] = [];
  protected bids: Bid[] = [];

  /** Number of transactions that reached their commit point. */
  commitCount = 0;
  /** Number of transactions that were rolled back. */
  rollbackCount = 0;
  /** True while a transaction is in progress. */
  inTransaction = false;

  async transaction<T>(fn: () => Promise<T> | T): Promise<T> {
    if (this.inTransaction) {
      // A programming error, not a transient fault: retrying cannot help.
      throw new SeedError("VALIDATION_FAILED", "nested transactions are not supported", {
        phase: "committing",
      });
    }
    const snapshot = {
      users: this.users.slice(),
      invoices: this.invoices.slice(),
      bids: this.bids.slice(),
    };
    this.inTransaction = true;
    try {
      const result = await fn();
      this.commitCount += 1;
      return result;
    } catch (err) {
      // Invariant 3: restore the exact pre-transaction state so a failed seed
      // never leaves the datastore partially written.
      this.users = snapshot.users;
      this.invoices = snapshot.invoices;
      this.bids = snapshot.bids;
      this.rollbackCount += 1;
      throw err;
    } finally {
      this.inTransaction = false;
    }
  }

  async deleteAllBids(): Promise<void> {
    this.bids = [];
  }
  async deleteAllInvoices(): Promise<void> {
    this.invoices = [];
  }
  async deleteAllUsers(): Promise<void> {
    this.users = [];
  }
  async createUsers(users: User[]): Promise<void> {
    this.users.push(...users.map((u) => ({ ...u })));
  }
  async createInvoices(invoices: Invoice[]): Promise<void> {
    this.invoices.push(...invoices.map((i) => ({ ...i })));
  }
  async createBids(bids: Bid[]): Promise<void> {
    this.bids.push(...bids.map((b) => ({ ...b })));
  }

  /** Read-only snapshot for assertions. */
  snapshot(): { users: User[]; invoices: Invoice[]; bids: Bid[] } {
    return {
      users: this.users.map((u) => ({ ...u })),
      invoices: this.invoices.map((i) => ({ ...i })),
      bids: this.bids.map((b) => ({ ...b })),
    };
  }

  reset(): void {
    this.users = [];
    this.invoices = [];
    this.bids = [];
    this.commitCount = 0;
    this.rollbackCount = 0;
    this.inTransaction = false;
  }
}

/* -------------------------------------------------------------------------- */
/* Run lease (single-writer + stale reclamation)                              */
/* -------------------------------------------------------------------------- */

interface Lease {
  owner: string;
  acquiredAt: number;
}

let activeLease: Lease | null = null;
let leaseCounter = 0;

export interface LeaseOutcome {
  runId: string;
  /** True when a lease left behind by an earlier run was reclaimed. */
  reclaimedStale: boolean;
}

/**
 * Acquires the single-writer lease.
 *
 * A lease is reclaimed (rather than honoured) once it is older than
 * `leaseMs`, which models recovery from a process that died mid-seed. A lease
 * that is still fresh causes a CONCURRENT_RUN rejection.
 */
export function acquireLease(now: number, leaseMs: number = DEFAULT_LEASE_MS): LeaseOutcome {
  leaseCounter += 1;
  const runId = `run-${leaseCounter}`;

  if (activeLease) {
    const age = now - activeLease.acquiredAt;
    if (age < leaseMs) {
      throw new SeedError(
        "CONCURRENT_RUN",
        `another seed run is already in progress (lease held for ${age}ms)`,
        { phase: "guard" }
      );
    }
  }

  const reclaimedStale = activeLease !== null;
  activeLease = { owner: runId, acquiredAt: now };
  return { runId, reclaimedStale };
}

export function releaseLease(): void {
  activeLease = null;
}

/** Test helper: drops any held lease. */
export function resetLeases(): void {
  activeLease = null;
  leaseCounter = 0;
}

/* -------------------------------------------------------------------------- */
/* Options, result, logger                                                    */
/* -------------------------------------------------------------------------- */

export interface SeedLogger {
  info(message: string, fields?: Record<string, unknown>): void;
  warn(message: string, fields?: Record<string, unknown>): void;
  error(message: string, fields?: Record<string, unknown>): void;
}

const consoleLogger: SeedLogger = {
  info: (message, fields) => console.log(`🌱 ${message}`, formatFields(fields)),
  warn: (message, fields) => console.warn(`⚠️  ${message}`, formatFields(fields)),
  error: (message, fields) => console.error(`❌ ${message}`, formatFields(fields)),
};

function formatFields(fields?: Record<string, unknown>): string {
  if (!fields) return "";
  const parts = Object.entries(fields).map(([k, v]) => `${k}=${String(v)}`);
  return parts.length ? `(${parts.join(", ")})` : "";
}

export interface SeedResult {
  status: "completed";
  runId: string;
  reclaimedStaleLease: boolean;
  attempts: number;
  created: { users: number; invoices: number; bids: number };
  completedAt: number;
}

export interface SeedOptions {
  /** Datastore adapter. Defaults to a fresh InMemorySeedStore. */
  store?: SeedStore;
  /** Explicit dataset. Defaults to the deterministic development dataset. */
  dataset?: SeedDataset;
  /** Environment map used for the production guard. Defaults to process.env. */
  env?: Record<string, string | undefined>;
  /** Injected clock. Defaults to the fixed SEED_EPOCH for reproducibility. */
  now?: number;
  /** Injected id factory. Defaults to a deterministic generator. */
  createId?: (kind: string, index: number) => string;
  /** Retry budget for transient failures. */
  maxAttempts?: number;
  /** Base backoff in ms; doubled per attempt. */
  retryBaseMs?: number;
  /** Injected sleep so tests never wait on real timers. */
  sleep?: (ms: number) => Promise<void>;
  /** Single-writer lease window. */
  leaseMs?: number;
  logger?: SeedLogger;
}

export interface ResolvedSeedOptions {
  store: SeedStore;
  dataset: SeedDataset;
  env: Record<string, string | undefined>;
  now: number;
  createId: (kind: string, index: number) => string;
  maxAttempts: number;
  retryBaseMs: number;
  sleep: (ms: number) => Promise<void>;
  leaseMs: number;
  logger: SeedLogger;
}

export function resolveOptions(options: SeedOptions = {}): ResolvedSeedOptions {
  const now = options.now ?? SEED_EPOCH;
  const createId = options.createId ?? createDeterministicIdFactory();
  return {
    store: options.store ?? new InMemorySeedStore(),
    dataset: options.dataset ?? buildDefaultDataset(now, createId),
    env: options.env ?? process.env,
    now,
    createId,
    maxAttempts: normalizePositiveInt(options.maxAttempts, DEFAULT_MAX_ATTEMPTS),
    retryBaseMs: normalizeNonNegativeInt(options.retryBaseMs, DEFAULT_RETRY_BASE_MS),
    sleep: options.sleep ?? ((ms: number) => new Promise((r) => setTimeout(r, ms))),
    leaseMs: normalizePositiveInt(options.leaseMs, DEFAULT_LEASE_MS),
    logger: options.logger ?? consoleLogger,
  };
}

function normalizePositiveInt(value: number | undefined, fallback: number): number {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 1) {
    return fallback;
  }
  return Math.floor(value);
}

function normalizeNonNegativeInt(value: number | undefined, fallback: number): number {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
    return fallback;
  }
  return Math.floor(value);
}

/* -------------------------------------------------------------------------- */
/* main                                                                       */
/* -------------------------------------------------------------------------- */

/**
 * Seeds the datastore.
 *
 * Resolves with a SeedResult on success and rejects with a SeedError on every
 * failure path. Never calls process.exit(), so it is safe to invoke directly
 * from tests or from a longer-lived process.
 */
export async function main(options: SeedOptions = {}): Promise<SeedResult> {
  const resolved = resolveOptions(options);
  const { store, dataset, env, logger } = resolved;

  // --- Permission boundary -------------------------------------------------
  if (env.NODE_ENV === "production") {
    const err = new SeedError(
      "PRODUCTION_FORBIDDEN",
      "seed script cannot be run in production environment"
    );
    logger.error(err.message, { code: err.code, phase: err.phase });
    throw err;
  }

  // --- Validation boundary (before any mutation) ---------------------------
  const reasons = validateDataset(dataset);
  if (reasons.length > 0) {
    const err = new SeedError(
      "VALIDATION_FAILED",
      `seed dataset rejected: ${reasons.length} problem(s) found`,
      { phase: "validating", reasons }
    );
    logger.error(err.message, { code: err.code, phase: err.phase, reasons: reasons.length });
    throw err;
  }

  // --- Single-writer boundary ---------------------------------------------
  const { runId, reclaimedStale } = acquireLease(resolved.now, resolved.leaseMs);
  if (reclaimedStale) {
    logger.warn("reclaimed a stale seed lease from an interrupted run", {
      runId,
      phase: "guard",
    });
  }

  const { users, invoices, bids } = dataset;

  try {
    logger.info("starting database seed", { runId, phase: "clearing" });

    // Everything below is one unit of work: either the whole seed lands or
    // none of it does.
    const { attempts } = await withRetry(
      async (attempt) => {
        logger.info("applying seed", { runId, attempt, phase: "clearing" });
        await store.transaction(async () => {
          // Child-first: bids reference invoices, invoices reference users.
          await store.deleteAllBids();
          await store.deleteAllInvoices();
          await store.deleteAllUsers();

          logger.info("creating sample users", {
            runId,
            phase: "creating_users",
            count: users.length,
          });
          await store.createUsers(users);

          logger.info("creating sample invoices", {
            runId,
            phase: "creating_invoices",
            count: invoices.length,
          });
          await store.createInvoices(invoices);

          logger.info("creating sample bids", {
            runId,
            phase: "creating_bids",
            count: bids.length,
          });
          await store.createBids(bids);
        });
      },
      {
        maxAttempts: resolved.maxAttempts,
        retryBaseMs: resolved.retryBaseMs,
        sleep: resolved.sleep,
        logger,
        runId,
      }
    );

    const result: SeedResult = {
      status: "completed",
      runId,
      reclaimedStaleLease: reclaimedStale,
      attempts,
      created: { users: users.length, invoices: invoices.length, bids: bids.length },
      completedAt: resolved.now,
    };

    logger.info("seed completed successfully", {
      runId,
      phase: "committing",
      users: result.created.users,
      invoices: result.created.invoices,
      bids: result.created.bids,
    });
    return result;
  } catch (err) {
    if (isSeedError(err)) {
      logger.error(err.message, { runId, code: err.code, phase: err.phase });
      throw err;
    }
    const wrapped = new SeedError("SEED_FAILED", "seed transaction failed and was rolled back", {
      phase: "committing",
      cause: err,
    });
    logger.error(wrapped.message, { runId, code: wrapped.code, phase: wrapped.phase });
    throw wrapped;
  } finally {
    releaseLease();
  }
}

interface RetryOptions {
  maxAttempts: number;
  retryBaseMs: number;
  sleep: (ms: number) => Promise<void>;
  logger: SeedLogger;
  runId: string;
}

/**
 * Runs `fn`, retrying only transient failures with deterministic backoff.
 *
 * A non-transient failure, or an exhausted budget, surfaces as a SeedError so
 * the operator can tell "the datastore was briefly unavailable" apart from
 * "the dataset is wrong".
 *
 * The attempt count is returned rather than tracked in module state, so
 * concurrent runs cannot observe each other's counters.
 */
export async function withRetry<T>(
  fn: (attempt: number) => Promise<T>,
  options: RetryOptions
): Promise<{ value: T; attempts: number }> {
  const { maxAttempts, retryBaseMs, sleep, logger, runId } = options;

  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    try {
      return { value: await fn(attempt), attempts: attempt };
    } catch (err) {
      const transient = isTransientError(err);
      if (!transient) {
        // Deterministic failure: retrying cannot change the outcome.
        throw err;
      }
      if (attempt === maxAttempts) {
        const exhausted = new SeedError(
          "RETRY_EXHAUSTED",
          `seed failed after ${maxAttempts} attempt(s) against a transient datastore error`,
          { phase: "committing", cause: err }
        );
        logger.error(exhausted.message, { runId, code: exhausted.code, attempts: attempt });
        throw exhausted;
      }
      const backoff = retryBaseMs * 2 ** (attempt - 1);
      logger.warn("transient datastore error; retrying", {
        runId,
        attempt,
        backoffMs: backoff,
        reason: describeError(err),
      });
      if (backoff > 0) {
        await sleep(backoff);
      }
    }
  }

  // Unreachable: the loop either returns or throws.
  throw new SeedError("RETRY_EXHAUSTED", "seed failed after exhausting retries", {
    phase: "committing",
  });
}

/**
 * Produces a short, log-safe description of an error.
 * Only the error's own name/code are used; payloads and messages that could
 * carry connection strings or credentials are dropped.
 */
function describeError(err: unknown): string {
  if (isSeedError(err)) return err.code;
  const name = (err as { name?: unknown } | null | undefined)?.name;
  const code = (err as { code?: unknown } | null | undefined)?.code;
  if (typeof code === "string") return code;
  if (typeof name === "string") return name;
  return "unknown";
}

/* -------------------------------------------------------------------------- */
/* CLI                                                                        */
/* -------------------------------------------------------------------------- */

export async function runCli(): Promise<number> {
  try {
    const result = await main();
    console.log(`Created: ${result.created.users} Users, ${result.created.invoices} Invoice, ${result.created.bids} Bid.`);
    return 0;
  } catch (err) {
    if (isSeedError(err)) {
      console.error(`Seed failed [${err.code}] during ${err.phase}: ${err.message}`);
      if (err.reasons && err.reasons.length > 0) {
        err.reasons.forEach((reason) => console.error(`  - ${reason}`));
      }
    } else {
      console.error("Seed failed with an unexpected error.");
    }
    return 1;
  }
}

if (require.main === module) {
  runCli()
    .then((code) => {
      process.exitCode = code;
    })
    .catch(() => {
      process.exitCode = 1;
    });
}

export { main as seedScript };

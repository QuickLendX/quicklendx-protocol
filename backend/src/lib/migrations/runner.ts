import { pathToFileURL } from "url";
import * as fs from "fs/promises";
import * as path from "path";
import { createHash } from "crypto";
import { AsyncLocalStorage } from "async_hooks";
import { getDatabase } from "../database";
import { config } from "../../config";
import type { MigrationContext, MigrationDefinition, MigrationState, ParsedMigration } from "./types";

/**
 * A single prepared statement on a `DatabaseClient`.
 *
 * `reader` is optional and, when present, is the driver's own "does this
 * statement return rows?" flag (better-sqlite3 exposes it). It is read at
 * prepare time — i.e. *before* the statement executes — so routing on it can
 * never execute a statement twice.
 */
export interface DatabaseStatement {
  all: (params?: unknown[]) => unknown;
  get: (params?: unknown[]) => unknown;
  run: (params?: unknown[]) => unknown;
  readonly reader?: boolean;
}

/**
 * The narrow database contract the migration runner depends on.
 *
 * Invariants callers must uphold:
 *  - `prepare(sql)` is called once per statement and must not execute it.
 *  - Statement methods take bind parameters as an **array**, and `undefined`
 *    means "no binds" (not "one NULL bind").
 *  - `exec(sql)` runs raw SQL (including multi-statement DDL) and returns
 *    nothing; it is used for transaction control statements.
 *  - `transaction(fn)` invokes `fn` immediately; it is NOT a transaction
 *    factory. Use {@link toDatabaseClient} to adapt a raw better-sqlite3
 *    handle, whose `transaction(fn)` returns a wrapper instead.
 */
export interface DatabaseClient {
  exec: (sql: string) => void;
  prepare: (sql: string) => DatabaseStatement;
  transaction: (fn: () => void) => void;
}

/**
 * Raw better-sqlite3-style handle, as returned by `getDatabase()`.
 * Statement methods are variadic and `transaction(fn)` is a factory.
 */
export interface RawDatabase {
  exec: (sql: string) => unknown;
  prepare: (sql: string) => {
    all: (...params: unknown[]) => unknown;
    get: (...params: unknown[]) => unknown;
    run: (...params: unknown[]) => unknown;
    readonly reader?: boolean;
  };
  transaction: <T>(fn: () => T) => () => T;
}

/** Result shape produced by the context's `db.run`, per `MigrationContext`. */
export interface RunResult {
  lastInsertRowId: number;
  changes: number;
}

/** Members a value must expose to be usable as a {@link DatabaseClient}. */
export const REQUIRED_CLIENT_METHODS = ["exec", "prepare", "transaction"] as const;

/** SQL keywords whose statements return rows, used when a driver has no `reader` flag. */
const ROW_RETURNING_KEYWORDS = new Set(["SELECT", "PRAGMA", "VALUES", "EXPLAIN", "WITH"]);

const BEGIN_SQL = "BEGIN IMMEDIATE";
const COMMIT_SQL = "COMMIT";
const ROLLBACK_SQL = "ROLLBACK";

/**
 * Per-async-flow transaction nesting depth.
 *
 * Each caller (each `runMigrations` invocation, each test) gets its own store,
 * so two *concurrent* runs on the same client are correctly seen as unrelated
 * flows rather than mistaken for a nested transaction.
 */
const transactionDepth = new AsyncLocalStorage<number>();

/** FIFO tail promises, keyed by client, used to serialize migrations per database. */
const clientLocks = new WeakMap<DatabaseClient, Promise<void>>();

const MIGRATIONS_TABLE = `
  CREATE TABLE IF NOT EXISTS _migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    checksum TEXT NOT NULL,
    applied_at TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    author TEXT NOT NULL,
    meta TEXT DEFAULT 't{}',
    UNIQUE(version)
  )
`;

const MIGRATIONS_DIR = path.resolve(process.cwd(), "src", "migrations");
const HOTFIX_APPROVALS_DIR_NAME = ".hotfix-approvals";
const HOTFIX_APPROVALS_DIR = path.resolve(process.cwd(), HOTFIX_APPROVALS_DIR_NAME);

// The only shape a migration name can legitimately have, per
// parseMigrationFilename. Enforced again when building an approval path so a
// ParsedMigration assembled by any other caller cannot point the approval
// check outside HOTFIX_APPROVALS_DIR via "../" or an absolute path.
const APPROVAL_NAME_PATTERN = /^[a-z0-9_]+$/;

/**
 * Computes a deterministic SHA-256 checksum for migration content.
 *
 * Invariants:
 * - The output is always a 64-character lowercase hex string.
 * - The same input always produces the same output (deterministic).
 * - No normalization is applied; bytes are hashed as-provided.
 * - Non-string inputs are rejected with a TypeError to avoid silent coercion.
 */
export function computeChecksum(content: string): string {
  if (typeof content !== "string") {
    throw new TypeError(
      `computeChecksum expects a string, received ${content === null ? "null" : typeof content}`
    );
  }
  return createHash("sha256").update(content, "utf-8").digest("hex");
}

export function parseMigrationFilename(filename: string): { version: number; name: string } | null {
  const match = filename.match(/^v?(\d{3})_([a-z0-9_]+)\.ts$/);
  if (!match) return null;
  return { version: parseInt(match[1], 10), name: match[2] };
}

export async function loadMigrationsFromFS(): Promise<ParsedMigration[]> {
  try {
    const files = await fs.readdir(MIGRATIONS_DIR);
    const migrations: ParsedMigration[] = [];

    for (const file of files) {
      const parsed = parseMigrationFilename(file);
      if (!parsed) continue;

      const filePath = path.join(MIGRATIONS_DIR, file);
      const content = await fs.readFile(filePath, "utf-8");

      let def: MigrationDefinition;
      try {
        def = (await import(pathToFileURL(filePath).href)).default as MigrationDefinition;
      } catch (err: any) {
        throw new Error(`Failed to load migration ${file}: ${err.message}`);
      }

      if (def.version !== parsed.version) {
        throw new Error(`Version mismatch in ${file}: filename ${parsed.version} but export ${def.version}`);
      }

      migrations.push({ file, version: parsed.version, name: parsed.name, content: def });
    }

    return migrations.sort((a, b) => a.version - b.version);
  } catch (err: any) {
    if (err.code === "ENOENT") return [];
    throw err;
  }
}

// Hotfix approval contract. Every branch fails closed: the function only ever
// returns true for a verified approval artifact, and anything it cannot
// decide it reports rather than guessing. Pinned by
// src/tests/migration-runner-hotfix.test.ts.
//
//   H1 A migration that is not a hotfix never needs approval.
//   H2 A hotfix is approved only when <version>_<name>.approval exists AND is
//      a regular file. A directory or any other node type is a broken
//      deployment, not an approval, and is reported as such.
//   H3 "No approval artifact" (ENOENT, ENOTDIR) is an expected outcome and
//      returns false; the caller turns that into the user-facing error.
//   H4 Any other filesystem failure (EACCES, EPERM, ELOOP, ...) is an
//      operational fault, not a verdict. It throws carrying the errno so an
//      operator is not sent hunting for a missing approval file that is
//      actually present but unreadable.
//   H5 The approval path is built from the filename-parsed version and name,
//      never from content.name, and the name is re-validated, so a crafted
//      migration name cannot redirect the check outside the approvals dir.
//   H6 Approval is a read-only check: no shared state, so repeated and
//      concurrent calls agree.
export async function isHotfixApproved(migration: ParsedMigration): Promise<boolean> {
  if (!migration.content.meta?.hotfix) return true;

  const label = `${migration.version}_${migration.name}`;

  if (!APPROVAL_NAME_PATTERN.test(migration.name)) {
    throw new Error(
      `Refusing to evaluate hotfix approval for ${label}: migration name is not a valid identifier.`
    );
  }

  // Only the artifact's file name is reported in errors, never the absolute
  // path, so deployment layout is not echoed into CI logs.
  const approvalFileName = `${label}.approval`;

  let stats: Awaited<ReturnType<typeof fs.stat>>;
  try {
    stats = await fs.stat(path.join(HOTFIX_APPROVALS_DIR, approvalFileName));
  } catch (error: any) {
    if (error?.code === "ENOENT" || error?.code === "ENOTDIR") {
      return false;
    }

    throw new Error(
      `Unable to evaluate hotfix approval for ${label}: ` +
      `${error?.code || error?.message || "unknown error"} reading ${approvalFileName}. ` +
      `Verify the ${HOTFIX_APPROVALS_DIR_NAME} directory exists and is readable.`
    );
  }

  if (!stats.isFile()) {
    throw new Error(
      `Hotfix approval ${approvalFileName} for ${label} is not a regular file. ` +
      `Remove the entry and create it as a file.`
    );
  }

  return true;
}

/**
 * Normalize a bind-parameter argument to the array form used by
 * {@link DatabaseStatement}. `undefined` and `null` both mean "no binds";
 * anything else is passed through untouched so malformed input surfaces as a
 * driver error instead of being silently coerced.
 */
function normalizeParams(params?: unknown[] | null): unknown[] {
  if (params === undefined || params === null) return [];
  return params;
}

/**
 * Decide whether a statement returns rows, without executing it.
 *
 * Preference order:
 *  1. The driver's own `reader` flag (resolved at prepare time — deterministic,
 *     and safe for `INSERT ... RETURNING` because nothing has run yet).
 *  2. A leading-keyword classification for drivers that do not expose `reader`.
 *     `WITH` is treated as row-returning because SQLite CTEs in this codebase
 *     are read queries.
 *
 * The classification is a fallback only: getting it wrong is preferable to the
 * previous behaviour of executing the same SQL twice, which the old
 * `exec -> db.all`/`db.run` split could do.
 */
function statementReturnsRows(statement: DatabaseStatement, sql: string): boolean {
  if (typeof statement.reader === "boolean") return statement.reader;
  // Leading comments and the whitespace that follows them are stripped before
  // the keyword is read; whitespace must be consumed *inside* the loop, because
  // `  SELECT 1` and `-- note\n  SELECT 1` are equally row-returning.
  const match = /^\s*(?:(?:--[^\n]*|\/\*[\s\S]*?\*\/)\s*)*([A-Za-z]+)/.exec(sql);
  if (!match) return false;
  return ROW_RETURNING_KEYWORDS.has(match[1].toUpperCase());
}

/**
 * Validate that a value can back a {@link DatabaseClient}.
 *
 * Fails fast at context-construction time so a misconfigured database handle is
 * reported before any SQL runs, rather than surfacing as an opaque
 * `TypeError` from deep inside a migration body (and, for the up/down paths,
 * as an unhandled rejection after the transaction had already committed).
 *
 * The error message names only the missing *members* and the received
 * `typeof` — never SQL text, bind values, or environment contents.
 *
 * @throws {TypeError} when `db` is not a usable {@link DatabaseClient}.
 */
export function assertDatabaseClient(db: unknown): asserts db is DatabaseClient {
  const missing = missingClientMembers(db);
  if (missing.length > 0) {
    throw new TypeError(
      `Invalid migration database client (received ${describe(db)}): missing or non-callable ` +
        `${missing.join(", ")}. Expected a DatabaseClient; see the interface in ` +
        `src/lib/migrations/runner.ts.`
    );
  }
}

/** List the {@link DatabaseClient} members a value is missing (empty when usable). */
function missingClientMembers(db: unknown): string[] {
  if (!db || typeof db !== "object") {
    return [...REQUIRED_CLIENT_METHODS];
  }
  const candidate = db as Record<string, unknown>;
  return REQUIRED_CLIENT_METHODS.filter((member) => typeof candidate[member] !== "function");
}

/** Type-only description of a rejected value. Never includes its contents. */
function describe(value: unknown): string {
  if (value === null) return "null";
  if (Array.isArray(value)) return "array";
  return typeof value;
}

/**
 * Adapt a raw better-sqlite3 handle (such as `getDatabase()`) to the narrower
 * {@link DatabaseClient} contract.
 *
 * Two semantic differences are bridged here:
 *  - better-sqlite3 statement methods are variadic, `DatabaseStatement` takes an
 *    array of binds.
 *  - better-sqlite3 `transaction(fn)` returns a wrapper and does not run `fn`.
 *
 * @throws {TypeError} when `raw` is not a usable raw database handle.
 */
export function toDatabaseClient(raw: RawDatabase): DatabaseClient {
  assertRawDatabase(raw);
  return {
    exec: (sql: string) => {
      raw.exec(sql);
    },
    prepare: (sql: string): DatabaseStatement => {
      const statement = raw.prepare(sql);
      return {
        all: (params?: unknown[]) => statement.all(...normalizeParams(params)),
        get: (params?: unknown[]) => statement.get(...normalizeParams(params)),
        run: (params?: unknown[]) => statement.run(...normalizeParams(params)),
        reader: typeof statement.reader === "boolean" ? statement.reader : undefined,
      };
    },
    transaction: (fn: () => void) => {
      raw.transaction(fn)();
    },
  };
}

function assertRawDatabase(raw: unknown): asserts raw is RawDatabase {
  const missing = missingClientMembers(raw);
  if (missing.length > 0) {
    throw new TypeError(
      `Invalid raw database handle (received ${describe(raw)}): missing or non-callable ` +
        `${missing.join(", ")}.`
    );
  }
}

/** Acquire the per-client FIFO lock; returns a release function. */
async function acquireClientLock(client: DatabaseClient): Promise<() => void> {
  const previous = clientLocks.get(client) ?? Promise.resolve();
  let release!: () => void;
  const current = new Promise<void>((resolve) => {
    release = resolve;
  });
  clientLocks.set(
    client,
    previous.then(() => current)
  );
  await previous;
  return () => {
    release();
  };
}

/**
 * Run `fn` inside a database transaction that survives `await` boundaries.
 *
 * `up`/`down`/`validate` are `async`, so a driver-managed transaction (which
 * commits when its *synchronous* callback returns) would commit before the
 * migration body has done any work. Explicit `BEGIN IMMEDIATE` / `COMMIT` /
 * `ROLLBACK` keeps the migration body and the runner's `_migrations`
 * bookkeeping in one atomic unit.
 *
 * Failure handling: on any throw — and on a rejected `COMMIT` — the
 * transaction is rolled back and the original error is rethrown unchanged. A
 * rollback that itself fails is swallowed so the actionable cause is never
 * masked, and the per-client lock is always released so a later run still
 * proceeds.
 *
 * Caller invariant: `fn` must be awaited. Work started inside `fn` but not
 * awaited by it outlives the transaction scope, and such a floating promise
 * would be misclassified as a nested call.
 */
async function runInTransaction<T>(
  client: DatabaseClient,
  contextDb: MigrationContext["db"],
  fn: (db: MigrationContext["db"]) => T | Promise<T>
): Promise<T> {
  const depth = transactionDepth.getStore() ?? 0;

  if (depth > 0) {
    // Nested call inside the same async flow: use a savepoint so the outer
    // transaction can still choose to roll back. The name is derived purely
    // from the nesting depth — it is never caller-controlled, and it cannot
    // collide because concurrent flows are serialized per client.
    const name = `qlx_migration_sp_${depth}`;
    client.exec(`SAVEPOINT ${name}`);
    try {
      const nested = await transactionDepth.run(depth + 1, () => fn(contextDb));
      client.exec(`RELEASE ${name}`);
      return nested;
    } catch (err) {
      try {
        client.exec(`ROLLBACK TO ${name}`);
        client.exec(`RELEASE ${name}`);
      } catch {
        /* keep the original failure */
      }
      throw err;
    }
  }

  const release = await acquireClientLock(client);
  try {
    client.exec(BEGIN_SQL);
    let result: T;
    try {
      result = await transactionDepth.run(1, () => fn(contextDb));
    } catch (err) {
      try {
        client.exec(ROLLBACK_SQL);
      } catch {
        /* keep the original failure */
      }
      throw err;
    }
    try {
      client.exec(COMMIT_SQL);
    } catch (err) {
      // A rejected COMMIT can leave the connection with a transaction still
      // open. Best-effort rollback so the connection is never handed to the
      // next migration inside a dangling transaction; the original error wins.
      try {
        client.exec(ROLLBACK_SQL);
      } catch {
        /* keep the original failure */
      }
      throw err;
    }
    return result;
  } finally {
    release();
  }
}

/**
 * Build the context handed to a migration's `up` / `down` / `validate`.
 *
 * Design invariants (all covered by `migration-build-context.test.ts`):
 *
 *  1. **Fail fast.** The client is validated here, so an unusable handle throws
 *     before any SQL is issued rather than mid-migration.
 *  2. **No implicit execution.** Every statement is routed through
 *     `prepare()`; a statement is executed exactly once, by exactly one of
 *     `exec` / `get` / `run`.
 *  3. **Stable result shapes.** `exec` always resolves to an array, `get` to a
 *     row or `undefined`, `run` to `{ lastInsertRowId, changes }` — regardless
 *     of what the driver returned.
 *  4. **Transactional scope covers `await`.** `db.transaction` commits only
 *     after the (possibly async) callback settles, rolls back on any throw,
 *     re-enters via savepoints, and serializes concurrent flows per client. It
 *     always returns a Promise, so callers must `await` it (the declared
 *     `MigrationContext` return type resolves to that Promise for async
 *     callbacks, which is how every migration uses it).
 *  5. **Nested callbacks get the context handle**, not the raw client.
 *
 * @throws {TypeError} when `db` is not a usable {@link DatabaseClient}.
 */
export function buildContext(db: DatabaseClient, isProd: boolean): MigrationContext {
  assertDatabaseClient(db);
  const client = db;

  const contextDb: MigrationContext["db"] = {
    async exec(sql: string, params?: unknown[]): Promise<unknown[]> {
      const statement = client.prepare(sql);
      if (!statementReturnsRows(statement, sql)) {
        statement.run(normalizeParams(params));
        return [];
      }
      const rows = statement.all(normalizeParams(params));
      return Array.isArray(rows) ? rows : [];
    },

    async get<T = unknown>(sql: string, params?: unknown[]): Promise<T | undefined> {
      const row = client.prepare(sql).get(normalizeParams(params));
      // Drivers disagree on "no row": better-sqlite3 yields undefined, others null.
      return (row === null ? undefined : row) as T | undefined;
    },

    async run(sql: string, params?: unknown[]): Promise<RunResult> {
      const result = client.prepare(sql).run(normalizeParams(params)) as
        | { changes?: unknown; lastInsertRowId?: unknown; lastInsertRowid?: unknown }
        | undefined;
      // Drivers disagree on the casing of the rowid field (better-sqlite3 spells
      // it `lastInsertRowid`), so both are accepted and normalized to 0.
      return {
        lastInsertRowId: toCount(result?.lastInsertRowId ?? result?.lastInsertRowid),
        changes: toCount(result?.changes),
      };
    },

    transaction<T>(fn: (db: MigrationContext["db"]) => T): T {
      return runInTransaction(client, contextDb, fn) as T;
    },
  };

  return {
    db: contextDb,
    env: process.env,
    isProduction: isProd,
    isTest: config.NODE_ENV === "test",
  };
}

function toCount(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

export async function runMigrations(options: { dryRun?: boolean; allowDown?: boolean; verbose?: boolean; skipChecksumVerify?: boolean; db?: DatabaseClient } = {}): Promise<{ applied: MigrationState[]; skipped: number; durationMs: number }> {
  const { dryRun = false, allowDown = false, verbose = false, skipChecksumVerify = false, db: providedDb } = options;
  const isProd = config.NODE_ENV === "production";
  const startTime = Date.now();

  // `getDatabase()` returns a raw better-sqlite3 handle whose statement methods
  // are variadic and whose `transaction(fn)` is a factory; adapt it so both the
  // default path and an injected client speak the same DatabaseClient contract.
  const db = providedDb || toDatabaseClient(getDatabase());
  db.exec(MIGRATIONS_TABLE);

  // Verify checksums of applied migrations on startup
  // In production, checksum verification cannot be bypassed
  if (skipChecksumVerify && isProd) {
    throw new Error("Checksum verification cannot be bypassed in production environment.");
  }

  if (!skipChecksumVerify && !dryRun) {
    const checksumCheck = await verifyAppliedChecksums(db);
    if (!checksumCheck.valid) {
      throw new Error(
        `Migration checksum verification failed:\n${checksumCheck.errors.map((e) => `  - ${e}`).join("\n")}\n` +
        "This indicates migration files have been modified after application. " +
        "Use --skip-checksum-verify to bypass in test environments only."
      );
    }
    if (verbose) console.log("✅ Checksum verification passed for all applied migrations");
  }

  const appliedRows = db.prepare(
    "SELECT version, name, checksum, applied_at, duration_ms, author, meta FROM _migrations ORDER BY version ASC"
  ).all() || [];
  const applied = new Map<number, MigrationState>();
  appliedRows.forEach((r: any) => {
    let meta: Record<string, unknown> = {};
    try {
      const parsed = JSON.parse(r.meta);
      if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
        meta = parsed as Record<string, unknown>;
      }
    } catch {
      meta = {};
    }
    applied.set(r.version, {
      version: r.version,
      name: r.name,
      checksum: r.checksum,
      appliedAt: r.applied_at,
      durationMs: r.duration_ms,
      author: r.author,
      meta,
    });
  });

  const fileMigrations = await loadMigrationsFromFS();
  const direction = allowDown ? "down" : "up";
  const targetVersions = direction === "up"
    ? fileMigrations.filter((m) => !applied.has(m.version)).map((m) => m.version)
    : fileMigrations.filter((m) => applied.has(m.version)).map((m) => m.version).sort((a, b) => b - a);

  let appliedThisRun: MigrationState[] = [];
  let skipped = 0;
  const ctx = buildContext(db, isProd);

  for (const version of targetVersions) {
    const fileMig = fileMigrations.find((m) => m.version === version)!;
    const existing = applied.get(version);

    if (direction === "up") {
      if (existing) {
        if (verbose) console.log(`⎩ Migration ${version}_${fileMig.name} already applied, skipping`);
        skipped++;
        continue;
      }

      if (isProd && !(await isHotfixApproved(fileMig))) {
        throw new Error(`Hotfix migration ${version}_${fileMig.name} lacks production approval.`);
      }

      if (fileMig.content.validate) {
        const warnings = await fileMig.content.validate(ctx);
        if (warnings.length > 0 && verbose) {
          console.warn(`⚠️  Migration ${version}_${fileMig.name} validation warnings:`);
          warnings.forEach((w) => console.warn(`   - ${w}`));
        }
      }

      if (!dryRun) {
        try {
          const fileContent = await fs.readFile(path.join(MIGRATIONS_DIR, fileMig.file), "utf-8");
          const checksum = computeChecksum(fileContent);
          const meta = fileMig.content.meta || {};
          const appliedAt = new Date().toISOString();
          const migStart = Date.now();
          // `durationMs` is assigned inside the transaction callback, so it is
          // declared out here — otherwise the verbose log below would read an
          // out-of-scope binding *after* the migration had already committed.
          let durationMs = 0;
          let state!: MigrationState;
          let appliedInTransaction = false;

          // The context's transaction helper is awaited so that the async `up`
          // body and the `_migrations` bookkeeping commit (or roll back)
          // together. A driver-managed transaction would commit as soon as its
          // synchronous callback returned — i.e. before `up` had run at all.
          // `up` receives the full MigrationContext (it calls `ctx.db.*`); the
          // callback argument is the bare db handle, per MigrationContext.
          await ctx.db.transaction(async () => {
            // Re-read under the transaction: a concurrent run sharing this
            // client may have applied the version while we waited for the
            // migration lock, and re-applying it would be a silent double
            // apply. The re-check makes concurrent/retried runs idempotent.
            const claimed = db
              .prepare("SELECT version FROM _migrations WHERE version = ?")
              .get([version]);
            if (claimed) return;

            const upFn = fileMig.content.up;
            if (!upFn) throw new Error(`Migration ${fileMig.file} missing up function`);
            await upFn(ctx);

            durationMs = Date.now() - migStart;
            state = {
              version,
              name: fileMig.name,
              checksum,
              appliedAt,
              durationMs,
              author: fileMig.content.author,
              meta,
            };

            db.prepare(
              "INSERT INTO _migrations (version, name, checksum, applied_at, duration_ms, author, meta) VALUES (?, ?, ?, ?, ?, ?, ?)"
            ).run([
              state.version,
              state.name,
              state.checksum,
              state.appliedAt,
              state.durationMs,
              state.author,
              JSON.stringify(state.meta),
            ]);
            appliedInTransaction = true;
          });
          applyTx();

          if (concurrentlyApplied) {
            skipped++;
            continue;
          }

          if (!appliedInTransaction) {
            if (verbose) console.log(`⏭  Migration ${version}_${fileMig.name} already applied, skipping`);
            skipped++;
            continue;
          }

          appliedThisRun.push(state);
          if (verbose) console.log(`✅ Applied migration ${version}_${fileMig.name} (${state.durationMs}ms)`);
        } catch (err: any) {
          console.error(`❌ Migration ${version}_${fileMig.name} failed:', err.message);
          throw err;
        }
      } else {
        if (verbose) console.log(`[DRY-RUN] Would apply migration ${version}_${fileMig.name}`);
        appliedThisRun.push({
          version,
          name: fileMig.name,
          checksum: "(dry-run)",
          appliedAt: new Date().toISOString(),
          durationMs: 0,
          author: fileMig.content.author,
          meta: fileMig.content.meta,
        });
      }
    } else {
      if (!allowDown) {
        throw new Error(`Down migrations are disabled. Use --allow-down flag to enable.`);
      }

      if (!existing) {
        if (verbose) console.log(`⎩  Migration ${version}_${fileMig.name} not applied, cannot rollback`);
        skipped++;
        continue;
      }

      if (!fileMig.content.down) {
        throw new Error(`Migration ${version}_${fileMig.name} has no down function.`);
      }

      if (isProd) {
        const approvalFile = path.join(HOTFIX_APPROVALS_DIR, `rollback_${version}_${fileMig.name}.approval`);
        try {
          await fs.access(approvalFile);
        } catch {
          throw new Error(`Rollback of ${version}_${fileMig.name} requires production approval.`);
        }
      }

      if (!dryRun) {
        const migStart = Date.now();
        try {
          let durationMs = 0;
          await ctx.db.transaction(async () => {
            const downFn = fileMig.content.down;
            if (!downFn) throw new Error(`Migration ${fileMig.file} missing down function`);
            await downFn(ctx);

            durationMs = Date.now() - migStart;
            db.prepare("DELETE FROM _migrations WHERE version = ?").run([version]);
          });
          rollbackTx();

          if (concurrentlyApplied) {
            skipped++;
            continue;
          }

          appliedThisRun.push({
            version,
            name: fileMig.name,
            checksum: existing.checksum,
            appliedAt: new Date().toISOString(),
            durationMs,
            author: fileMig.content.author,
            meta: fileMig.content.meta,
          });

if (verbose) console.log(`⬩ Rolled back migration ${version}_${fileMig.name} (${durationMs}ms`);
        } catch (err: any) {
          console.error(`❌ Rollback of ${version}_${fileMig.name} failed:', err.message);
          throw err;
        }
      } else {
        if (verbose) console.log(`[DRY-RUN] Would rollback migration ${version}_${fileMig.name}`);
        appliedThisRun.push({
          version,
          name: fileMig.name,
          checksum: existing.checksum,
          appliedAt: new Date().toISOString(),
          durationMs: 0,
          author: fileMig.content.author,
          meta: fileMig.content.meta,
        });
      }
    }
  }

  return { applied: appliedThisRun, skipped, durationMs: Date.now() - startTime };
}

/**
 * Versions recorded in `_migrations`, ascending.
 *
 * The bookkeeping table is created first (`CREATE TABLE IF NOT EXISTS`), so a
 * database that has never been migrated resolves to `[]` instead of failing
 * with `no such table`. That matters because {@link isDatabaseInitialized} uses
 * this to answer "has this database been migrated yet?" — it must be able to
 * answer "no" rather than throw on a fresh install.
 */
export async function getAppliedVersions(db?: DatabaseClient): Promise<number[]> {
  const database = db || toDatabaseClient(getDatabase());
  database.exec(MIGRATIONS_TABLE);
  const rows = database.prepare("SELECT version FROM _migrations ORDER BY version ASC").all() || [];
  return rows.map((r: any) => r.version);
}

/** True when at least one migration has been recorded as applied. */
export async function isDatabaseInitialized(db?: DatabaseClient): Promise<boolean> {
  const applied = await getAppliedVersions(db);
  return applied.length > 0;
}

/**
 * validateMigrationFiles — deterministic failure-boundary validation for the
 * migration file set loaded from the filesystem.
 *
 * Invariants enforced (in order):
 *  1. No duplicate version numbers (reports each duplicated version explicitly).
 *  2. Versions must start at 1 (version 0 is never valid).
 *  3. No gaps in the version sequence (every integer between 1 and max must exist).
 *  4. No duplicate migration names (a name collision causes split-brain in logs/metrics).
 *  5. Every migration must have the required fields: version (number > 0), name
 *     (non-empty string), author (non-empty string), authoredAt (non-empty string),
 *     and an `up` function.
 *  6. Hotfix migrations must additionally carry meta.reason, meta.rollback_risk,
 *     and a `down` function (delegated to MigrationPolicy.validateMetadata).
 *
 * Design notes:
 *  - All errors are collected before returning so callers receive a complete
 *    diagnostic list rather than failing on the first error (fail-complete, not
 *    fail-fast).
 *  - loadMigrationsFromFS already guards the filename/export-version mismatch
 *    and propagates filesystem errors; this function focuses on the logical
 *    consistency of the loaded set.
 *  - No database I/O is performed — this is a pure in-memory validation so it
 *    is safe to run at startup, in CI, and during dry-run without a connected DB.
 */
export async function validateMigrationFiles(): Promise<{ valid: boolean; errors: string[] }> {
  const errors: string[] = [];

  // loadMigrationsFromFS propagates hard filesystem errors (permissions, corrupt
  // files) and throws on filename/export version mismatches. We let those bubble
  // up as-is because they are unrecoverable and need operator attention.
  const migrations = await loadMigrationsFromFS();

  // ── 1. Duplicate version detection ───────────────────────────────────────────
  // Collect every version number, then find which ones appear more than once so
  // the error message names each offending version explicitly.
  const versionCounts = new Map<number, number>();
  for (const m of migrations) {
    versionCounts.set(m.version, (versionCounts.get(m.version) ?? 0) + 1);
  }

  const uniqueVersions = new Set(versions);
  if (uniqueVersions.size !== versions.length) {
    errors.push("Duplicate version numbers detected");
  }

  return { valid: errors.length === 0, errors };
}

export async function verifyAppliedChecksums(db?: DatabaseClient): Promise<{ valid: boolean; errors: string[] }> {
  const errors: string[] = [];
  const database = db || toDatabaseClient(getDatabase());
  
  // Ensure migrations table exists
  database.exec(MIGRATIONS_TABLE);
  
  const appliedRows = database.prepare(
    "SELECT version, name, checksum FROM _migrations ORDER BY version ASC"
  ).all() || [];
  
  const fileMigrations = await loadMigrationsFromFS();
  const fileMigrationMap = new Map(fileMigrations.map((m) => [m.version, m]));
  
  for (const row of appliedRows) {
    const fileMig = fileMigrationMap.get(row.version);
    if (!fileMig) {
      errors.push(`Applied migration ${row.version}_${row.name} not found in filesystem`);
      continue;
    }
    
    const filePath = path.join(MIGRATIONS_DIR, fileMig.file);
    const fileContent = await fs.readFile(filePath, "utf-8");
    const currentChecksum = computeChecksum(fileContent);
    
    if (currentChecksum !== row.checksum) {
      errors.push(
        `Duplicate version ${version}: found ${count} migration files with the same version number`
      );
    }
  }

  // Work on the deduplicated, sorted version list for sequence checks.
  const versions = Array.from(new Set(migrations.map((m) => m.version))).sort((a, b) => a - b);

  // ── 2. Version-zero guard ─────────────────────────────────────────────────────
  // Version 0 is explicitly prohibited; the sequence must begin at 1.
  if (versions.length > 0 && versions[0] === 0) {
    errors.push(
      "Version 0 is not allowed: migration versions must start at 1"
    );
  }

  // ── 3. Gap detection ──────────────────────────────────────────────────────────
  // After deduplication, every integer from 1 to max must be present.
  // Report each missing version individually so operators can act on all gaps
  // at once without having to re-run validation iteratively.
  for (let i = 0; i < versions.length; i++) {
    if (i === 0) {
      // The first version should be 1 (unless 0 was already reported above).
      if (versions[i] !== 0 && versions[i] !== 1) {
        errors.push(
          `Version sequence must start at 1, but the first migration found is version ${versions[i]}`
        );
      }
    } else {
      const expected = versions[i - 1] + 1;
      if (versions[i] !== expected) {
        // There may be multiple missing versions between two adjacent entries.
        for (let missing = expected; missing < versions[i]; missing++) {
          errors.push(`Gap detected: migration version ${missing} is missing`);
        }
      }
    }
  }

  // ── 4. Duplicate name detection ───────────────────────────────────────────────
  // Migration names are used in logs, metrics, and the _migrations table. A
  // collision causes ambiguity that cannot be resolved at runtime without
  // reading version numbers, which operators frequently do not do under pressure.
  const nameCounts = new Map<string, number[]>();
  for (const m of migrations) {
    if (!nameCounts.has(m.name)) nameCounts.set(m.name, []);
    nameCounts.get(m.name)!.push(m.version);
  }
  for (const [name, usedByVersions] of nameCounts) {
    if (usedByVersions.length > 1) {
      errors.push(
        `Duplicate migration name "${name}" used by versions: ${usedByVersions.sort((a, b) => a - b).join(", ")}`
      );
    }
  }

  // ── 5 & 6. Per-migration required-field and hotfix validation ─────────────────
  // MigrationPolicy.validateMetadata covers:
  //   - name non-empty
  //   - author non-empty
  //   - authoredAt non-empty
  //   - up function present
  //   - hotfix-specific fields (meta.reason, meta.rollback_risk, down function)
  //
  // We import lazily via require to avoid a circular dependency at module load time
  // (policy imports from runner, runner would import from policy). The dynamic
  // import is safe here because validateMigrationFiles is always async.
  //
  // Each error is prefixed with the migration identifier so the caller can
  // display them as a flat list without needing to group by migration.
  const { MigrationPolicy } = await import("./policy");
  for (const m of migrations) {
    const metaCheck = MigrationPolicy.validateMetadata(m.content);
    if (!metaCheck.valid) {
      for (const err of metaCheck.errors) {
        errors.push(`Migration ${m.version}_${m.name}: ${err}`);
      }
    }
  }

  return { valid: errors.length === 0, errors };
}

export interface ChecksumVerificationResult {
  valid: boolean;
  errors: string[];
}

export interface AppliedMugrationRow {
  version: number;
  name: string;
  checksum: string;
}

export interface VerifyAppliedChecksumsOptions {
  /** Override the migrations directory (used by tests). Defaults to src/migrations. */
  migrationsDir?: string;
  /** Override the applied-rows source (used by tests). Defaults to reading _migrations. */
  appliedRows?: AppliedMugrationRow[];
}

/**
 * Verifies that every applied migration in _migrations still matches the
 * current on-disk migration file.
 *
 * Invariants:
 *   - The function is pure with respect to its inputs: given the same
 *     database state and on-disk files, it always returns the same result.
 *   - It never throws for expected failure modes (DB read failure, missing
 *     file, mismatched checksum); instead it reports them in `errors`.
 *   - Error messages are deterministic and do not include sensitive data
 *     (no file contents, no absolute paths beyond the basename).
 *   - Duplicate applied rows for the same version are reported as an error
 *     rather than silently de-duplicated.
 */
export async function verifyAppliedChecksums(
  db?: DatabaseClient,
  options: VerifyAppliedChecksumsOptions = {},
): Promise<ChecksumVerificationResult> {
  const errors: string[] = [];
const migrationsDir = options.migrationsDir ?? MIGRATIONS_DIR;

  // Load applied rows. Prefer explicit override (tests), then the provided
  // database client, and finally the global database.
  let appliedRows: AppliedMugrationRow[];
  if (options.appliedRows) {
    appliedRows = options.appliedRows;
  } else {
    try {
      const database = db || getDatabase();
      const rows = database.prepare(
        "SELECT version, name, checksum FROM _migrations ORDER BY version ASC"
      ).all() || [];
      appliedRows = rows as AppliedMugrationRow[];
    } catch (err: any) {
      // A failure to read the _migrations table is not a checksum mismatch.
      // We report it as an error so callers can decide whether to fail closed.
      return {
        valid: false,
        errors: [`Unable to read applied migrations: ${err.message}`],
      };
    }
  }

  if (appliedRows.length === 0) {
    return { valid: true, errors };
  }

  // Detect duplicate versions in the applied set. This is a corruption
  // signal and must be surfaced explicitly rather than being masked by
  // Map de-duplication.
  const seenVersions = new Set<number>();
  for (const row of appliedRows) {
    if (seenVersions.has(row.version)) {
      errors.push(`Duplicate applied migration version ${row.version}`);
    }
    seenVersions.add(row.version);
  }

  // Index on-disk files by version. We only need the filename and content
  // for the applied versions, so we read the directory once and then
  // only read files we need.
  let fileNames: string[];
  try {
    fileNames = await fs.readdir(migrationsDir);
  } catch (err: any) {
    if (err.code === "ENOENT") {
      // No migrations directory but applied rows exist -> every applied
      // migration is now missing.
      for (const row of appliedRows) {
        errors.push(`Missing migration file for applied version ${row.version}`);
      }
      return { valid: errors.length === 0, errors };
    }
    return {
      valid: false,
      errors: [`Unable to read migrations directory: ${err.message}`],
    };
  }

  const fileByVersion = new Map<number, { file: string; name: string }>();
  for (const file of fileNames) {
    const parsed = parseMigrationFilename(file);
    if (!parsed) continue;
    if (fileByVersion.has(parsed.version)) {
      // Two files with the same version on disk. This is ambiguous and
      // would lead to non-deterministic behavior if we picked one.
      errors.push(`Duplicate migration file for version ${parsed.version}`);
      continue;
    }
    fileByVersion.set(parsed.version, { file, name: parsed.name });
  }

  for (const row of appliedRows) {
const on = fileByVersion.get(row.version);
    if (!on) {
      errors.push(`Applied migration ${row.version}_${row.name} has no corresponding file`);
      continue;
    }
const filePath = path.join(MIGRATIONS_DIR, fileMig.file);
    const fileContent = await fs.readFile(filePath, "utf-8");
    const actualChecksum = computeChecksum(fileContent);

    // Name must match the applied record. This catches renames that would
    // otherwise silently shift the meaning of a version.
    if (on.name !== row.name) {
      errors.push(
`Checksum mismatch for ${row.version}_${row.name}: expected ${row.checksum} but got ${actual}`
      );
    }
  }

  return { valid: errors.length === 0, errors };
}
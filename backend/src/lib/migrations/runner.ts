import * as fs from "fs/promises";
import * as path from "path";
import { createHash } from "crypto";
import { getDatabase } from "../database";
import { config } from "../../config";
import { MigrationErrorCodes } from "./types";
import type { MigrationDefinition, MigrationState, ParsedMigration } from "./types";

export interface DatabaseClient {
  exec: (sql: string) => void;
  prepare: (sql: string) => { all: (params?: unknown[]) => unknown[]; get: (params?: unknown[]) => unknown; run: (params?: unknown[]) => unknown };
  transaction: (fn: () => void) => void;
}

const MIGRATIONS_TABLE = `
  CREATE TABLE IF NOT EXISTS _migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    checksum TEXT NOT NULL,
    applied_at TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    author TEXT NOT NULL,
    meta TEXT DEFAULT '{}',
    UNIQUE(version)
  )
`;

const MIGRATIONS_DIR = path.resolve(process.cwd(), "src", "migrations");
const HOTFIX_APPROVALS_DIR = path.resolve(process.cwd(), ".hotfix-approvals");

/** Read-only query backing `getAppliedVersions`. Never interpolates input. */
const APPLIED_VERSIONS_SQL = "SELECT version FROM _migrations ORDER BY version ASC";

export function computeChecksum(content: string): string {
  return createHash("sha256").update(content).digest("hex");
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
        def = require(filePath).default as MigrationDefinition;
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

async function isHotfixApproved(migration: ParsedMigration): Promise<boolean> {
  if (!migration.content.meta?.hotfix) return true;
  const approvalFile = path.join(HOTFIX_APPROVALS_DIR, `${migration.version}_${migration.name}.approval`);
  try {
    await fs.access(approvalFile);
    return true;
  } catch {
    return false;
  }
}

function buildContext(db: any, isProd: boolean): any {
  return {
    db: {
      exec: (sql: string, params?: unknown[]) => db.all(sql, params),
      get: (sql: string, params?: unknown[]) => db.get(sql, params),
      run: (sql: string, params?: unknown[]) => db.run(sql, params),
      transaction: (fn: (db: any) => void) => db.transaction(() => fn(db)),
    },
    env: process.env,
    isProduction: isProd,
    isTest: config.NODE_ENV === "test",
  };
}

export async function runMigrations(options: { dryRun?: boolean; allowDown?: boolean; verbose?: boolean; skipChecksumVerify?: boolean; db?: DatabaseClient } = {}): Promise<{ applied: MigrationState[]; skipped: number; durationMs: number }> {
  const { dryRun = false, allowDown = false, verbose = false, skipChecksumVerify = false, db: providedDb } = options;
  const isProd = config.NODE_ENV === "production";
  const startTime = Date.now();

  const db = providedDb || getDatabase();
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
    applied.set(r.version, {
      version: r.version,
      name: r.name,
      checksum: r.checksum,
      appliedAt: r.applied_at,
      durationMs: r.duration_ms,
      author: r.author,
      meta: JSON.parse(r.meta),
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
        if (verbose) console.log(`⏭  Migration ${version}_${fileMig.name} already applied, skipping`);
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
        const migStart = Date.now();
        try {
          db.transaction(() => {
            const txCtx = buildContext(db, isProd);
            const upFn = fileMig.content.up;
            if (!upFn) throw new Error(`Migration ${fileMig.file} missing up function`);
            upFn(txCtx);
          });

          const durationMs = Date.now() - migStart;
          const fileContent = await fs.readFile(path.join(MIGRATIONS_DIR, fileMig.file), "utf-8");
          const checksum = computeChecksum(fileContent);
          const meta = fileMig.content.meta || {};
          const state: MigrationState = {
            version,
            name: fileMig.name,
            checksum,
            appliedAt: new Date().toISOString(),
            durationMs,
            author: fileMig.content.author,
            meta,
          };

          db.prepare(
            "INSERT INTO _migrations (version, name, checksum, applied_at, duration_ms, author, meta) VALUES (?, ?, ?, ?, ?, ?, ?)"
          ).run(state.version, state.name, state.checksum, state.appliedAt, state.durationMs, state.author, JSON.stringify(state.meta));

          appliedThisRun.push(state);
          if (verbose) console.log(`✅ Applied migration ${version}_${fileMig.name} (${durationMs}ms)`);
        } catch (err: any) {
          console.error(`❌ Migration ${version}_${fileMig.name} failed:`, err.message);
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
        if (verbose) console.log(`⏭  Migration ${version}_${fileMig.name} not applied, cannot rollback`);
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
          db.transaction(() => {
            const txCtx = buildContext(db, isProd);
            const downFn = fileMig.content.down;
            if (!downFn) throw new Error(`Migration ${fileMig.file} missing down function`);
            downFn(txCtx);
          });

          const durationMs = Date.now() - migStart;
          db.prepare("DELETE FROM _migrations WHERE version = ?").run(version);

          appliedThisRun.push({
            version,
            name: fileMig.name,
            checksum: existing.checksum,
            appliedAt: new Date().toISOString(),
            durationMs,
            author: fileMig.content.author,
            meta: fileMig.content.meta,
          });

          if (verbose) console.log(`⏪ Rolled back migration ${version}_${fileMig.name} (${durationMs}ms)`);
        } catch (err: any) {
          console.error(`❌ Rollback of ${version}_${fileMig.name} failed:`, err.message);
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
 * Deterministic failure-boundary error for reads of the `_migrations` ledger.
 *
 * Invariants:
 * - Every failure path in `getAppliedVersions` rejects with this error rather
 *   than returning a partial or malformed version list, so callers can never
 *   mis-order, skip, or re-apply migrations based on corrupt state.
 * - `message` is single-line and length-capped: diagnosable in logs and
 *   user-facing output without echoing raw driver payloads or ledger rows.
 * - `cause` preserves the original driver error for programmatic diagnosis.
 * - `retryable` reports whether a retry can succeed without any state change
 *   (the read itself is side-effect free apart from idempotent table DDL).
 */
export class MigrationStateReadError extends Error {
  /** Stable machine-readable code (see `MigrationErrorCodes`). */
  readonly code: string;
  /** True when the underlying failure is transient (e.g. SQLITE_BUSY). */
  readonly retryable: boolean;
  /** Original, unsanitized driver error for programmatic diagnosis. */
  cause?: unknown;

  constructor(message: string, options: { retryable?: boolean; cause?: unknown } = {}) {
    super(message);
    this.name = "MigrationStateReadError";
    this.code = MigrationErrorCodes.MIGRATION_STATE_READ_FAILED;
    this.retryable = options.retryable ?? false;
    this.cause = options.cause;
  }
}

/**
 * Collapse a driver error into a single-line, length-capped summary.
 *
 * Driver messages can be multi-line and may embed SQL, filesystem paths, or
 * row payloads, so callers and logs only receive a sanitized summary while the
 * untouched original stays available on `MigrationStateReadError.cause`.
 */
function sanitizeDriverMessage(err: unknown): string {
  let raw: string;
  if (err instanceof Error) {
    raw = err.message;
  } else if (err !== null && typeof err === "object" && typeof (err as { message?: unknown }).message === "string") {
    raw = (err as { message: string }).message;
  } else {
    raw = String(err);
  }
  const flattened = raw.replace(/\s+/g, " ").trim();
  if (flattened.length === 0) return "unknown error";
  return flattened.length > 200 ? `${flattened.slice(0, 200)}...` : flattened;
}

/** Driver codes that indicate a transient condition a retry may resolve. */
const RETRYABLE_DRIVER_CODES = new Set(["SQLITE_BUSY", "SQLITE_LOCKED", "SQLITE_TIMEOUT", "EBUSY", "EAGAIN", "ETIMEDOUT"]);
/** SQLite primary result codes: 5 = SQLITE_BUSY, 6 = SQLITE_LOCKED. */
const RETRYABLE_SQLITE_PRIMARY_CODES = new Set([5, 6]);

/**
 * Decide whether a driver failure is transient. Matches the driver `code`,
 * the numeric `errcode` masked to its primary SQLite result code (so extended
 * codes such as 0x105 SQLITE_BUSY_SNAPSHOT also match), or well-known lock
 * message text. Reads are idempotent, so `retryable` failures are always safe
 * to retry — retries can never produce an inconsistent result here.
 */
function isRetryableDbError(err: unknown): boolean {
  if (err === null || typeof err !== "object") return false;
  const { code, errcode, message } = err as { code?: unknown; errcode?: unknown; message?: unknown };
  if (typeof code === "string" && RETRYABLE_DRIVER_CODES.has(code)) return true;
  if (typeof errcode === "number" && RETRYABLE_SQLITE_PRIMARY_CODES.has(errcode & 0xff)) return true;
  if (typeof message === "string") {
    const lower = message.toLowerCase();
    if (lower.includes("database is locked") || lower.includes("database is busy")) return true;
  }
  return false;
}

/**
 * Describe a value for diagnostics without dumping raw row payloads.
 * Primitive values are included (truncated) because they identify the bad
 * column cheaply; structured values are reported by type only so ledger
 * contents can never leak into logs or user-facing errors.
 */
function describeValue(value: unknown): string {
  if (value === null) return "null";
  if (Array.isArray(value)) return "array";
  const type = typeof value;
  if (type === "string" || type === "number" || type === "boolean" || type === "bigint") {
    const repr = String(value as string | number | boolean | bigint);
    return repr.length > 64 ? `${type}(${repr.slice(0, 64)}...)` : `${type}(${repr})`;
  }
  return type; // undefined, object, function, symbol
}

/**
 * Convert a stored `_migrations.version` cell to a safe integer.
 *
 * Accepts integers and integer-valued strings (drivers differ in how they
 * serialize INTEGER columns). Anything else is a hard failure: returning a
 * bogus version could make callers mis-order or re-apply migrations.
 * Boundary cases: version 0 is valid (the filename regex allows `v000`);
 * negative, fractional, unsafe-integer, and empty values are rejected.
 *
 * @param value    raw `version` cell from the ledger row
 * @param rowIndex zero-based row index, used for diagnostics only
 * @throws {MigrationStateReadError} when the cell is not a valid version
 */
function toMigrationVersion(value: unknown, rowIndex: number): number {
  let version: number;
  if (typeof value === "number") {
    version = value;
  } else if (typeof value === "string" && value.trim() !== "") {
    version = Number(value);
  } else {
    throw new MigrationStateReadError(
      `Invalid _migrations row: version at index ${rowIndex} must be an integer, received ${describeValue(value)}.`
    );
  }
  if (!Number.isSafeInteger(version) || version < 0) {
    throw new MigrationStateReadError(
      `Invalid _migrations row: version at index ${rowIndex} must be a non-negative safe integer, received ${describeValue(value)}.`
    );
  }
  return version;
}

/**
 * Read the applied migration versions from the `_migrations` ledger.
 *
 * Deterministic failure-boundary invariants:
 * - Output contract: an ascending, de-duplicated array of non-negative safe
 *   integers, or a rejected promise. A partial or malformed list is never
 *   returned, so callers cannot act on corrupt migration state.
 * - Fresh database: the ledger table is bootstrapped with the same idempotent
 *   `CREATE TABLE IF NOT EXISTS` used by `runMigrations()` and
 *   `verifyAppliedChecksums()` (when the adapter exposes `exec`), so an
 *   uninitialized database deterministically yields `[]` instead of throwing.
 * - Errors: every failure path (connection acquisition, permission denied,
 *   table bootstrap, statement preparation, execution, malformed rows) rejects
 *   with `MigrationStateReadError` (`code: MIGRATION_STATE_READ_FAILED`) whose
 *   message is sanitized for logs and whose `cause` carries the raw error.
 * - Retry & concurrency: the call is read-only apart from the idempotent DDL
 *   above, so retries and concurrent invocations are safe and yield identical
 *   results; transient lock/busy failures are flagged `retryable === true`.
 * - Compatibility: `db` remains optional, legacy adapters that implement only
 *   `prepare` keep working, and nullish query results still mean an empty
 *   ledger (preserves the previous `|| []` behavior).
 *
 * @param db optional injected client; defaults to the shared connection
 * @throws {MigrationStateReadError} on any failure or malformed ledger row
 */
export async function getAppliedVersions(db?: DatabaseClient): Promise<number[]> {
  let database: DatabaseClient;
  try {
    database = db || getDatabase();
  } catch (err) {
    throw new MigrationStateReadError(
      `Unable to acquire database handle for migration state read: ${sanitizeDriverMessage(err)}`,
      { cause: err, retryable: isRetryableDbError(err) }
    );
  }
  if (!database) {
    throw new MigrationStateReadError(
      "Unable to acquire database handle for migration state read: no database handle available."
    );
  }

  // Legacy adapters may implement only `prepare` (existing callers in this
  // repo do); the real better-sqlite3 handle always exposes `exec`.
  try {
    if (typeof database.exec === "function") {
      database.exec(MIGRATIONS_TABLE);
    }
  } catch (err) {
    throw new MigrationStateReadError(
      `Failed to ensure _migrations ledger exists: ${sanitizeDriverMessage(err)}`,
      { cause: err, retryable: isRetryableDbError(err) }
    );
  }

  let rows: unknown;
  try {
    rows = database.prepare(APPLIED_VERSIONS_SQL).all();
  } catch (err) {
    throw new MigrationStateReadError(
      `Failed to read applied migration ledger: ${sanitizeDriverMessage(err)}`,
      { cause: err, retryable: isRetryableDbError(err) }
    );
  }

  // Legacy contract: adapters may signal "no rows" with a nullish result.
  if (rows === null || rows === undefined) return [];

  if (!Array.isArray(rows)) {
    throw new MigrationStateReadError(
      `Failed to read applied migration ledger: expected an array of rows, received ${describeValue(rows)}.`,
      { cause: rows }
    );
  }

  // Validate every row before returning anything: a failure mid-way throws
  // before any partial result can escape to the caller.
  const versions = new Set<number>();
  rows.forEach((row: unknown, index: number) => {
    if (row === null || typeof row !== "object" || Array.isArray(row)) {
      throw new MigrationStateReadError(
        `Failed to read applied migration ledger: row at index ${index} must be an object, received ${describeValue(row)}.`,
        { cause: row }
      );
    }
    versions.add(toMigrationVersion((row as { version?: unknown }).version, index));
  });

  // Numeric (not lexicographic) ascending order, guaranteed even if an adapter
  // ignores ORDER BY; the Set collapses duplicate ledger rows defensively.
  return [...versions].sort((a, b) => a - b);
}

export async function isDatabaseInitialized(db?: DatabaseClient): Promise<boolean> {
  const applied = await getAppliedVersions(db);
  return applied.length > 0;
}

export async function validateMigrationFiles(): Promise<{ valid: boolean; errors: string[] }> {
  const errors: string[] = [];
  const migrations = await loadMigrationsFromFS();

  const versions = migrations.map((m) => m.version).sort((a, b) => a - b);
  for (let i = 0; i < versions.length; i++) {
    if (i > 0 && versions[i] !== versions[i - 1] + 1) {
      errors.push(`Gap detected: migration ${versions[i - 1] + 1} is missing`);
    }
  }

  const uniqueVersions = new Set(versions);
  if (uniqueVersions.size !== versions.length) {
    errors.push("Duplicate version numbers detected");
  }

  return { valid: errors.length === 0, errors };
}

export async function verifyAppliedChecksums(db?: DatabaseClient): Promise<{ valid: boolean; errors: string[] }> {
  const errors: string[] = [];
  const database = db || getDatabase();
  
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
        `Checksum mismatch for migration ${row.version}_${row.name}: ` +
        `expected ${row.checksum}, got ${currentChecksum}. ` +
        `Migration file may have been modified after application.`
      );
    }
  }
  
  return { valid: errors.length === 0, errors };
}

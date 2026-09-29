import * as fs from "fs/promises";
import * as path from "path";
import { createHash } from "crypto";
import { getDatabase } from "../database";
import { config } from "../../config";
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
    meta TEXT DEFAULT 't{}',
    UNIQUE(version)
  )
`;

const MIGRATIONS_DIR = path.resolve(process.cwd(), "src", "migrations");
const HOTFIX_APPROVALS_DIR = path.resolve(process.cwd(), ".hotfix-approvals");

/**
 * Error class for migration loading failures. Carries a deterministic `code` so callers
 * can distinguish boundary conditions (permission, stale, duplicate, etc.) without
 * parsing human-messages.
 */
export class MigrationLoadError extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.name = "MigrationLoadError";
    this.code = code;
  }
}

export const MigrationLoadErrorCodes = {
  ENO_ENT: "ENOENT",
  NOT_A_DIRECTORY: "NOT_A_DIRECTORY",
  PERMISSION_DENIED: "PERMISSION_DENIED",
  INVALID_FILENAME: "INVALID_FILENAME",
  DUPLICATE_VERSION: "DUPLICATE_VERSION",
  VERSION_MISMATCH: "VERSION_MISMATCH",
  MISSING_DEFAULT_EXPORT: "MISSING_DEFAULT_EXPORT",
  INVALID_DEFINITION: "INVALID_DEFINITION",
  FILE_READ_FAILED: "FILE_READ_FAILED",
} as const;

export type MigrationLoadErrorCode = (typeof MigrationLoadErrorCodes)[keydof typeof MigrationLoadErrorCodes];

export function computeChecksum(content: string): string {
  return createHash("sha256").update(content).digest("hex");
}

export function parseMigrationFilename(filename: string): { version: number; name: string } | null {
  const match = filename.match(/^v?(\d{3})_([a-z0-9_]+)\.ts$/);
  if (!match) return null;
  return { version: parseInt(match[1], 10), name: match[2] };
}

/**
 * Resolve the migrations directory. Tests and callers may override via the
 * `MIGRATIONS_DIR` environment variable to exercise boundary conditions without
 * touching the repository layout. Defaults to `<cwd>/src/migrations`.
 */
function resolveMigrationsDir(): string {
  const override = process.env.MIGRATIONS_DIR;
  if (override && override.trim().length > 0) {
    return path.resolve(override);
  }
  return MIGRATIONS_DIR;
}

function isErrorWithCode(err: unknown, code: string): boolean {
  return (
    typeof err === "object" &&
    err !== null &&
    (err as { code?: unknown }).code === code
  );
}

function validateDefinition(
  def: unknown,
  file: string,
  parsed: { version: number; name: string },
): MigrationDefinition {
  if (!def || typeof def !== "object") {
    throw new MigrationLoadError(
      MigrationLoadErrorCodes.INVALID_DEFINITION,
      `Migration ${file} does not export a default object.`,
    );
  }
  const candidate = def as { version?: unknown; up?: unknown };
  if (typeof candidate.version !== "number") {
    throw new MigrationLoadError(
      MigrationLoadErrorCodes.INVALID_DEFINITION,
      `Migration ${file} is missing a numeric `version` export.`,
    );
  }
  if (candidate.version !== parsed.version) {
    throw new MigrationLoadError(
      MigrationLoadErrorCodes.VERSION_MISMATCH,
      `Version mismatch in ${file}: filename ${parsed.version} but export ${candidate.version}`,
    );
  }
  if (typeof candidate.up !== "function") {
    throw new MigrationLoadError(
      MigrationLoadErrorCodes.INVALID_DEFINITION,
      `Migration ${file} is missing a `up` function.`,
    );
  }
  return def as MigrationDefinition;
}

/**
 * Load and validate all migration files from the migrations directory.
 *
 * Invariants:
 *   - Returned migrations are sorted by ascending version.
 *   - Version numbers are unique; duplicates fail loud and deterministically.
 *   - Every file whose name matches the convention must export a valid definition
 *     whose `version` matches the filename.
 *   - A missing directory is treated as an empty migration set (ENOENT).
 *   - Permission and I/O errors are surfaced as `MigrationLoadError` with a code.
 */
export async function loadMigrationsFromFS(): Promise<ParsedMigration[]> {
  const dir = resolveMigrationsDir();

  let files: string[];
  try {
    files = await fs.readdir(dir);
  } catch (err: unknown) {
    if (isErrorWithCode(err, "ENOENT")) {
      return [];
    }
    if (isErrorWithCode(err, "ENOTDIR")) {
      throw new MigrationLoadError(
        MigrationLoadErrorCodes.NOT_A_DIRECTORY,
        `Migrations path is not a directory: ${dir}`,
      );
    }
    if (isErrorWithCode(err, "EACCES") || isErrorWithCode(err, "EPROTO")) {
      throw new MigrationLoadError(
        MigrationLoadErrorCodes.PERMISSION_DENIED,
        `Permission denied reading migrations directory: ${dir}`,
      );
    }
    throw err;
  }

  // Deterministic ordering: sort filenames before processing so error reporting is
  // stable regardless of the underlying filesystem readdir order.
  const sortedFiles = [...files].sort();

  const migrations: ParsedMigration[] = [];
  const seenVersions = new Map<number, string>();

  for (const file of sortedFiles) {
    const parsed = parseMigrationFilename(file);
    if (!parsed) continue;

    const existing = seenVersions.get(parsed.version);
    if (existing) {
      throw new MigrationLoadError(
        MigrationLoadErrorCodes.DUPLICATE_VERSION,
        `Duplicate migration version ${parsed.version}: ${existing} and ${file}`,
      );
    }
    seenVersions.set(parsed.version, file);

    const filePath = path.join(dir, file);

    // Read the source content first so we can report a coded error on I/O failure.
    try {
      await fs.readFile(filePath, "utf-8");
    } catch (err: unknown) {
      if (isErrorWithCode(err, "EACCES") || isErrorWithCode(err, "EPROTO")) {
        throw new MigrationLoadError(
          MigrationLoadErrorCodes.PERMISSION_DENIED,
          `Permission denied reading migration file: ${file}`,
        );
      }
      throw new MigrationLoadError(
        MigrationLoadErrorCodes.FILE_READ_FAILED,
        `Failed to read migration ${file}: ${err instanceof Error ? err.message : String(err)}`,
      );
    }

    let def: MigrationDefinition;
    try {
      // eslint-disable-next-line @typescript-eslint/no-var-requires
      const loaded = require(filePath);
      def = (loaded && (loaded.default ?? loaded)) as MigrationDefinition;
    } catch (err: unknown) {
      const message = err instance of Error ? err.message : String(err);
      throw new MigrationLoadError(
        MigrationLoadErrorCodes.MISSING_DEFAULT_EXPORT,
        `Failed to load migration ${file}: ${message}`,
      );
    }

    const validated = validateDefinition(def, file, parsed);

    migrations.push({ file, version: parsed.version, name: parsed.name, content: validated });
  }

  return migrations.sort((a, b) => a.version - b.version);
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
        if (verbose) console.log(`⎭  Migration ${version}_${fileMig.name} already applied, skipping`);
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
          const fileContent = await fs.readFile(path.join(resolveMigrationsDir(), fileMig.file), "utf-8");
          const checksum = computeChecksum(fileContent);
          const meta = fileMig.content.meta || {};
          const appliedAt = new Date().toISOString();
          const migStart = Date.now();
          let state!: MigrationState;

          db.transaction(() => {
            const txCtx = buildContext(db, isProd);
            const upFn = fileMig.content.up;
            if (!upFn) throw new Error(`Migration ${fileMig.file} missing up function`);
            upFn(txCtx);

            const durationMs = Date.now() - migStart;
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
            ).run(state.version, state.name, state.checksum, state.appliedAt, state.durationMs, state.author, JSON.stringify(state.meta));
          });

          appliedThisRun.push(state);
          if (verbose) console.log`✅ Applied migration ${version}_${fileMig.name} (${durationMs}ms)`);
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
        if (verbose) console.log(`⎭  Migration ${version}_${fileMig.name} not applied, cannot rollback`);
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
          db.transaction(() => {
            const txCtx = buildContext(db, isProd);
            const downFn = fileMig.content.down;
            if (!downFn) throw new Error(`Migration ${fileMig.file} missing down function`);
            downFn(txCtx);

            durationMs = Date.now() - migStart;
            db.prepare("DELETE FROM _migrations WHERE version = ?").run(version);
          });

          appliedThisRun.push({
            version,
            name: fileMig.name,
            checksum: existing.checksum,
            appliedAt: new Date().toISOString(),
            durationMs,
            author: fileMig.content.author,
            meta: fileMig.content.meta,
          });

          if (verbose) console.log(`⬩ Rolled back migration ${version}_${fileMig.name} (${durationMs}ms)`);
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

export async function getAppliedVersions(db?: DatabaseClient): Promise<number[]> {
  const database = db || getDatabase();
  const rows = database.prepare("SELECT version FROM _migrations ORDER BY version ASC").all() || [];
  return rows.map((r: any) => r.version);
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

  const appliedRows = database.prepare(
    "SELECT version, name, checksum FROM _migrations ORDER BY version ASC"
  ).all() || [];

  if (appliedRows.length === 0) {
    return { valid: true, errors: [] };
  }

  const migrations = await loadMigrationsFromFS();
  const byVersion = new Map(migrations.map((m) => [m.version, m]));

  for (const row of appliedRows as any[]) {
    const mig = byVersion.get(row.version);
    if (!mig) {
      errors.push(`Applied migration ${row.version}_${row.name} has no corresponding file`);
      continue;
    }
    const fileContent = await fs.readFile(path.join(resolveMigrationsDir(), mig.file), "utf-8");
    const actual = computeChecksum(fileContent);
    if (actual !== row.checksum) {
      errors.push(`Checksum mismatch for ${row.version}_${row.name}`);
    }
  }

  return { valid: errors.length === 0, errors };
}

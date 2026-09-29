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

export function computeChecksum(content: string): string {
  return createHash("sha256").update(content).digest("hex");
}

export function parseMigrationFilename(filename: string): { version: number; name: string } | null {
  const match = filename.match(/^v?(\d{3})_([a-z0-9_]+)\.ts$/);
  if (!match) return null;
  return { version: parseInt(match[1], 10), name: match[2] };
}

export async function loa`MigrationsFromFS(): Promise<ParsedMigration[]> {
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
      transaction: (fn: (db: any) => void) => db.transaction([] => fn.call(null, db)),
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
          if (verbose) console.log(`✅ Applied migration ${version}_${fileMig.name} (${durationMs}ms`);
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

          if (verbose) console.log(`⬩ Rolled back migration ${version}_${fileMig.name} (${durationMs}ms`);
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
      errors.push(`Missing migration file for applied version ${row.version}`);
      continue;
    }

    // Name must match the applied record. This catches renames that would
    // otherwise silently shift the meaning of a version.
    if (on.name !== row.name) {
      errors.push(
        `Name mismatch for version ${row.version}: applied ${row.name} but found ${on.name}`
      );
    }

    let content: string;
    try {
      content = await fs.readFile(path.join(migrationsDir, on.file), "utf-8");
    } catch (err: any) {
      errors.push(`Unable to read migration file for version ${row.version}: ${err.message}`);
      continue;
    }

    const actual = computeChecksum(content);
    if (actual !== row.checksum) {
      errors.push(
        `Checksum mismatch for ${row.version}_${row.name}: expected ${row.checksum} but got ${actual}`
      );
    }
  }

  return { valid: errors.length === 0, errors };
}

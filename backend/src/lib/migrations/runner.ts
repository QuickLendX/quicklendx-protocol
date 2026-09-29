import * as fs from "fs/promises";
import * as path from "path";
import { createHash } from "crypto";
import { getDatabase } from "../database";
import { config } from "../../config";
import type { MigrationDefinition, MigrationState, ParsedMigration } from "./types";

export interface DatabaseClient {
  exec: (sql: string) => void;
  prepare: (sql: string) => { all: (params?: unknown[]) => unknown[]; get: (params?: unknown[]) => unknown; run: (params?: unknown[]) => unknown };
  /**
   * Returns a transaction-wrapped function (better-sqlite3 semantics). The
   * caller must invoke the returned function; calling `db.transaction(fn)`
   * alone does NOT execute `fn`.
   */
  transaction: (fn: () => void) => () => void;
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
const HOTFIX_APPROVALS_DIR_NAME = ".hotfix-approvals";
const HOTFIX_APPROVALS_DIR = path.resolve(process.cwd(), HOTFIX_APPROVALS_DIR_NAME);

// The only shape a migration name can legitimately have, per
// parseMigrationFilename. Enforced again when building an approval path so a
// ParsedMigration assembled by any other caller cannot point the approval
// check outside HOTFIX_APPROVALS_DIR via "../" or an absolute path.
const APPROVAL_NAME_PATTERN = /^[a-z0-9_]+$/;

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

function buildContext(db: any, isProd: boolean): any {
  return {
    db: {
      exec: (sql: string, params?: unknown[]) => db.exec(sql),
      get: (sql: string, params?: unknown[]) => db.prepare(sql).get(...(params || [])),
      run: (sql: string, params?: unknown[]) => db.prepare(sql).run(...(params || [])),
      transaction: (fn: (db: any) => void) => {
        const wrapped = db.transaction(() => fn(db));
        return wrapped();
      },
    },
    env: process.env,
    isProduction: isProd,
    isTest: config.NODE_ENV === "test",
  };
}

export async function runMigrations(options: { dryRun?: boolean; allowDown?: boolean; verbose?: boolean; skipChecksumVerify?: boolean; db?: DatabaseClient; to?: string; all?: boolean } = {}): Promise<{ applied: MigrationState[]; skipped: number; durationMs: number }> {
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
        try {
          const fileContent = await fs.readFile(path.join(MIGRATIONS_DIR, fileMig.file), "utf-8");
          const checksum = computeChecksum(fileContent);
          const meta = fileMig.content.meta || {};
          const appliedAt = new Date().toISOString();
          const migStart = Date.now();
          let state!: MigrationState;
          let durationMs = 0;

          let concurrentlyApplied = false;
          const applyTx = db.transaction(() => {
            const txCtx = buildContext(db, isProd);
            const upFn = fileMig.content.up;
            if (!upFn) throw new Error(`Migration ${fileMig.file} missing up function`);
            // Re-check inside the transaction: a concurrent run may have applied
            // this version after our initial snapshot. better-sqlite3 executes
            // statements synchronously on one connection, so this select-then-apply
            // sequence is atomic per worker. INSERT OR IGNORE is a backstop so a
            // duplicate version is treated as "already applied" (skipped) instead of
            // aborting the whole run with a UNIQUE constraint error.
            const alreadyAppliedRow = db.prepare("SELECT 1 AS present FROM _migrations WHERE version = ?").get(version);
            if (alreadyAppliedRow) {
              concurrentlyApplied = true;
              return;
            }
            upFn(txCtx);

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

            const inserted = db.prepare(
              "INSERT OR IGNORE INTO _migrations (version, name, checksum, applied_at, duration_ms, author, meta) VALUES (?, ?, ?, ?, ?, ?, ?)"
            ).run(state.version, state.name, state.checksum, state.appliedAt, state.durationMs, state.author, JSON.stringify(state.meta));
            if ((inserted as any).changes === 0) concurrentlyApplied = true;
          });
          applyTx();

          if (concurrentlyApplied) {
            skipped++;
            continue;
          }

          appliedThisRun.push(state);
          if (verbose) console.log(`✅ Applied migration ${version}_${fileMig.name} (${state.durationMs}ms)`);
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
          let durationMs = 0;
          let concurrentlyApplied = false;
          const rollbackTx = db.transaction(() => {
            // Re-check inside the transaction: a concurrent run may have already
            // rolled this version back after our initial snapshot.
            const existingRow = db.prepare("SELECT 1 AS present FROM _migrations WHERE version = ?").get(version);
            if (!existingRow) {
              concurrentlyApplied = true;
              return;
            }
            const txCtx = buildContext(db, isProd);
            const downFn = fileMig.content.down;
            if (!downFn) throw new Error(`Migration ${fileMig.file} missing down function`);
            downFn(txCtx);

            durationMs = Date.now() - migStart;
            db.prepare("DELETE FROM _migrations WHERE version = ?").run(version);
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
  
  // Ensure migrations table exists
  database.exec(MIGRATIONS_TABLE);
  
  const appliedRows = database.prepare(
    "SELECT version, name, checksum FROM _migrations ORDER BY version ASC"
  ).all() || [];
  
  const fileMigrations = await loadMigrationsFromFS();
  // First-match semantics (matching loadMigrationsFromFS ordering) so duplicate
  // version numbers resolve deterministically the same way the runner applies them.
  const fileMigrationMap = new Map<number, ParsedMigration>();
  for (const m of fileMigrations) {
    if (!fileMigrationMap.has(m.version)) fileMigrationMap.set(m.version, m);
  }
  
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

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
    meta TEXT DEFAULT '{}',
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
  for (const [version, count] of versionCounts) {
    if (count > 1) {
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

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
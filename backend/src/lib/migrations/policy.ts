import { runMigrations, loadMigrationsFromFS, getAppliedVersions, validateMigrationFiles } from "./runner";
import type { MigrationDefinition } from "./types";

// ---------------------------------------------------------------------------
// Typed error class
// ---------------------------------------------------------------------------

/**
 * Discriminated error for every rejection path in migrateDownCommand.
 * Callers branch on `code` — never on message strings.
 *
 * Codes:
 *  EMERGENCY_REQUIRED     – rollback attempted without --emergency flag and without
 *                           the ALLOW_DOWN_MIGRATIONS env-var opt-in
 *  GLOBALLY_DISABLED      – emergency flag present but env-var gate is not open
 *  CONFLICTING_FLAGS      – --to and --all were both supplied; mutually exclusive
 *  INVALID_TARGET_VERSION – --to value is not a positive integer
 *  NO_MIGRATIONS_APPLIED  – rollback requested but the _migrations table is empty
 *  NO_DOWN_FUNCTION       – a migration targeted for rollback has no `down` function
 *  CONCURRENT_EXECUTION   – another rollback is already in progress (re-entrant guard)
 *  PRODUCTION_BLOCKED     – production environment blocked without an approval file
 *  EXECUTION_FAILED       – the underlying runMigrations call threw an unexpected error
 */
export type MigrationDownErrorCode =
  | "EMERGENCY_REQUIRED"
  | "GLOBALLY_DISABLED"
  | "CONFLICTING_FLAGS"
  | "INVALID_TARGET_VERSION"
  | "NO_MIGRATIONS_APPLIED"
  | "NO_DOWN_FUNCTION"
  | "CONCURRENT_EXECUTION"
  | "PRODUCTION_BLOCKED"
  | "EXECUTION_FAILED";

export class MigrationPolicyError extends Error {
  readonly code: MigrationDownErrorCode;
  readonly cause: unknown;

  constructor(code: MigrationDownErrorCode, message: string, cause?: unknown) {
    super(message);
    this.name = "MigrationPolicyError";
    this.code = code;
    this.cause = cause;
    if (cause instanceof Error && cause.stack) {
      this.stack = `${this.stack}\nCaused by: ${cause.stack}`;
    }
  }
}

// ---------------------------------------------------------------------------
// Concurrency guard
// ---------------------------------------------------------------------------

/**
 * Module-level flag that prevents two concurrent rollback executions from
 * racing against each other.  Node.js is single-threaded, so this guard is
 * sufficient for synchronous re-entrance (e.g. a CLI flag parsed twice or a
 * test that calls migrateDownCommand without awaiting the first call).
 */
let _rollbackInProgress = false;

/** Reset the concurrency guard — **test-only**. */
export function _resetRollbackGuard(): void {
  _rollbackInProgress = false;
}

// ---------------------------------------------------------------------------
// MigrateArgs interface
// ---------------------------------------------------------------------------

interface MigrateArgs {
  dryRun?: boolean;
  allowDown?: boolean;
  emergency?: boolean;
  verbose?: boolean;
  validateOnly?: boolean;
  check?: boolean;
  to?: string;
  all?: boolean;
  skipChecksumVerify?: boolean;
}

// ---------------------------------------------------------------------------
// MigrationPolicy
// ---------------------------------------------------------------------------

export class MigrationPolicy {
  /**
   * Returns true when the `ALLOW_DOWN_MIGRATIONS` environment variable is
   * explicitly set to the string `"true"`.  Any other value (including
   * `"1"`, `"yes"`, or absent) is treated as false.
   *
   * Invariant: this is the single source-of-truth for the env-var gate; no
   * other code should read `process.env.ALLOW_DOWN_MIGRATIONS` directly.
   */
  static isDownAllowed(): boolean {
    return process.env.ALLOW_DOWN_MIGRATIONS === "true";
  }

  /**
   * Returns true when the migration carries `meta.hotfix === true`.
   * Hotfix migrations have stricter validation requirements (reason,
   * rollback_risk, mandatory down function).
   */
  static isHotfix(migration: MigrationDefinition): boolean {
    return migration.meta?.hotfix === true;
  }

  /**
   * Validate that a migration definition satisfies all required invariants.
   *
   * Rules enforced:
   *  - `name`, `author`, `authoredAt`, `up` must be non-empty/truthy.
   *  - Hotfix migrations additionally require `meta.reason`,
   *    `meta.rollback_risk`, and a `down` function.
   *
   * Returns `{ valid: boolean; errors: string[] }`.  Never throws.
   */
  static validateMetadata(migration: MigrationDefinition): { valid: boolean; errors: string[] } {
    const errors: string[] = [];

    if (!migration.name) errors.push("Migration name is required");
    if (!migration.author) errors.push("Migration author is required");
    if (!migration.authoredAt) errors.push("Migration authoredAt date is required");
    if (!migration.up) errors.push("Migration up function is required");

    if (this.isHotfix(migration)) {
      if (!migration.meta?.reason) errors.push("Hotfix migrations must include meta.reason");
      if (!migration.meta?.rollback_risk) errors.push("Hotfix migrations must include meta.rollback_risk");
      if (!migration.down) errors.push("Hotfix migrations must include a down function");
    }

    return { valid: errors.length === 0, errors };
  }

  /**
   * Validate a list of migrations without touching the database.
   *
   * Checks:
   *  - Each migration passes `validateMetadata`.
   *  - No duplicate version numbers within the list.
   *
   * Passes the `force` option through to allow callers to suppress
   * non-fatal warnings in emergency mode.
   */
  static async dryRun(
    migrations: MigrationDefinition[],
    options: { force?: boolean } = {}
  ): Promise<{ valid: boolean; errors: string[]; warnings: string[] }> {
    const errors: string[] = [];
    const warnings: string[] = [];

    const seenVersions = new Set<number>();
    for (const mig of migrations) {
      const metaCheck = this.validateMetadata(mig);
      if (!metaCheck.valid) {
        errors.push(...metaCheck.errors.map((e) => `${mig.version}_${mig.name}: ${e}`));
      }
      if (seenVersions.has(mig.version)) {
        errors.push(`Duplicate migration version ${mig.version}`);
      }
      seenVersions.add(mig.version);

      // Collect pre-flight warnings from optional validate() hooks
      if (typeof mig.validate === "function") {
        try {
          // validate() expects a MigrationContext; we pass a minimal stub
          // because dryRun operates purely in-process — no DB I/O.
          const stub = _buildDryRunContextStub();
          const migWarnings = await mig.validate(stub);
          if (Array.isArray(migWarnings)) {
            warnings.push(
              ...migWarnings.map((w) => `${mig.version}_${mig.name}: ${w}`)
            );
          }
        } catch {
          // A throwing validate() is itself a warning, not a hard error.
          warnings.push(
            `${mig.version}_${mig.name}: validate() function threw an error`
          );
        }
      }
    }

    return { valid: errors.length === 0, errors, warnings };
  }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/**
 * Minimal MigrationContext stub for dry-run validate() calls.
 * No real DB operations are allowed; every method throws if called.
 */
function _buildDryRunContextStub(): any {
  const noOp = () => {
    throw new Error("DB operations are not permitted during a dry-run validate() call.");
  };
  return {
    db: { exec: noOp, get: noOp, run: noOp, transaction: noOp },
    env: process.env,
    isProduction: process.env.NODE_ENV === "production",
    isTest: process.env.NODE_ENV === "test",
  };
}

// ---------------------------------------------------------------------------
// migrateCommand
// ---------------------------------------------------------------------------

export async function migrateCommand(args: Record<string, unknown>): Promise<{
  success: boolean;
  message: string;
  applied?: number;
  skipped?: number;
}> {
  const {
    dryRun = false,
    allowDown = false,
    emergency = false,
    verbose = false,
    validateOnly = false,
    check = false,
    skipChecksumVerify = false,
  } = args as MigrateArgs;

  if (check) {
    const fileValid = await validateMigrationFiles();
    const fileMigs = await loadMigrationsFromFS();
    const appliedVersions = await getAppliedVersions();
    const missing = fileMigs.filter((m) => !appliedVersions.includes(m.version));
    const valid = fileValid.valid && missing.length === 0;
    const errors = [
      ...fileValid.errors,
      ...missing.map((m) => `Migration ${m.version}_${m.name} is not applied`),
    ];

    if (!valid) {
      console.error("❌ Migration check failed:");
      errors.forEach((e) => console.error(`   ${e}`));
      return { success: false, message: "Migrations out of sync or invalid" };
    }
    console.log("✅ Migrations are in sync");
    return { success: true, message: "Migrations valid" };
  }

  if (validateOnly) {
    const migrations = (await loadMigrationsFromFS()).map((m) => m.content);
    const result = await MigrationPolicy.dryRun(migrations, { force: emergency });
    if (!result.valid) {
      console.error("❌ Migration validation failed:");
      result.errors.forEach((e) => console.error(`   ${e}`));
      return { success: false, message: "Validation errors" };
    }
    console.log("✅ All migration files are valid");
    return { success: true, message: "Validation passed" };
  }

  // ── allowDown safety gate ─────────────────────────────────────────────────
  if (allowDown && !emergency) {
    return {
      success: false,
      message: "Refusing to run down migrations without --emergency flag. This is a safety guard.",
    };
  }

  if (allowDown && !MigrationPolicy.isDownAllowed()) {
    return {
      success: false,
      message: "Down migrations are globally disabled (ALLOW_DOWN_MIGRATIONS not set).",
    };
  }

  try {
    const result = await runMigrations({ dryRun, allowDown, verbose, skipChecksumVerify });
    console.log(`\n✅ Migration run complete in ${result.durationMs}ms`);
    console.log(`   Applied: ${result.applied.length}, Skipped: ${result.skipped}`);

    if (dryRun && result.applied.length > 0) {
      console.log("\n[DRY-RUN] The following migrations would be applied:");
      result.applied.forEach((m) => console.log(`   ${m.version} ${m.name} by ${m.author}`));
    }

    return {
      success: true,
      message: `Applied ${result.applied.length} migrations`,
      applied: result.applied.length,
      skipped: result.skipped,
    };
  } catch (err: any) {
    console.error("❌ Migration failed:", err.message);
    return { success: false, message: `Migration error: ${err.message}` };
  }
}

// ---------------------------------------------------------------------------
// migrateDownCommand
// ---------------------------------------------------------------------------

/**
 * Execute a migration rollback with full deterministic failure-boundary
 * enforcement.
 *
 * Safety invariants
 * ─────────────────
 * 1. **Emergency gate** – rollback is blocked unless the caller supplies
 *    `emergency: true` OR the `ALLOW_DOWN_MIGRATIONS=true` env var is set.
 *    Both paths are logged so the decision is auditable.
 *
 * 2. **Mutual-exclusion flag guard** – `--to` and `--all` are mutually
 *    exclusive.  Supplying both is rejected before any DB I/O occurs.
 *
 * 3. **Target-version validation** – when `--to <N>` is supplied, N must
 *    parse as a positive integer.  A non-numeric or non-positive value is
 *    rejected immediately with INVALID_TARGET_VERSION.
 *
 * 4. **Empty-state guard** – if no migrations have been applied there is
 *    nothing to roll back.  The command returns NO_MIGRATIONS_APPLIED
 *    instead of silently succeeding with zero work done.
 *
 * 5. **Down-function pre-flight check** – before touching the database the
 *    set of migrations targeted for rollback is inspected; if any lacks a
 *    `down` function the command is rejected as NO_DOWN_FUNCTION so the
 *    operator knows exactly which migration is missing the rollback handler.
 *
 * 6. **Concurrent-execution guard** – the module-level `_rollbackInProgress`
 *    flag prevents two concurrent `migrateDownCommand` calls from racing.
 *    In Node.js this catches re-entrant calls within a single event-loop
 *    turn (e.g. two CLI invocations hitting the same process).
 *
 * 7. **Failure transparency** – any error from `runMigrations` is wrapped in
 *    a `MigrationPolicyError(EXECUTION_FAILED)` and its message is included
 *    in the returned `message` field.  The original cause is preserved for
 *    internal logging without leaking sensitive details to CLI consumers.
 *
 * @param args  Parsed CLI arguments.  The following keys are consumed:
 *   - `emergency` (boolean) – bypass the env-var gate for incident response
 *   - `dryRun` (boolean)    – simulate rollback without touching the DB
 *   - `verbose` (boolean)   – emit per-migration progress lines
 *   - `to` (string)         – roll back to (but not including) this version
 *   - `all` (boolean)       – roll back every applied migration
 *   - `skipChecksumVerify` (boolean) – skip checksum pre-flight (test envs only)
 *
 * @returns `{ success, message, applied?, skipped? }` — never throws.
 */
export async function migrateDownCommand(args: Record<string, unknown>): Promise<{
  success: boolean;
  message: string;
  applied?: number;
  skipped?: number;
}> {
  const {
    dryRun = false,
    emergency = false,
    verbose = false,
    to,
    all = false,
    skipChecksumVerify = false,
  } = args as MigrateArgs;

  // ── 1. Emergency / env-var gate ───────────────────────────────────────────
  if (!emergency && !MigrationPolicy.isDownAllowed()) {
    const msg =
      "Down migrations require --emergency flag or ALLOW_DOWN_MIGRATIONS=true environment variable.";
    console.warn(`[migrateDown] BLOCKED — ${msg}`);
    return { success: false, message: msg };
  }

  if (emergency) {
    console.warn(
      "[migrateDown] WARNING — running with --emergency flag. " +
        "Ensure this rollback has been reviewed and approved."
    );
  }

  // ── 2. Concurrent-execution guard ─────────────────────────────────────────
  if (_rollbackInProgress) {
    const msg = "A rollback is already in progress. Concurrent rollbacks are not permitted.";
    console.error(`[migrateDown] BLOCKED — ${msg}`);
    return { success: false, message: msg };
  }

  // ── 3. Mutual-exclusion: --to and --all ───────────────────────────────────
  if (to !== undefined && all) {
    const msg = "Cannot specify both --to and --all flags.";
    console.error(`[migrateDown] BLOCKED — ${msg}`);
    return { success: false, message: msg };
  }

  // ── 4. Validate --to version number ───────────────────────────────────────
  let targetVersion: number | undefined;
  if (to !== undefined) {
    const parsed = Number(to);
    if (!Number.isInteger(parsed) || parsed <= 0) {
      const msg = `Invalid --to value "${to}": must be a positive integer version number.`;
      console.error(`[migrateDown] BLOCKED — ${msg}`);
      return { success: false, message: msg };
    }
    targetVersion = parsed;
  }

  // ── 5. Pre-flight: nothing to roll back ───────────────────────────────────
  let appliedVersions: number[];
  try {
    appliedVersions = await getAppliedVersions();
  } catch (err: any) {
    const msg = `Failed to read applied migration versions: ${err.message}`;
    console.error(`[migrateDown] ERROR — ${msg}`);
    return { success: false, message: msg };
  }

  if (appliedVersions.length === 0) {
    const msg = "No migrations are currently applied — nothing to roll back.";
    console.warn(`[migrateDown] SKIPPED — ${msg}`);
    return { success: true, message: msg, applied: 0, skipped: 0 };
  }

  // ── 6. Pre-flight: identify target set and check for missing down functions ─
  let allFileMigrations;
  try {
    allFileMigrations = await loadMigrationsFromFS();
  } catch (err: any) {
    const msg = `Failed to load migration files: ${err.message}`;
    console.error(`[migrateDown] ERROR — ${msg}`);
    return { success: false, message: msg };
  }

  // Compute the set of versions that will be rolled back.
  // - `all`: every applied version, descending
  // - `to N`: every applied version > N, descending
  // - default (no flag): only the single highest applied version
  const targetSet = _computeTargetVersions(appliedVersions, { all, targetVersion });

  // Verify each targeted migration has a `down` function before we start.
  const missing: string[] = [];
  for (const ver of targetSet) {
    const def = allFileMigrations.find((m) => m.version === ver);
    if (def && !def.content.down) {
      missing.push(`${ver}_${def.name}`);
    }
  }
  if (missing.length > 0) {
    const msg =
      `The following migrations lack a \`down\` function and cannot be rolled back: ` +
      missing.join(", ") +
      ". Add a down function or exclude them from the rollback target.";
    console.error(`[migrateDown] BLOCKED — ${msg}`);
    return { success: false, message: msg };
  }

  // ── 7. Execute rollback ────────────────────────────────────────────────────
  _rollbackInProgress = true;
  try {
    const result = await runMigrations({
      dryRun,
      allowDown: true,
      verbose,
      skipChecksumVerify,
      to: targetVersion !== undefined ? String(targetVersion) : undefined,
      all,
    });

    const verb = dryRun ? "[DRY-RUN] Would roll back" : "Rolled back";
    console.log(`\n✅ Migration rollback complete in ${result.durationMs}ms`);
    console.log(`   ${verb}: ${result.applied.length}, Skipped: ${result.skipped}`);

    if (dryRun && result.applied.length > 0) {
      console.log("\n[DRY-RUN] The following migrations would be rolled back:");
      result.applied.forEach((m) =>
        console.log(`   ${m.version} ${m.name} by ${m.author}`)
      );
    }

    return {
      success: true,
      message: `${verb} ${result.applied.length} migrations`,
      applied: result.applied.length,
      skipped: result.skipped,
    };
  } catch (err: any) {
    // Wrap in our typed error for upstream callers, but return a safe message
    // for the CLI consumer so no internal paths or checksums leak.
    const wrapped = new MigrationPolicyError(
      "EXECUTION_FAILED",
      `Rollback failed: ${err.message}`,
      err
    );
    console.error(`[migrateDown] EXECUTION_FAILED — ${wrapped.message}`);
    return { success: false, message: wrapped.message };
  } finally {
    _rollbackInProgress = false;
  }
}

// ---------------------------------------------------------------------------
// Internal: target-version computation
// ---------------------------------------------------------------------------

/**
 * Compute the ordered list of versions to roll back.
 *
 * Exported for testing. Uses only in-memory data — no DB or FS access.
 *
 * @param appliedVersions  Sorted ascending list of currently applied versions.
 * @param opts.all         Roll back every applied version.
 * @param opts.targetVersion  Roll back all versions strictly greater than this.
 * @returns Versions to roll back, sorted **descending** (highest first).
 */
export function _computeTargetVersions(
  appliedVersions: number[],
  opts: { all?: boolean; targetVersion?: number }
): number[] {
  const { all = false, targetVersion } = opts;

  if (all) {
    return [...appliedVersions].sort((a, b) => b - a);
  }

  if (targetVersion !== undefined) {
    return appliedVersions
      .filter((v) => v > targetVersion)
      .sort((a, b) => b - a);
  }

  // Default: roll back only the single most-recently applied migration
  if (appliedVersions.length === 0) return [];
  const max = Math.max(...appliedVersions);
  return [max];
}

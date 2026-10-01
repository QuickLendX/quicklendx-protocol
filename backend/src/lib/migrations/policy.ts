import { runMigrations, loadMigrationsFromFS, getAppliedVersions, validateMigrationFiles } from "./runner";
import type { MigrationDefinition } from "./types";

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

/**
 * Error class for deterministic migration policy failures.
 * All failure boundaries throw this so callers can distinguish policy rejections
 * from runtime errors and from actual migration execution failures.
 */
export class MigrationPolicyError extends Error {
  readonly code: string;
  readonly details?: Record<string, unknown>;

  constructor(code: string, message: string, details?: Record<string, unknown>) {
    super(message);
    this.name = "MigrationPolicyError";
    this.code = code;
    this.details = details;
  }
}

/**
 * Normalized outcome of a migration command. The command functions never throw
 * for expected failure boundaries (validation, permission, conflicting flags, etc.);
 * they return a deterministic result instead. Unlearned exceptions from the runner
 * are captured and surfaced as `success: false` with a `code`.
 */
export interface MigrationCommandResult {
  success: boolean;
  message: string;
  applied?: number;
  skipped?: number;
  code?: string;
  errors?: string[];
  warnings?: string[];
}

const DEFAULT_CODE = "MIGRATION_POLICY_FAILURE";

function toErrorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  try {
    return JSON.stringify(err);
  } catch {
    return String(err);
  }
}

function toErrorCode(err: unknown): string {
  if (err && typeof err === "object" && "code" in err && typeof (err as { code?: unknown }).code === "string") {
    return (err as { code: string }).code;
  }
  return DEFAULT_CODE;
}

export class MigrationPolicy {
  static isDownAllowed(): boolean {
    return process.env.ALLOW_DOWN_MIGRATIONS === "true";
  }

  static isHotfix(migration: MigrationDefinition): boolean {
    return migration.meta?.hotfix === true;
  }

  /**
   * Validate a single migration definition.
   *
   * Invariants:
   *  - Never throws for malformed input; returns a deterministic error list.
   *  - Error ordering is stable so tests and operators can rely on it.
   *  - Hotfix metadata requirements are enforced at this boundary.
   */
  static validateMetadata(migration: MigrationDefinition): { valid: boolean; errors: string[] } {
    const errors: string[] = [];

    if (!migration || typeof migration !== "object") {
      return { valid: false, errors: ["Migration definition is required"] };
    }

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
   * Dry-run a set of migrations without touching the database.
   *
   * Invariants:
   *  - Never throws for invalid input; returns a deterministic report.
   *  - Duplicate versions are reported exactly once per duplicate occurrence.
   *  - Error ordering follows input order for repeatability.
   */
  static async dryRun(migrations: MigrationDefinition[], options: { force?: boolean } = {}): Promise<{
    valid: boolean;
    errors: string[];
    warnings: string[];
  }> {
    const errors: string[] = [];
    const warnings: string[] = [];

    if (!Array.isArray(migrations)) {
      return { valid: false, errors: ["Migrations must be an array"], warnings };
    }

    const seenVersions = new Set<number>();
    for (const mig of migrations) {
      const label = mig && typeof mig === "object" ? `${(mig as MigrationDefinition).version}_${(mig as MigrationDefinition).name}` : "<unknown>";
      const metaCheck = this.validateMetadata(mig);
      if (!metaCheck.valid) {
        errors.push(...metaCheck.errors.map((e) => `${label}: ${e}`));
      }
      if (mig && typeof mig === "object" && typeof mig.version === "number") {
        if (seenVersions.has(mig.version)) {
          errors.push(`Duplicate migration version ${mig.version}`);
        }
        seenVersions.add(mig.version);
      } else {
        errors.push(`${label}: Migration version is required and must be a number`);
      }
    }

    return { valid: errors.length === 0, errors, warnings };
  }
}

// ─── migrateCommand failure boundaries (#2685) ─────────────────────────────
//
// Invariants enforced below:
//  1. Flags are explicit booleans. `cli.ts` turns `--dry-run` into the key
//     `dryrun`, which the old destructuring (`dryRun`) never read — so
//     `npm run migrate -- --dry-run` silently APPLIED migrations. Keys are now
//     matched case-, dash- and underscore-insensitively, and unknown keys or
//     non-boolean values (e.g. the string "false", which is truthy) are
//     rejected before anything touches the database.
//  2. Mutually exclusive modes (check / validate-only / allow-down) are
//     rejected instead of one silently winning.
//  3. One migration command at a time per process: a concurrent call returns
//     BUSY instead of racing the runner. The slot is always released.
//  4. migrateCommand never rejects: every failure — including errors while
//     loading files or reading applied state — resolves to
//     `{ success: false, code, message }`.
//  5. Partial failure is explicit: the runner commits each migration in its
//     own transaction, so a failure can leave earlier migrations applied. The
//     result reports how many were committed; re-running resumes from the
//     next pending migration (applied versions are skipped).
//  6. Failure messages and logs redact credentials (URL userinfo,
//     password/secret/token/api-key values) and never echo flag values.

/** Stable reason for a failed `migrateCommand` / `migrateDownCommand` result. */
//
// Codes shared with the upstream implementation (#2684/#2832) keep upstream's
// names: DOWN_REQUIRES_EMERGENCY, DOWN_GLOBALLY_DISABLED, DOWN_NOT_ALLOWED,
// CONFLICTING_FLAGS, MIGRATION_OUT_OF_SYNC, MIGRATION_VALIDATION_FAILED.
export type MigrateFailureCode =
  | "INVALID_ARGS"
  | "CONFLICTING_FLAGS"
  | "BUSY"
  | "DOWN_REQUIRES_EMERGENCY"
  | "DOWN_GLOBALLY_DISABLED"
  | "DOWN_NOT_ALLOWED"
  | "MIGRATION_OUT_OF_SYNC"
  | "MIGRATION_VALIDATION_FAILED"
  | "LOAD_FAILED"
  | "RUN_FAILED";

/** `MigrationCommandResult` narrowed to the stable failure codes above. */
export interface MigrateCommandResult extends MigrationCommandResult {
  /** Set on every failure; absent on success. */
  code?: MigrateFailureCode;
  /** Migrations committed by a failed (non-dry) run before it stopped, when known. */
  appliedBeforeFailure?: number;
}

const MIGRATE_FLAGS = [
  "dryRun",
  "allowDown",
  "emergency",
  "verbose",
  "validateOnly",
  "check",
  "skipChecksumVerify",
] as const;

type MigrateFlag = (typeof MIGRATE_FLAGS)[number];
export type MigrateFlags = Record<MigrateFlag, boolean>;

const normalizeFlagKey = (key: string): string => key.replace(/[-_]/g, "").toLowerCase();
const FLAG_BY_KEY = new Map<string, MigrateFlag>(MIGRATE_FLAGS.map((flag) => [normalizeFlagKey(flag), flag]));
/** Flags that only make sense for `migrate down`. */
const DOWN_ONLY_KEYS = new Set(["to", "all"]);

/** Render a caller-supplied key safely in an error message. */
function displayKey(key: string): string {
  const cleaned = key.replace(/[^A-Za-z0-9_-]/g, "?");
  return cleaned.length > 40 ? `${cleaned.slice(0, 40)}…` : cleaned;
}

/**
 * Parse and validate `migrateCommand` arguments. Keys are matched
 * case/dash/underscore-insensitively (`dryRun`, `dryrun`, `dry-run`, `dry_run`);
 * `_` holds CLI positional arguments and is ignored. Values are never echoed.
 */
export function parseMigrateFlags(
  args: unknown,
): { ok: true; flags: MigrateFlags } | { ok: false; message: string } {
  const flags = Object.fromEntries(MIGRATE_FLAGS.map((flag) => [flag, false])) as MigrateFlags;
  if (args === undefined || args === null) return { ok: true, flags };
  if (typeof args !== "object" || Array.isArray(args)) {
    return { ok: false, message: "Migration arguments must be an object of boolean flags." };
  }

  const seen = new Map<MigrateFlag, boolean>();
  for (const [key, value] of Object.entries(args as Record<string, unknown>)) {
    if (key === "_") continue;
    const normalized = normalizeFlagKey(key);

    if (DOWN_ONLY_KEYS.has(normalized)) {
      return { ok: false, message: `--${displayKey(key)} only applies to "migrate down".` };
    }

    const flag = FLAG_BY_KEY.get(normalized);
    if (!flag) {
      return {
        ok: false,
        message: `Unknown flag --${displayKey(key)}. Allowed: ${MIGRATE_FLAGS.join(", ")}.`,
      };
    }
    if (value === undefined) continue;
    if (typeof value !== "boolean") {
      return { ok: false, message: `Flag ${flag} must be a boolean (got ${typeof value}).` };
    }
    if (seen.has(flag) && seen.get(flag) !== value) {
      return { ok: false, message: `Flag ${flag} was given more than once with different values.` };
    }
    seen.set(flag, value);
    flags[flag] = value;
  }

  return { ok: true, flags };
}

/** Strip credentials from text before it is logged or returned. */
export function redactSensitive(text: string): string {
  return text
    .replace(/([a-z][a-z0-9+.-]*:\/\/)[^\s/@]+@/gi, "$1***@")
    .replace(
      /\b(password|passwd|pwd|secret|token|api[_-]?key)(\s*[=:]\s*)("[^"]*"|'[^']*'|[^\s,;]+)/gi,
      "$1$2***",
    );
}

/** A redacted, never-empty message for any thrown value. */
export function describeFailure(err: unknown): string {
  let message = "";
  if (err instanceof Error) message = err.message;
  else if (typeof err === "string") message = err;
  return redactSensitive(message.trim() || "Unknown error");
}

function normalizeArgs(value: unknown): MigrateArgs {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return {};
  }
  return value as MigrateArgs;
}

let activeMigrationCommand: string | null = null;

/** Run `fn` while holding the single per-process migration slot. */
async function withMigrationLock(
  command: string,
  fn: () => Promise<MigrateCommandResult>,
): Promise<MigrateCommandResult> {
  // Checked and taken synchronously, before any await, so two calls started
  // in the same tick cannot both acquire it.
  if (activeMigrationCommand !== null) {
    return {
      success: false,
      code: "BUSY",
      message: `Another migration command ("${activeMigrationCommand}") is already running in this process; retry when it finishes.`,
    };
  }
  activeMigrationCommand = command;
  try {
    return await fn();
  } finally {
    activeMigrationCommand = null;
  }
}

/** Applied-version count, or null when it cannot be read (e.g. no _migrations table yet). */
async function countAppliedMigrations(): Promise<number | null> {
  try {
    return (await getAppliedVersions()).length;
  } catch {
    return null;
  }
}

export async function migrateCommand(args: Record<string, unknown>): Promise<MigrateCommandResult> {
  const parsed = parseMigrateFlags(args);
  if (!parsed.ok) {
    console.error(`❌ ${parsed.message}`);
    return { success: false, code: "INVALID_ARGS", message: parsed.message };
  }
  const { dryRun, allowDown, emergency, verbose, validateOnly, check, skipChecksumVerify } = parsed.flags;

  if (check && validateOnly) {
    return {
      success: false,
      code: "CONFLICTING_FLAGS",
      message: "--check and --validate-only cannot be combined; run them separately.",
    };
  }
  if (allowDown && (check || validateOnly)) {
    return {
      success: false,
      code: "CONFLICTING_FLAGS",
      message: "--allow-down cannot be combined with --check or --validate-only.",
    };
  }

  return withMigrationLock("migrate", async () => {
    if (check) {
      try {
        const fileValid = await validateMigrationFiles();
        const fileMigs = await loadMigrationsFromFS();
        const appliedVersions = await getAppliedVersions();
        const fileVersions = new Set(fileMigs.map((m) => m.version));
        const missing = fileMigs.filter((m) => !appliedVersions.includes(m.version));
        // Stale state: the database has migrations this checkout does not know about.
        const unknownApplied = appliedVersions.filter((v) => !fileVersions.has(v));
        const errors = [
          ...fileValid.errors,
          ...missing.map((m) => `Migration ${m.version}_${m.name} is not applied`),
          ...unknownApplied.map(
            (v) => `Applied migration ${v} has no file in this checkout (database is ahead of the code)`,
          ),
        ];

        if (!fileValid.valid || errors.length > 0) {
          console.error("❌ Migration check failed:");
          errors.forEach((e) => console.error(`   ${e}`));
          return { success: false, code: "MIGRATION_OUT_OF_SYNC", message: "Migrations out of sync or invalid", errors };
        }
        console.log("✅ Migrations are in sync");
        return { success: true, message: "Migrations valid" };
      } catch (err) {
        const reason = describeFailure(err);
        console.error("❌ Migration check could not complete:", reason);
        return { success: false, code: "LOAD_FAILED", message: `Migration check error: ${reason}` };
      }
    }

    if (validateOnly) {
      try {
        const migrations = await loadMigrationsFromFS();
        // Validate each file's exported definition. The loader returns
        // { file, version, name, content }; passing those wrappers directly
        // meant author/authoredAt/up were always "missing", so validate-only
        // failed for every valid migration (#2685).
        const definitions = migrations.map((m) => m.content);
        const result = await MigrationPolicy.dryRun(definitions, { force: emergency });
        if (!result.valid) {
          console.error("❌ Migration validation failed:");
          result.errors.forEach((e) => console.error(`   ${e}`));
          return {
            success: false,
            code: "MIGRATION_VALIDATION_FAILED",
            message: "Validation errors",
            errors: result.errors,
            warnings: result.warnings,
          };
        }
        console.log("✅ All migration files are valid");
        return { success: true, message: "Validation passed", warnings: result.warnings };
      } catch (err) {
        const reason = describeFailure(err);
        console.error("❌ Migration validation could not complete:", reason);
        return { success: false, code: "LOAD_FAILED", message: `Migration validation error: ${reason}` };
      }
    }

    if (allowDown && !emergency) {
      return {
        success: false,
        code: "DOWN_REQUIRES_EMERGENCY",
        message: "Refusing to run down migrations without --emergency flag. This is a safety guard.",
      };
    }

    if (allowDown && !MigrationPolicy.isDownAllowed()) {
      return {
        success: false,
        code: "DOWN_GLOBALLY_DISABLED",
        message: "Down migrations are globally disabled (ALLOW_DOWN_MIGRATIONS not set).",
      };
    }

    // Snapshot applied state so a partial failure can be reported precisely.
    const appliedBefore = dryRun ? null : await countAppliedMigrations();

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
    } catch (err) {
      const reason = describeFailure(err);
      console.error("❌ Migration failed:", reason);

      const failure: MigrateCommandResult = {
        success: false,
        code: "RUN_FAILED",
        message: `Migration error: ${reason}`,
      };
      if (!dryRun) {
        const appliedAfter = await countAppliedMigrations();
        if (appliedBefore !== null && appliedAfter !== null) {
          const committed = Math.max(0, appliedAfter - appliedBefore);
          failure.appliedBeforeFailure = committed;
          failure.message +=
            committed > 0
              ? ` (${committed} migration(s) were committed before the failure; re-run to resume from the next pending migration)`
              : " (no migrations were recorded as applied by this run)";
        }
      }
      return failure;
    }
  });
}

export async function migrateDownCommand(args: Record<string, unknown>): Promise<MigrateCommandResult> {
  // Shares the per-process slot with migrateCommand so up and down runs never overlap (#2685).
  return withMigrationLock("migrate down", () => migrateDownCommandUnlocked(args));
}

async function migrateDownCommandUnlocked(args: Record<string, unknown>): Promise<MigrateCommandResult> {
  const {
    dryRun = false,
    emergency = false,
    verbose = false,
    to: rawTo,
    all = false,
    skipChecksumVerify = false,
  } = normalizeArgs(args) as Omit<MigrateArgs, "to"> & { to?: unknown };
  // The CLI parser yields strings, programmatic callers may pass a number.
  const to = typeof rawTo === "number" ? String(rawTo) : typeof rawTo === "string" ? rawTo : undefined;

  if (!emergency && !MigrationPolicy.isDownAllowed()) {
    return {
      success: false,
      message: "Down migrations require --emergency flag or ALLOW_DOWN_MIGRATIONS=true environment variable.",
      code: "DOWN_NOT_ALLOWED",
    };
  }

  if (to && all) {
    return {
      success: false,
      message: "Cannot specify both --to and --all flags.",
      code: "CONFLICTING_FLAGS",
    };
  }

  try {
    const result = await runMigrations({ dryRun, allowDown: true, verbose, skipChecksumVerify, to, all });
    console.log(`\n✅ Migration rollback complete in ${result.durationMs}ms`);
    console.log(`   Rolled back: ${result.applied.length}, Skipped: ${result.skipped}`);

    if (dryRun && result.applied.length > 0) {
      console.log("\n[DRY-RUN] The following migrations would be rolled back:");
      result.applied.forEach((m) => console.log(`   ${m.version} ${m.name} by ${m.author}`));
    }

    return {
      success: true,
      message: `Rolled back ${result.applied.length} migrations`,
      applied: result.applied.length,
      skipped: result.skipped,
    };
  } catch (err) {
    const reason = describeFailure(err);
    console.error("❌ Migration rollback failed:", reason);
    return { success: false, code: "RUN_FAILED", message: `Rollback error: ${reason}` };
  }
}

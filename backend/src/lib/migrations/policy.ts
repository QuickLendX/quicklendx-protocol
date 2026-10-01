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

/**
 * Deterministic failure-boundary coverage for the migration CLI entry point.
 *
 * Invariants enforced here:
 *  - Command arguments are normalized before any side effects (no down without
    emergency + global allowance; no conflicting --to/--all).
 *  - Every failure path returns a structured result and never throws, so callers
 *    can retry deterministically without losing state.
 *  - Error messages are sanitized to avoid leaking secrets or connection
 *    strings to logs.
 */

function sanitizeErrorMessage(err: unknown): string {
  const raw = err instanceof Error ? err.message : typeof err === "string" ? err : "Unknown error";
  // Redact typical credential/connection secrets from error output.
  return raw
    .replace(/(\/\/[^:@]+):[^@]+@/g, "//$1:@@")
    .replace(/(password|pwd|secret|token|api[_\-]?key)\s*=\s*[^\s]+/gi, "$1=[REDACTED]")
    .replace(/(BEARER\s+)[A-Za-z0-9\-\._=]+/g, "$1[REDACTED]");
}

function normalizeArgs(value: unknown): MigrateArgs {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return {};
  }
  return value as MigrateArgs;
}

function toBoolean(value: unknown): boolean {
  return value === true || value === "true" || value === 1;
}

/**
 * Run the forward migration command.
 *
 * Failure boundaries are deterministic and do not mutate state:
 *  - check / validateOnly modes return a result without running migrations.
 *  - Permission guards (down migrations) reject before any runner invocation.
 *  - Runner errors are captured and reported with a stable code for diagnosis.
 */
export async function migrateCommand(args: Record<string, unknown>): Promise<MigrationCommandResult> {
  const {
    dryRun = false,
    allowDown = false,
    emergency = false,
    verbose = false,
    validateOnly = false,
    check = false,
    skipChecksumVerify = false,
  } = normalizeArgs(args);

  if (check) {
try {
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
        return { success: false, message: "Migrations out of sync or invalid", code: "MIGRATION_OUT_OF_SYNC", errors };
      }
      console.log("✅ Migrations are in sync");
      return { success: true, message: "Migrations valid" };
    } catch (err) {
      const message = sanitizeErrorMessage(err);
      console.error("❌ Migration check failed:", message);
      return {
        success: false,
        message: `Migration check error: ${message}`,
        code: toErrorCode(err),
      };
    }
  }

  if (validateOnly) {
try {
      const migrations = (await loadMigrationsFromFS()).map((m) => m.content);
      const result = await MigrationPolicy.dryRun(migrations, { force: emergency });
      if (!result.valid) {
        console.error("❌ Migration validation failed:");
        result.errors.forEach((e) => console.error(`   ${e}`));
        return {
          success: false,
          message: "Validation errors",
          code: "MIGRATION_VALIDATION_FAILED",
          errors: result.errors,
          warnings: result.warnings,
        };
      }
      console.log("✅ All migration files are valid");
      return { success: true, message: "Validation passed", warnings: result.warnings };
    } catch (err) {
      const message = sanitizeErrorMessage(err);
      console.error("❌ Migration validation failed:", message);
      return {
        success: false,
        message: `Migration validation error: ${message}`,
        code: toErrorCode(err),
      };
    }
  }

  if (allowDown && !emergency) {
    return {
      success: false,
      message: "Refusing to run down migrations without --emergency flag. This is a safety guard.",
      code: "DOWN_REQUIRES_EMERGENCY",
    };
  }

  if (allowDown && !MigrationPolicy.isDownAllowed()) {
    return {
      success: false,
      message: "Down migrations are globally disabled (ALLOW_DOWN_MIGRATIONS not set).",
      code: "DOWN_GLOBALLY_DISABLED",
    };
  }

  try {
    const result = await runMigrations({ dryRun, allowDown, verbose, skipChecksumVerify });
    console.log(`\n ✅ Migration run complete in ${result.durationMs}ms`);
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
    const message = sanitizeErrorMessage(err);
    console.error("❌ Migration failed:", message);
    return {
      success: false,
      message: `Migration error: ${message}`,
      code: toErrorCode(err),
    };
  }
}

/**
 * Run the down migration command.
 *
 * Failure boundaries are deterministic and do not mutate state:
 *  - Permission guards reject before any runner invocation.
 *  - Conflicting flags (`--to` + `--all`) reject before any runner invocation.
 *  - Runner errors are captured and reported with a stable code for diagnosis.
 */
export async function migrateDownCommand(args: Record<string, unknown>): Promise<MigrationCommandResult> {
  const {
    dryRun = false,
    emergency = false,
    verbose = false,
    to,
    all = false,
    skipChecksumVerify = false,
  } = normalizeArgs(args);

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
    console.log(`\n ✅ Migration rollback complete in ${result.durationMs}ms`);
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
    const message = sanitizeErrorMessage(err);
    console.error("❌ Migration rollback failed:", message);
    return {
      success: false,
      message: `Rollback error: ${message}`,
      code: toErrorCode(err),
    };
  }
}

/**
 * Migration system types and core interfaces.
 */

/**
 * The context passed to each migration's up/down/validate functions.
 */
export interface MigrationContext {
  /** Database instance with exec, get, run, and transaction methods. */
  db: {
    /** Execute a query and return all results. */
    exec(sql: string, params?: unknown[]): Promise<unknown[]>;
    /** Execute a query and return the first result. */
    get<T = unknown>(sql: string, params?: unknown[]): Promise<T | undefined>;
    /** Execute a query and return the number of changes and last insert row id. */
    run(sql: string, params?: unknown[]): Promise<{ lastInsertRowId: number; changes: number }>;
    /** Run a function in a database transaction. */
    transaction<T>(fn: (db: MigrationContext['db']) => T): T;
  };
  /** Environment variables. */
  env: NodeJS.ProcessEnv;
  /** True if the current environment is production. */
  isProduction: boolean;
  /** True if the current environment is test. */
  isTest: boolean;
}

/**
 * A migration definition.
 */
export interface MigrationDefinition {
  /** Unique version number (Sequence: 1, 2, 3, ...). Must be monotonically increasing. */
  version: number;
  /** Human-readable name (snake_case, no spaces). Used in filenames and logs. */
  name: string;
  /** Timestamp of when this migration was authored (ISO Date). */
  authoredAt: string;
  /** Author identifier (GitHub username or team). */
  author: string;
  /** Rollback function. Required for all migrations except forward-only baseline (v001).
   *  Rollbacks are ONLY for critical production incidents.
   *  Hotfix migrations MUST include a down function.
   */
  down?: (ctx: MigrationContext) => Promise<void>;
  /** Optional: pre-flight validation that runs before `up` in dry-run mode.
   *  Returns list of warnings; non-fatal.
   */
  validate?: (ctx: MigrationContext) => Promise<string[]>;
  /** Forward migration logic.
   *  CRITICAL: Must be idempotent-safe if re-run (runner guarantees single execution per version).
   */
  up: (ctx: MigrationContext) => Promise<void>;
  /** Optional: jq-filterable metadata for hotfix triage.
   *  Example: { "critical": true, "reason": "fix_foreign_key_violation", "rollback_risk": "low" }
   */
  meta?: Record<string, unknown>;
}

/** Parsed migration file content. */
export interface ParsedMigration {
  file: string;
  version: number;
  name: string;
  content: MigrationDefinition;
}

/** Migration runner state (what gets stored in the _migrations table). */
export interface MigrationState {
  appliedAt: string;
  version: number;
  name: string;
  checksum: string;
  durationMs: number;
  author: string;
  meta?: Record<string, unknown>;
}

/** Hotfix flag definitions. */
export const HotfixFlags = {
  CRITICAL: "critical", // Requires two approved signatures before application
  URGENT: "urgent", // Can be applied by senior engineer, documented post-facto
  STANDARD: "standard", // Regular forward-only migration
} as const;

export type HotfixFlag = (typeof HotfixFlags)[keyof typeof HotfixFlags];

/** Migration error codes. */
export const MigrationErrorCodes = {
  MIGRATION_ALREADY_APPLIED: "MIGRATION_ALREADY_APPLIED",
  MIGRATION_MISSING: "MIGRATION_MISSING",
  DOWN_MIGRATION_NOT_ALLOWED: "DOWN_MIGRATION_NOT_ALLOWGED",
  MIGRATION_VALIDATION_FAILED: "MIGRATION_VALIDATION_FAILED",
  MIGRATION_EXECUTION_FAILED: "MIGRATION_EXECUTION_FAILED",
  CHECKSUM_MISMATCH: "CHECKSUM_MISMATCH",
  HOTFIX_REQUIRES_APPROVAL: "HOTFIX_REQUIRES_APPROVAL",
  UNSUPPORTED_IN_PRODUCTION: "UNSUPPORTED_IN_PRODUCTION",
  /** The runner attempted to execute a migration while another runner held the application lock. */
  MIGRATION_LOCK_HELD: "MIGRATION_LOCK_HELD",
  /** The application lock could not be acquired within the configured timeout. */
  MIGRATION_LOCK_TIMEOUT: "MIGRATION_LOCK_TIMEOUT",
  /** The application lock was lost mid-migration (e.g. lease expired or connection dropped). */
  MIGRATION_LOCK_LOST: "MIGRATION_LOCK_LOST",
  /** A migration failed and the rollback attempt also failed. */
  MIGRATION_ROLLBACK_FAILED: "MIGRATION_ROLLBACK_FAILED",
  /** The cli invocation was invalid (bad arguments, missing command, etc.); no database work was attempted. */
  INVALID_INVOCATION: "INVALID_INVOCATION",
} as const;

export type MigrationErrorCode = (typeof MigrationErrorCodes)[keyof typeof MigrationErrorCodes];

/**
 * Structured error thrown by the migration runner and CLI.
 *
 * Every failure path in the migration system must surface a deterministic
 * code so that the CLI can map it to a stable exit code and a non-sensitive
 * message. The code is the contract; the message is for humans.
 */
export interface MigrationErrorOptions {
  /** Machine-readable error code from MigrationErrorCodes. */
  code: MigrationErrorCode;
  /** Optional migration version associated with the failure. */
  version?: number;
  /** Optional migration name associated with the failure. */
  name?: string;
  /** Whether the failure is considered retryable by the CLI. */
  retryable?: boolean;
  /** Underlying cause, preserved for diagnostics but never included in user-facing output. */
  cause?: unknown;
}

export class MigrationError extends Error {
  public readonly code: MigrationErrorCode;
  public readonly version?: number;
  public readonly name?: string;
  public readonly retryable: boolean;

  constructor(message: string, options: MigrationErrorOptions) {
    super(message);
    this.name = "MigrationError";
    this.code = options.code;
    this.version = options.version;
    this.name = options.name;
    this.retryable = options.retryable ?? false;
    if (options.cause !== undefined) {
      // Preserve the cause for stack tracing without exposing it in CLI output.
      (this as Error & { cause?: unknown }).cause = options.cause;
    }
  }
}

/**
 * Result of a CLI invocation. Returned by `main` so the process entry
 * point can map it to an exit code without relying on process globals.
 */
export interface CliResult {
  /** Process exit code the CLI should use. */
  exitCode: number;
  /** Human-readable summary suitable for stderr/stdout. */
  message: string;
  /** Machine-readable error code when the invocation failed. */
  errorCode?: MigrationErrorCode;
  /** Whether the failure is safe to retry without operator intervention. */
  retryable?: boolean;
}

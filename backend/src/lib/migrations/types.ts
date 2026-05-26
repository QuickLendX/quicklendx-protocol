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
  /** Timestamp of when this migration was authored (ISO date). */
  authoredAt: string;
  /** Author identifier (GitHub username or team). */
  author: string;
  /** Optional: explicit rollback function. Omit for forward-only migrations.
   *  Rollbacks are ONLY for critical production incidents.
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
  DOWN_MIGRATION_NOT_ALLOWED: "DOWN_MIGRATION_NOT_ALLOWED",
  MIGRATION_VALIDATION_FAILED: "MIGRATION_VALIDATION_FAILED",
  MIGRATION_EXECUTION_FAILED: "MIGRATION_EXECUTION_FAILED",
  CHECKSUM_MISMATCH: "CHECKSUM_MISMATCH",
  HOTFIX_REQUIRES_APPROVAL: "HOTFIX_REQUIRES_APPROVAL",
  UNSUPPORTED_IN_PRODUCTION: "UNSUPPORTED_IN_PRODUCTION",
} as const;
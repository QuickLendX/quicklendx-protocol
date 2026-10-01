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
  DOWN_MIGRATION_NOT_ALLOWED: "DOWN_MIGRATION_NOT_ALLOWED",
  MIGRATION_VALIDATION_FAILED: "MIGRATION_VALIDATION_FAILED",
  MIGRATION_EXECUTION_FAILED: "MIGRATION_EXECUTION_FAILED",
  CHECKSUM_MISMATCH: "CHECKSUM_MISMATCH",
  HOTFIX_REQUIRES_APPROVAL: "HOTFIX_REQUIRES_APPROVAL",
  UNSUPPORTED_IN_PRODUCTION: "UNSUPPORTED_IN_PRODUCTION",
} as const;

export type MigrationErrorCode = (typeof MigrationErrorCodes)[keyof typeof MigrationErrorCodes];

/*
 * Runtime validation of migration definitions.
 *
 * The interfaces above are erased at build time, so a migration that is loaded
 * from disk with `require()` can violate every one of these contracts without the
 * type system objecting. Everything below exists to turn those violations into a
 * deterministic, machine-readable report instead of an ad-hoc `Error` thrown
 * halfway through a migration run.
 *
 * Invariants:
 * - Validation is pure and total: for any serialisable value, including `null`,
 *   primitives, and prototype-pollution attempts, it performs no I/O, mutates
 *   nothing, and returns a report instead of throwing.
 * - Output is deterministic: issues are returned in a fixed field order for a
 *   single definition, and in input order for a list, so repeated calls with
 *   equal inputs produce deep-equal results.
 * - Messages never echo non-string field values. Migration metadata can be
 *   sourced from files that reach the deploy pipeline, so only bounded,
 *   validated text (field names, enum values, numeric versions) is rendered.
 */

/** Legal migration names: snake_case segments, no spaces, no leading/trailing underscore. */
export const MIGRATION_NAME_PATTERN = /^[a-z0-9]+(?:_[a-z0-9]+)*$/;

/** Upper bound on rendered identifiers, keeping validation output safe to log. */
const MAX_IDENTIFIER_LENGTH = 128;

/** `YYYY-MM-DD`, the date-only form of `authoredAt`. */
const ISO_DATE_PATTERN = /^\d{4}-\d{2}-\d{2}$/;

/** A single validation failure, addressable by code and field. */
export interface MigrationValidationIssue {
  /** Machine-readable code drawn from {@link MigrationErrorCodes}. */
  code: MigrationErrorCode;
  /** Dotted path of the offending field, e.g. `meta.rollback_risk`. */
  field: string;
  /** Human-readable, secret-free explanation. */
  message: string;
}

/** Outcome of validating one definition or a list of definitions. */
export interface MigrationValidationResult {
  /** True only when `issues` is empty. */
  valid: boolean;
  /** Formatted, secret-free issues in deterministic order. */
  errors: string[];
  /** Structured issues in the same order as `errors`. */
  issues: MigrationValidationIssue[];
}

/** Type guard for the {@link HotfixFlag} union. */
export function isHotfixFlag(value: unknown): value is HotfixFlag {
  return typeof value === "string" && (Object.values(HotfixFlags) as string[]).includes(value);
}

/**
 * Type guard for {@link MigrationDefinition}.
 *
 * Narrower than a bare structural check on purpose: it verifies the required
 * scalar fields and that the callable members are functions, but leaves hotfix
 * metadata to {@link validateMigrationDefinition}, which reports per-field
 * issues instead of collapsing them into one boolean.
 */
export function isMigrationDefinition(value: unknown): value is MigrationDefinition {
  if (!isPlainObject(value)) return false;
  const candidate = value as Partial<MigrationDefinition>;
  return (
    isValidVersion(candidate.version) &&
    isValidName(candidate.name) &&
    isValidAuthoredAt(candidate.authoredAt) &&
    isNonEmptyString(candidate.author) &&
    typeof candidate.up === "function" &&
    isOptionalFunction(candidate.down) &&
    isOptionalFunction(candidate.validate) &&
    (candidate.meta === undefined || isPlainObject(candidate.meta))
  );
}

/**
 * Validate a single migration definition against every runtime invariant.
 *
 * Accepts `unknown` so it can guard values arriving from `require()` without a
 * cast at the call site.
 */
export function validateMigrationDefinition(definition: unknown): MigrationValidationResult {
  const issues: MigrationValidationIssue[] = [];

  if (!isPlainObject(definition)) {
    issues.push(
      issue("definition", `Migration definition must be a plain object, received ${describe(definition)}`),
    );
    return toResult(issues);
  }

  const candidate = definition as Partial<MigrationDefinition>;

  // version
  if (typeof candidate.version !== "number") {
    issues.push(issue("version", `Migration version must be a number, received ${describe(candidate.version)}`));
  } else if (!Number.isSafeInteger(candidate.version)) {
    issues.push(
      issue("version", `Migration version must be a safe integer, received ${describeNumber(candidate.version)}`),
    );
  } else if (candidate.version < 1) {
    issues.push(issue("version", `Migration version must be >= 1, received ${candidate.version}`));
  }

  // name
  if (typeof candidate.name !== "string") {
    issues.push(issue("name", `Migration name must be a string, received ${describe(candidate.name)}`));
  } else if (candidate.name.length > MAX_IDENTIFIER_LENGTH) {
    issues.push(issue("name", `Migration name must be at most ${MAX_IDENTIFIER_LENGTH} characters`));
  } else if (!MIGRATION_NAME_PATTERN.test(candidate.name)) {
    issues.push(
      issue(
        "name",
        `Migration name "${candidate.name}" must be snake_case ([a-z0-9_] with no leading or trailing underscore)`,
      ),
    );
  }

  // authoredAt
  if (typeof candidate.authoredAt !== "string") {
    issues.push(
      issue("authoredAt", `Migration authoredAt must be a string, received ${describe(candidate.authoredAt)}`),
    );
  } else if (!isValidAuthoredAt(candidate.authoredAt)) {
    issues.push(
      issue(
        "authoredAt",
        `Migration authoredAt "${candidate.authoredAt}" must be a real ISO 8601 date or timestamp`,
      ),
    );
  }

  // author
  if (!isNonEmptyString(candidate.author)) {
    issues.push(issue("author", "Migration author is required and must be a non-empty string"));
  } else if (candidate.author.length > MAX_IDENTIFIER_LENGTH) {
    issues.push(issue("author", `Migration author must be at most ${MAX_IDENTIFIER_LENGTH} characters`));
  }

  // up is mandatory
  if (typeof candidate.up !== "function") {
    issues.push(issue("up", `Migration up function is required, received ${describe(candidate.up)}`));
  }

  // down / validate are optional, but must be functions when present. `undefined`
  // means "absent"; an explicit `null` is a mistake worth reporting.
  collectOptionalFunctionIssue(issues, "down", candidate.down);
  collectOptionalFunctionIssue(issues, "validate", candidate.validate);

  // meta
  if (candidate.meta !== undefined) {
    if (!isPlainObject(candidate.meta)) {
      issues.push(issue("meta", `Migration meta must be a plain object, received ${describe(candidate.meta)}`));
    } else {
      collectHotfixIssues(issues, candidate.meta, candidate.down);
    }
  }

  return toResult(issues);
}

/**
 * Validate an ordered list of migration definitions.
 *
 * Per-entry issues come first, in input order and each labelled with the
 * entry's index plus `version_name` when that label is derivable, followed by
 * the list-level invariants (duplicate versions, then version ordering).
 * Entries with an unusable `version` are excluded from the list-level checks so
 * a single bad entry cannot produce a cascade of follow-on errors.
 */
export function validateMigrationDefinitions(definitions: unknown): MigrationValidationResult {
  const issues: MigrationValidationIssue[] = [];

  if (!Array.isArray(definitions)) {
    issues.push(issue("definitions", `Migration definitions must be an array, received ${describe(definitions)}`));
    return toResult(issues);
  }

  const versions: number[] = [];
  const versionLabels: string[] = [];
  const seenVersions = new Map<number, number>();

  definitions.forEach((entry, index) => {
    const label = describeEntry(entry, index);
    const entryIssues = validateMigrationDefinition(entry).issues;
    for (const entryIssue of entryIssues) {
      issues.push({ ...entryIssue, field: `${label}.${entryIssue.field}` });
    }

    if (isPlainObject(entry)) {
      const rawVersion = (entry as Partial<MigrationDefinition>).version;
      if (isValidVersion(rawVersion)) {
        versions.push(rawVersion);
        versionLabels.push(label);
        const firstIndex = seenVersions.get(rawVersion);
        if (firstIndex === undefined) {
          seenVersions.set(rawVersion, index);
        } else {
          issues.push(
            issue(
              `${label}.version`,
              `Duplicate migration version ${rawVersion} (first declared at index ${firstIndex})`,
            ),
          );
        }
      }
    }
  });

  // Ordering is checked against the definitions list, so the issue is labelled
  // with the label of the entry that broke the sequence rather than with a
  // position in the filtered version list.
  for (let i = 1; i < versions.length; i++) {
    if (versions[i] < versions[i - 1]) {
      issues.push(
        issue(
          `${versionLabels[i]}.version`,
          `Migration versions must be monotonically increasing, but ${versions[i]} follows ${versions[i - 1]}`,
        ),
      );
    }
  }

  return toResult(issues);
}

/* -------------------------------------------------------------------------- */
/* Internal helpers                                                           */
/* -------------------------------------------------------------------------- */

function issue(field: string, message: string): MigrationValidationIssue {
  return { code: MigrationErrorCodes.MIGRATION_VALIDATION_FAILED, field, message };
}

function toResult(issues: MigrationValidationIssue[]): MigrationValidationResult {
  return {
    valid: issues.length === 0,
    errors: issues.map(
      (entry) => `${MigrationErrorCodes.MIGRATION_VALIDATION_FAILED} ${entry.field}: ${entry.message}`,
    ),
    issues,
  };
}

function collectOptionalFunctionIssue(
  issues: MigrationValidationIssue[],
  field: "down" | "validate",
  value: unknown,
): void {
  if (value === undefined) return;
  if (typeof value !== "function") {
    issues.push(issue(field, `Migration ${field} must be a function when present, received ${describe(value)}`));
  }
}

function collectHotfixIssues(
  issues: MigrationValidationIssue[],
  meta: Record<string, unknown>,
  down: unknown,
): void {
  if (meta.hotfix_flag !== undefined && !isHotfixFlag(meta.hotfix_flag)) {
    issues.push(
      issue(
        "meta.hotfix_flag",
        `meta.hotfix_flag must be one of ${Object.values(HotfixFlags).join(", ")}, ` +
          `received ${describe(meta.hotfix_flag)}`,
      ),
    );
  }

  // Mirrors MigrationPolicy.isHotfix: only a strict boolean `true` opts a
  // migration into the hotfix protocol.
  if (meta.hotfix !== true) return;

  if (!isNonEmptyString(meta.reason)) {
    issues.push(issue("meta.reason", "Hotfix migrations must include a non-empty meta.reason"));
  }
  if (!isNonEmptyString(meta.rollback_risk)) {
    issues.push(issue("meta.rollback_risk", "Hotfix migrations must include a non-empty meta.rollback_risk"));
  }
  // The rollback lives at the top level of the definition, not under `meta`. A
  // non-callable `down` is already reported by the optional-function check, so
  // only the absent case is reported here.
  if (down === undefined) {
    issues.push(issue("down", "Hotfix migrations must include a down function"));
  }
}

/** Plain object with `Object.prototype` or `null` prototype, never an array. */
function isPlainObject(value: unknown): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function isValidVersion(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 1;
}

function isValidName(value: unknown): value is string {
  return (
    typeof value === "string" &&
    value.length > 0 &&
    value.length <= MAX_IDENTIFIER_LENGTH &&
    MIGRATION_NAME_PATTERN.test(value)
  );
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function isOptionalFunction(value: unknown): boolean {
  return value === undefined || typeof value === "function";
}

/**
 * Accepts `YYYY-MM-DD` and full ISO 8601 timestamps, matching the two forms used
 * by the checked-in migrations. Round-trips through `Date` so impossible dates
 * such as `2026-02-30` are rejected rather than silently rolled over.
 */
function isValidAuthoredAt(value: unknown): value is string {
  if (typeof value !== "string" || value.length === 0) return false;
  if (ISO_DATE_PATTERN.test(value)) return isRealCalendarDate(value);
  // Full timestamps must carry an explicit zone designator or `Z`; a bare
  // local-time string is ambiguous and would make ordering non-deterministic.
  if (!/(Z|[+-]\d{2}:?\d{2})$/.test(value)) return false;
  return !Number.isNaN(Date.parse(value));
}

function isRealCalendarDate(value: string): boolean {
  const [year, month, day] = value.split("-").map(Number) as [number, number, number];
  if (month < 1 || month > 12 || day < 1 || day > 31) return false;
  const parsed = new Date(Date.UTC(year, month - 1, day));
  return (
    parsed.getUTCFullYear() === year && parsed.getUTCMonth() === month - 1 && parsed.getUTCDate() === day
  );
}

/** `version_name` label for list entries, falling back to the raw index. */
function describeEntry(entry: unknown, index: number): string {
  const fallback = `definitions[${index}]`;
  if (!isPlainObject(entry)) return fallback;
  const { version, name } = entry as Partial<MigrationDefinition>;
  // Only a well-formed name is embedded, so a field path never contains the
  // malformed value that is itself being reported.
  if (isValidVersion(version) && isValidName(name)) return `${fallback} (${version}_${name})`;
  return fallback;
}

/**
 * Describes a value's type without rendering its contents. Only primitives are
 * named, so untrusted objects are never interpolated into an error message.
 */
function describe(value: unknown): string {
  if (value === null) return "null";
  if (Array.isArray(value)) return "array";
  if (typeof value === "number") return describeNumber(value);
  if (typeof value === "object") return "object";
  return typeof value;
}

function describeNumber(value: number): string {
  if (Number.isNaN(value)) return "NaN";
  if (value === Number.POSITIVE_INFINITY) return "Infinity";
  if (value === Number.NEGATIVE_INFINITY) return "-Infinity";
  return String(value);
}
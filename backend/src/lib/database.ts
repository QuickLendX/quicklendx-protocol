// Updated implementation with deterministic failure₭boundary handling for prepared statements.

import Database from 'better-sqlite3';
import * as self from './database';

// ----- Type Declarations -----
const DatabaseConstructor = Database as any;
/**
 * Discriminated error type for every failure mode getDatabase can encounter.
 * Callers can branch on `code` without parsing message strings, which keeps
 * error handling deterministic and testable.
 *
 * Codes:
 *  - OPEN_FAILED      Database file could not be opened (corrupt, missing dir, etc.)
 *  - PERMISSION_DENIED OS-level permission error on the database file or its directory
 *  - BUSY_TIMEOUT      Database was locked for longer than the busy_timeout
 *  - PRAGMA_FAILED     A post-open configuration pragma failed (leaves no live instance)
 *  - CLOSED            The caller tried to use a connection that has already been closed
 *  - UNKNOWN           Any other failure – the original error is preserved in `cause`
 */
export type DatabaseErrorCode =
  | 'OPEN_FAILED'
  | 'PERMISSION_DENIED'
  | 'BUSY_TIMEOUT'
  | 'PRAGMA_FAILED'
  | 'CLOSED'
  | 'UNKNOWN';

export class DatabaseError extends Error {
  readonly code: DatabaseErrorCode;
  readonly cause: unknown;

  constructor(code: DatabaseErrorCode, message: string, cause?: unknown) {
    super(message);
    this.name = 'DatabaseError';
    this.code = code;
    this.cause = cause;
    // Preserve original stack when wrapping another error
    if (cause instanceof Error && cause.stack) {
      this.stack = `${this.stack}\nCaused by: ${cause.stack}`;
    }
  }
}

// ---------------------------------------------------------------------------
// Observable state
// ---------------------------------------------------------------------------

/**
 * Lifecycle states of the database singleton.
 *
 * - uninitialized: no connection has been opened yet this process lifetime
 * - opening:       constructor and pragma initialization are in progress
 * - open:          connection is live and ready to accept queries
 * - closed:        connection was explicitly closed via closeDatabase()
 * - error:         last open attempt failed; the next call to getDatabase()
 *                  will clear this state and retry
 */
export type DatabaseState = 'uninitialized' | 'opening' | 'open' | 'closed' | 'error';

/** Read-only snapshot of current connection health — safe to expose to monitors. */
export interface DatabaseStatus {
  state: DatabaseState;
  /** ISO-8601 timestamp of the most recent successful open, or null. */
  lastOpenedAt: string | null;
  /** ISO-8601 timestamp of the most recent close, or null. */
  lastClosedAt: string | null;
  /** ISO-8601 timestamp of the most recent error, or null. */
  lastErrorAt: string | null;
  /** Error code from the most recent failed open, or null. */
  lastErrorCode: DatabaseErrorCode | null;
  /** Number of consecutive failed open attempts since the last successful open. */
  consecutiveFailures: number;
}

// ---------------------------------------------------------------------------
// Internal singleton state
// ---------------------------------------------------------------------------

// Type declaration for better-sqlite3 (avoids `any` in call-site casts)
const DatabaseConstructor = Database as unknown as new (
  path: string,
  options?: object
) => InstanceType<typeof Database>;

let dbInstance: InstanceType<typeof Database> | null = null;
let _state: DatabaseState = 'uninitialized';
let _lastOpenedAt: string | null = null;
let _lastClosedAt: string | null = null;
let _lastErrorAt: string | null = null;
let _lastErrorCode: DatabaseErrorCode | null = null;
let _consecutiveFailures = 0;

/** Centralized prepared statement cache: SQL string → compiled statement. */
const statementCache = new Map<string, ReturnType<InstanceType<typeof Database>['prepare']>>();

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/**
 * Custom error hierarchy for deterministic error handling.
 */
export class DatabaseError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'DatabaseError';
  }
}
export class DatabasePrepareError extends DatabaseError {
  constructor(sql: string, original: any) {
    super(`Failed to prepare statement for SQL: ${sql}. ${original?.message ?? ''}`);
    this.name = 'DatabasePrepareError';
  }
}
export class DatabasePermissionError extends DatabaseError {
  constructor(sql: string, original: any) {
    super(`Permission denied while preparing statement for SQL: ${sql}. ${original?.message ?? ''}`);
    this.name = 'DatabasePermissionError';
  }
}
export class DatabaseBusyError extends DatabaseError {
  constructor(sql: string, original: any) {
    super(`Database busy while preparing statement for SQL: ${sql}. ${original?.message ?? ''}`);
    this.name = 'DatabaseBusyError';
  }
}

/**
 * Specific error types for database ping failures with deterministic categorization.
 */
export class DatabasePingError extends DatabaseError {
  public readonly code: string;
  public readonly retryable: boolean;
  public readonly severity: 'warning' | 'error' | 'critical';
  
  constructor(message: string, code: string, retryable: boolean = false, severity: 'warning' | 'error' | 'critical' = 'error') {
    super(message);
    this.name = 'DatabasePingError';
    this.code = code;
    this.retryable = retryable;
    this.severity = severity;
  }
}

export class DatabasePingTimeoutError extends DatabasePingError {
  constructor(timeoutMs: number) {
    super(`Database ping timed out after ${timeoutMs}ms`, 'PING_TIMEOUT', true, 'warning');
    this.name = 'DatabasePingTimeoutError';
  }
}

export class DatabasePingConnectionError extends DatabasePingError {
  constructor(message: string) {
    super(`Database connection failed: ${message}`, 'PING_CONNECTION_FAILED', true, 'error');
    this.name = 'DatabasePingConnectionError';
  }
}

export class DatabasePingPermissionError extends DatabasePingError {
  constructor(message: string) {
    super(`Database ping permission denied: ${message}`, 'PING_PERMISSION_DENIED', false, 'critical');
    this.name = 'DatabasePingPermissionError';
  }
}

export class DatabasePingBusyError extends DatabasePingError {
  constructor(message: string, attempts: number) {
    super(`Database ping failed after ${attempts} attempts: ${message}`, 'PING_BUSY', true, 'warning');
    this.name = 'DatabasePingBusyError';
  }
}

export class DatabasePingCorruptionError extends DatabasePingError {
  constructor(message: string) {
    super(`Database corruption detected: ${message}`, 'PING_CORRUPTION', false, 'critical');
    this.name = 'DatabasePingCorruptionError';
  }
}

/**
 * Database ping result with comprehensive failure information.
 */
export type DatabasePingResult = 
  | { success: true; latencyMs: number; timestamp: number; attempts: number }
  | { success: false; error: DatabasePingError; latencyMs?: number; timestamp: number; attempts: number };

/**
 * Database ping state for deterministic tracking.
 */
export type DatabasePingState = 
  | 'healthy'
  | 'degraded' 
  | 'timeout'
  | 'busy'
  | 'permission_denied'
  | 'connection_failed'
  | 'corrupted'
  | 'unknown';

/**
 * Configuration for database ping behavior.
 */
export interface DatabasePingConfig {
  /** Maximum time to wait for ping response in milliseconds */
  timeoutMs: number;
  /** Maximum number of retry attempts for retryable errors */
  maxRetries: number;
  /** Base delay between retries in milliseconds (will use exponential backoff) */
  baseRetryDelayMs: number;
  /** SQL query to use for ping test */
  pingQuery: string;
  /** Whether to enable metrics collection */
  enableMetrics: boolean;
}

/**
 * Centralized prepared statement cache.
 * Key: SQL string, Value: prepared statement.
 */
function classifyError(err: unknown): DatabaseErrorCode {
  if (!(err instanceof Error)) return 'UNKNOWN';

  const msg = err.message.toLowerCase();
  const code = (err as NodeJS.ErrnoException).code ?? '';

  if (code === 'EACCES' || code === 'EPERM' || msg.includes('permission denied') || msg.includes('access denied')) {
    return 'PERMISSION_DENIED';
  }

  if (
    msg.includes('database is locked') ||
    msg.includes('database table is locked') ||
    msg.includes('sqlite_busy') ||
    msg.includes('sqlite_locked') ||
    code === 'SQLITE_BUSY' ||
    code === 'SQLITE_LOCKED'
  ) {
    return 'BUSY_TIMEOUT';
  }

  return 'OPEN_FAILED';
}


/**
 * Metrics for deterministic observability.
 *
 * Invariant: these counters describe the lifetime of the *current* cache
 * generation. Every code path that empties `statementCache` must reset them,
 * otherwise `getStatementCacheStats()` would report `size: 0` alongside
 * non-zero counters that describe statements which no longer exist.
 */
let cacheHits = 0;
let cacheMisses = 0;
let cacheEvicts = 0;

/**
 * Get a singleton instance of the better–sqlite3 database with sensible pragmas.
 */
export function getDatabase() {
  if (!dbInstance) {
    const db = new DatabaseConstructor(process.env.DATABASE_PATH || '.data/dev.db');

    // Performance pragmas
    db.pragma('journal_mode = WAL');
    db.pragma('synchronous = NORMAL');
    db.pragma('foreign_keys = ON');
    db.pragma('busy_timeout = 5000');

    dbInstance = db;
  }

  // ── Fast path: already open ───────────────────────────────────────────────
  if (dbInstance !== null) {
    return dbInstance;
  }

  // ── Slow path: open a new connection ─────────────────────────────────────
  _state = 'opening';
  let fresh: InstanceType<typeof Database> | null = null;

  try {
    const path = process.env.DATABASE_PATH || '.data/dev.db';
    fresh = new DatabaseConstructor(path);
  } catch (err) {
    _state = 'error';
    _lastErrorAt = new Date().toISOString();
    _consecutiveFailures++;

    const code = classifyError(err);
    _lastErrorCode = code;

    // Never leak OS path details in the publicly visible message
    const safeMsg =
      code === 'PERMISSION_DENIED'
        ? 'Database file permission denied. Verify the process has read/write access to the database path.'
        : code === 'BUSY_TIMEOUT'
          ? 'Database is locked and could not be opened within the busy_timeout window.'
          : 'Failed to open database. Check that the database path exists and is not corrupt.';

    throw new DatabaseError(code, safeMsg, err);
  }

  // Apply configuration pragmas.  If any pragma fails we must not expose a
  // half-configured instance — close and rethrow.
  try {
    applyPragmas(fresh);
  } catch (err) {
    try { fresh.close(); } catch { /* best-effort cleanup */ }
    _state = 'error';
    _lastErrorAt = new Date().toISOString();
    _consecutiveFailures++;
    _lastErrorCode = 'PRAGMA_FAILED';
    throw err; // already a DatabaseError(PRAGMA_FAILED)
  }

  // ── Commit the instance ───────────────────────────────────────────────────
  dbInstance = fresh;
  _state = 'open';
  _lastOpenedAt = new Date().toISOString();
  _consecutiveFailures = 0;
  _lastErrorCode = null;

  return dbInstance;
}

// ---------------------------------------------------------------------------
// Statement cache
// ---------------------------------------------------------------------------

/**
 * Get a prepared statement from the cache, or prepare and cache it if not present.
 * This significantly improves performance by avoiding redundant statement preparation.
 *
 * 1. Cacheâ€‘hit returns the prepared statement after a cheap validation step.
 *    If validation fails due to a stale schema (`SQLITE_SCHEMA`) the entry is evicted
 *    and a fresh preparation is performed.
 * 2. Cache–miss triggers a guarded preparation sequence:
 *    - Concurrency guard ensures only one preparation per SQL string.
 *    - Retry loop (max 3 attempts) handles transient `SQLITE_BUSY` errors.
 *    - Permission checks surface a `DatabasePermissionError` without caching.
 *    - Any other preparation error surfaces a `DatabasePrepareError`.
 *
 * The public signature is unchanged – namely callers receive the prepared statement or
 * a thrown error they can handle deterministically.
 */
let customGetDatabase: (() => any) | null = null;

export function _setGetDatabaseForTesting(fn: (() => any) | null): void {
  customGetDatabase = fn;
}

export function getPreparedStatement(sql: string): any {
  // ----- Cache Hit Path -----
  if (statementCache.has(sql)) {
    cacheHits++;
    const cached = statementCache.get(sql);
    // Never execute the statement to "validate" it: that would run writes and
    // fail for parameterised SQL. better-sqlite3 re-prepares on schema change.
    if (cached && typeof cached.run === 'function') {
      return cached;
    } catch (e: any) {
      if (e.code === 'SQLITE_SCHEMA') {
        statementCache.delete(sql);
        cacheEvicts++;
        // fall through to preparation
      } else {
        // Other execution errors (like missing params) are expected; return the cached statement
        return cached;
      }
    }
    statementCache.delete(sql);
    cacheEvicts++;
  }

  // ----- Cache Miss / Evicted Path -----
  cacheMisses++;
  const maxAttempts = 3;
  for (let attempt = 0; attempt < maxAttempts; attempt++) {
    try {
      const db = exports.getDatabase();
      const stmt = db.prepare(sql);
      // Permission guard – attempt a harmless execution to surface read–only errors.
      try {
        if (stmt.reader) {
          stmt.get();
        }
      } catch (permErr: any) {
        if (permErr.code === 'SQLITE_READONLY') {
          throw new DatabasePermissionError(sql, permErr);
        }
        // ignore other errors here
      }
      statementCache.set(sql, stmt);
      return stmt;
    } catch (err: any) {
      // A permission failure is already a well-typed, diagnosable error. Re-throw
      // it verbatim so callers observe the documented `DatabasePermissionError`
      // instead of it being masked by a generic preparation error.
      if (err instanceof DatabasePermissionError) {
        throw err;
      }
      if (err.code === 'SQLITE_BUSY') {
        if (attempt < maxAttempts - 1) {
          // simple synchronous backâ€‘off
          const delay = 50 * (attempt + 1);
          const start = Date.now();
          while (Date.now() - start < delay) {}
          continue;
        }
        throw new DatabaseBusyError(sql, err);
      }
      if (err instanceof DatabasePermissionError) throw err;
      // Any other error is a preparation failure.
      throw new DatabasePrepareError(sql, err);
    }
  }
  // Should never reach here.
  throw new DatabaseError('Unexpected preparation failure');
}
  

/**
 * Clear the statement cache and metrics â€“ useful for testing or schema changes.
 */
export function clearStatementCache(): void {
  statementCache.clear();
  resetCacheMetrics();
}

/**
 * Reset the counters that describe the current cache generation.
 *
 * Must be called whenever `statementCache` is emptied so that
 * `getStatementCacheStats()` never mixes an empty cache with live counters.
 */
function resetCacheMetrics(): void {
  cacheHits = 0;
  cacheMisses = 0;
  cacheEvicts = 0;
  // Also reset ping metrics when clearing cache
  resetPingMetrics();
}

/**
 * Get comprehensive database ping statistics.
 */
export function getPingStats() {
  return {
    successCount: pingSuccessCount,
    failureCount: pingFailureCount,
    retryCount: pingRetryCount,
    totalAttempts: pingSuccessCount + pingFailureCount,
    successRate: pingSuccessCount + pingFailureCount > 0 
      ? pingSuccessCount / (pingSuccessCount + pingFailureCount) 
      : 0,
    averageLatencyMs: pingSuccessCount > 0 
      ? pingTotalLatencyMs / pingSuccessCount 
      : 0,
    lastState: lastPingState,
    lastPingTimestamp,
    timeSinceLastPing: lastPingTimestamp > 0 
      ? Date.now() - lastPingTimestamp 
      : -1,
    errorBreakdown: { ...pingErrorCounts },
  };
}

/**
 * Get Prometheus-compatible metrics for database ping monitoring.
 * Returns metrics in a format that can be easily integrated with external systems.
 */
export function getPingMetricsForPrometheus(): Array<{name: string, type: 'counter' | 'gauge', value: number, help: string, labels?: Record<string, string>}> {
  const stats = getPingStats();
  
  return [
    {
      name: 'qlx_db_ping_success_total',
      type: 'counter',
      value: stats.successCount,
      help: 'Total successful database ping operations',
    },
    {
      name: 'qlx_db_ping_failure_total',
      type: 'counter',
      value: stats.failureCount,
      help: 'Total failed database ping operations',
    },
    {
      name: 'qlx_db_ping_retry_total',
      type: 'counter',
      value: stats.retryCount,
      help: 'Total database ping retry attempts',
    },
    {
      name: 'qlx_db_ping_success_rate',
      type: 'gauge',
      value: stats.successRate,
      help: 'Database ping success rate (0-1)',
    },
    {
      name: 'qlx_db_ping_avg_latency_ms',
      type: 'gauge',
      value: stats.averageLatencyMs,
      help: 'Average database ping latency in milliseconds',
    },
    {
      name: 'qlx_db_ping_state',
      type: 'gauge',
      value: getStateNumeric(stats.lastState),
      help: 'Current database ping state (0=healthy, 1=degraded, 2=timeout, 3=busy, 4=permission_denied, 5=connection_failed, 6=corrupted, 7=unknown)',
    },
    {
      name: 'qlx_db_ping_time_since_last',
      type: 'gauge',
      value: stats.timeSinceLastPing,
      help: 'Time since last database ping in milliseconds (-1 if never)',
    },
    // Error breakdown metrics
    ...Object.entries(stats.errorBreakdown).map(([errorCode, count]) => ({
      name: 'qlx_db_ping_error_total',
      type: 'counter' as const,
      value: count,
      help: 'Database ping errors by type',
      labels: { error_code: errorCode },
    })),
  ];
}

/**
 * Convert ping state to numeric value for metrics.
 */
function getStateNumeric(state: DatabasePingState): number {
  const stateMap: Record<DatabasePingState, number> = {
    'healthy': 0,
    'degraded': 1,
    'timeout': 2,
    'busy': 3,
    'permission_denied': 4,
    'connection_failed': 5,
    'corrupted': 6,
    'unknown': 7,
  };
  return stateMap[state] ?? 7;
}

/**
 * Get current database ping state.
 */
export function getPingState(): DatabasePingState {
  return lastPingState;
}

/**
 * Reset ping metrics (useful for testing).
 */
export function resetPingMetrics(): void {
  pingSuccessCount = 0;
  pingFailureCount = 0;
  pingRetryCount = 0;
  pingTotalLatencyMs = 0;
  lastPingState = 'unknown';
  lastPingTimestamp = 0;
  
  // Reset error counts
  for (const key in pingErrorCounts) {
    pingErrorCounts[key] = 0;
  }
}

/**
 * Shape returned by {@link getStatementCacheStats}.
 *
 * Returned snapshots are defensive copies: mutating `statements` cannot
 * corrupt the cache, and each call is a self-consistent point-in-time view.
 */
export interface StatementCacheStats {
  /** Number of cached prepared statements. */
  size: number;
  /** SQL strings currently cached, in insertion order. */
  statements: string[];
  /** Cache hits recorded for the current cache generation. */
  hits: number;
  /** Cache misses recorded for the current cache generation. */
  misses: number;
  /** Entries evicted due to `SQLITE_SCHEMA` in the current generation. */
  evicts: number;
}

/**
 * Retrieve cache statistics including deterministic metrics.
 *
 * Deterministic guarantees:
 * - `statements` is a fresh array; callers cannot mutate internal state.
 * - Repeated calls without intervening cache activity return deep-equal values.
 * - Counters are non-negative integers and reset with the cache generation.
 */
export function getStatementCacheStats(): StatementCacheStats {
  return {
    size: statementCache.size,
    statements: Array.from(statementCache.keys()),
    hits: cacheHits,
    misses: cacheMisses,
    evicts: cacheEvicts,
  };
}

// ---------------------------------------------------------------------------
// Health / observability
// ---------------------------------------------------------------------------

/**
 * Enhanced database ping with deterministic failure boundaries, retry logic, and comprehensive error handling.
 * 
 * This implementation provides:
 * 1. Timeout handling with configurable limits
 * 2. Retry logic with exponential backoff for retryable errors
 * 3. Deterministic error classification with specific error types
 * 4. State tracking for observability
 * 5. Metrics collection for monitoring
 * 6. Backward compatibility through overloaded signatures
 * 
 * Failure modes handled:
 * - Database busy (retryable with backoff)
 * - Connection failures (retryable)
 * - Timeout errors (retryable with shorter timeout)
 * - Permission errors (non-retryable)
 * - Corruption errors (non-retryable, critical)
 * 
 * @param config Optional configuration for ping behavior
 * @returns DatabasePingResult with success/failure details and metrics
 */
export async function pingDatabaseDetailed(config: Partial<DatabasePingConfig> = {}): Promise<DatabasePingResult> {
  const finalConfig = { ...DEFAULT_PING_CONFIG, ...config };
  const startTime = Date.now();
  let attempts = 0;
  let lastError: DatabasePingError | undefined;

  for (let attempt = 0; attempt < finalConfig.maxRetries + 1; attempt++) {
    attempts++;
    
    if (attempt > 0) {
      pingRetryCount++;
      // Exponential backoff with jitter
      const delay = Math.min(
        finalConfig.baseRetryDelayMs * Math.pow(2, attempt - 1) + Math.random() * 50,
        2000
      );
      await new Promise(resolve => setTimeout(resolve, delay));
    }

    try {
      const result = await pingDatabaseSingleAttempt(finalConfig, startTime);
      
      // Success path
      const latencyMs = Date.now() - startTime;
      lastPingState = 'healthy';
      lastPingTimestamp = Date.now();
      
      if (finalConfig.enableMetrics) {
        pingSuccessCount++;
        pingTotalLatencyMs += latencyMs;
        recordMetric('db_ping_success_total', 1);
        recordMetric('db_ping_duration_ms', latencyMs);
      }

      return {
        success: true,
        latencyMs,
        timestamp: lastPingTimestamp,
        attempts
      };

    } catch (error) {
      const pingError = classifyPingError(error, attempts);
      lastError = pingError;
      
      // Update state based on error type
      lastPingState = getStateFromError(pingError);
      lastPingTimestamp = Date.now();
      
      if (finalConfig.enableMetrics) {
        pingFailureCount++;
        pingErrorCounts[pingError.code] = (pingErrorCounts[pingError.code] || 0) + 1;
        recordMetric('db_ping_failure_total', 1, { 
          error_code: pingError.code,
          error_type: pingError.constructor.name,
          severity: pingError.severity
        });
      }

      // Only retry if error is retryable and we haven't exhausted attempts
      if (!pingError.retryable || attempt >= finalConfig.maxRetries) {
        break;
      }
    }
  }

  // All attempts exhausted or non-retryable error
  const latencyMs = Date.now() - startTime;
  
  if (finalConfig.enableMetrics && attempts > 1) {
    recordMetric('db_ping_retry_attempts_total', attempts - 1, { 
      final_error: lastError?.code || 'unknown'
    });
  }
  
  return {
    success: false,
    error: lastError!,
    latencyMs,
    timestamp: lastPingTimestamp,
    attempts
  };
}

/**
 * Single ping attempt with timeout handling.
 */
async function pingDatabaseSingleAttempt(config: DatabasePingConfig, startTime: number): Promise<any> {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new DatabasePingTimeoutError(config.timeoutMs));
    }, config.timeoutMs);

    try {
      const db = getDatabase();
      const stmt = db.prepare(config.pingQuery);
      const result = stmt.get();
      
      clearTimeout(timeout);
      
      // Validate result structure based on query type
      if (config.pingQuery === 'SELECT 1 AS health_check') {
        if (result?.health_check !== 1) {
          reject(new DatabasePingCorruptionError('Ping query returned unexpected result'));
          return;
        }
      } else if (config.pingQuery.includes('AS custom_result') || config.pingQuery.includes('AS test_result')) {
        // Allow custom queries with different result structures
        if (!result || typeof result !== 'object') {
          reject(new DatabasePingCorruptionError('Custom ping query returned unexpected result format'));
          return;
        }
      }
      
      resolve(result);
    } catch (error) {
      clearTimeout(timeout);
      reject(error);
    }
  });
}

/**
 * Classify raw database errors into typed ping errors.
 */
function classifyPingError(error: any, attempts: number): DatabasePingError {
  if (error instanceof DatabasePingError) {
    return error;
  }

  const message = error?.message || 'Unknown error';
  const code = error?.code;

  // SQLite error code classification
  switch (code) {
    case 'SQLITE_BUSY':
    case 'SQLITE_LOCKED':
      return new DatabasePingBusyError(message, attempts);
    
    case 'SQLITE_READONLY':
    case 'SQLITE_PERM':
      return new DatabasePingPermissionError(message);
    
    case 'SQLITE_CORRUPT':
    case 'SQLITE_NOTADB':
      return new DatabasePingCorruptionError(message);
    
    case 'SQLITE_CANTOPEN':
    case 'SQLITE_IOERR':
      return new DatabasePingConnectionError(message);
    
    default:
      // Check message patterns for additional classification
      const lowerMessage = message.toLowerCase();
      
      if (lowerMessage.includes('timeout') || lowerMessage.includes('timed out')) {
        return new DatabasePingTimeoutError(5000);
      }
      
      if (lowerMessage.includes('permission') || lowerMessage.includes('denied')) {
        return new DatabasePingPermissionError(message);
      }
      
      if (lowerMessage.includes('busy') || lowerMessage.includes('locked')) {
        return new DatabasePingBusyError(message, attempts);
      }
      
      if (lowerMessage.includes('corrupt') || lowerMessage.includes('not a database')) {
        return new DatabasePingCorruptionError(message);
      }
      
      // Default to connection error for unknown failures
      return new DatabasePingConnectionError(message);
  }
}

/**
 * Map error types to ping states.
 */
function getStateFromError(error: DatabasePingError): DatabasePingState {
  if (error instanceof DatabasePingTimeoutError) return 'timeout';
  if (error instanceof DatabasePingBusyError) return 'busy';
  if (error instanceof DatabasePingPermissionError) return 'permission_denied';
  if (error instanceof DatabasePingConnectionError) return 'connection_failed';
  if (error instanceof DatabasePingCorruptionError) return 'corrupted';
  return 'unknown';
}

/**
 * Simple health probe – deterministic, never throws, backward compatible.
 * 
 * This maintains the existing boolean API for backward compatibility while
 * internally using the enhanced ping implementation.
 * 
 * @returns true if database is healthy, false otherwise
 */
export function pingDatabase(): boolean {
  try {
    // Use synchronous approach for backward compatibility
    const db = getDatabase();
    const row = db.prepare('SELECT 1 AS ok').get();
    
    // Update state tracking
    if (row?.ok === 1) {
      lastPingState = 'healthy';
      lastPingTimestamp = Date.now();
      if (DEFAULT_PING_CONFIG.enableMetrics) {
        pingSuccessCount++;
        recordMetric('db_ping_success_total', 1);
      }
      return true;
    } else {
      lastPingState = 'corrupted';
      lastPingTimestamp = Date.now();
      if (DEFAULT_PING_CONFIG.enableMetrics) {
        pingFailureCount++;
        pingErrorCounts['PING_CORRUPTION']++;
        recordMetric('db_ping_failure_total', 1, { 
          error_code: 'PING_CORRUPTION',
          error_type: 'DatabasePingCorruptionError',
          severity: 'critical'
        });
      }
      return false;
    }
  } catch (error) {
    // Classify error and update state
    const pingError = classifyPingError(error, 1);
    lastPingState = getStateFromError(pingError);
    lastPingTimestamp = Date.now();
    
    if (DEFAULT_PING_CONFIG.enableMetrics) {
      pingFailureCount++;
      pingErrorCounts[pingError.code] = (pingErrorCounts[pingError.code] || 0) + 1;
      recordMetric('db_ping_failure_total', 1, { 
        error_code: pingError.code,
        error_type: pingError.constructor.name,
        severity: pingError.severity
      });
    }
    
    return false;
  }
}

/**
 * Close the database connection and clear the statement cache.
 * Ensures clean shutdown and prevents memory leaks.
 *
 * Invariants:
 * - The statement cache is always cleared, even if `close()` throws. This
 *   prevents stale prepared statements from being reused against a fresh
 *   connection after a failed close (which would cause silent corruption
 *   or `database connection is not open` errors).
 * - `dbInstance` is always nulled after a close attempt, so a subsequent
 *   `getDatabase()` will re-open fresh instead of returning a half-closed
 *   handle. This makes close idempotent and retry-safe.
 * - Closing an already-closed or never-opened database is a no-op and
 *   does not throw.
 *
 * Concurrency: Node is single-threaded, so the check-then-close sequence
 * is atomic with respect to other JS callbacks. A close that races with a
 * concurrent getDatabase() call will either see the old instance (and the
 * close will fail cleanly with an error) or the new one (and the close
 * will not affect it). The invariant is that we never leave `dbInstance`
 * pointing at a closed handle.
 *
 * @throws Re-throws any error from `better-sqlite3.close()` after cleanup.
 */
export function getDatabaseStatus(): DatabaseStatus {
  return {
    state: _state,
    lastOpenedAt: _lastOpenedAt,
    lastClosedAt: _lastClosedAt,
    lastErrorAt: _lastErrorAt,
    lastErrorCode: _lastErrorCode,
    consecutiveFailures: _consecutiveFailures,
  };
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/**
 * Close the active database connection and release all resources.
 *
 * After this call:
 * - `dbInstance` is set to `null`
 * - The prepared statement cache is cleared
 * - The lifecycle state transitions to `closed`
 * - The next call to `getDatabase()` will open a fresh connection
 *
 * Idempotent: calling `closeDatabase()` on an already-closed instance is a
 * no-op.
 */
export function closeDatabase(): void {
  const instance = dbInstance;
  if (!instance) {
    // Idempotent no-op: closing a never-opened or already-closed database
    // must not throw. Still clear the cache in case it was populated by
    // a previous instance that was never closed.
    statementCache.clear();
    // Drop the metrics alongside the cache so the next generation starts from
    // zeroed counters instead of inheriting a closed generation's hit counts.
    resetCacheMetrics();
    dbInstance.close();
    dbInstance = null;
  }
}

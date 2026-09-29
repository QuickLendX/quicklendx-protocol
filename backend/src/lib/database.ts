// Updated implementation with deterministic failure‑boundary handling for prepared statements.

import Database from 'better-sqlite3';


// ----- Type Declarations -----
const DatabaseConstructor = Database as any;

let dbInstance: any = null;

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
const statementCache = new Map<string, any>();



/**
 * Metrics for deterministic observability.
 */
let cacheHits = 0;
let cacheMisses = 0;
let cacheEvicts = 0;

/**
 * Database ping metrics and state tracking.
 */
let pingSuccessCount = 0;
let pingFailureCount = 0;
let pingRetryCount = 0;
let pingTotalLatencyMs = 0;
let lastPingState: DatabasePingState = 'unknown';
let lastPingTimestamp = 0;

/**
 * Metrics by error type for detailed observability.
 */
const pingErrorCounts: Record<string, number> = {
  'PING_TIMEOUT': 0,
  'PING_CONNECTION_FAILED': 0,
  'PING_PERMISSION_DENIED': 0,
  'PING_BUSY': 0,
  'PING_CORRUPTION': 0,
};

/**
 * Default configuration for database ping behavior.
 */
const DEFAULT_PING_CONFIG: DatabasePingConfig = {
  timeoutMs: 5000,
  maxRetries: 3,
  baseRetryDelayMs: 100,
  pingQuery: 'SELECT 1 AS health_check',
  enableMetrics: true,
};

/**
 * Optional metrics callback for external metrics systems.
 * Set this to integrate with your preferred metrics collection system.
 */
let metricsCallback: ((eventType: string, value: number, labels?: Record<string, string>) => void) | null = null;

/**
 * Set external metrics callback for integration with monitoring systems.
 */
export function setPingMetricsCallback(callback: (eventType: string, value: number, labels?: Record<string, string>) => void): void {
  metricsCallback = callback;
}

/**
 * Record a metric event, both internally and via external callback if set.
 */
function recordMetric(eventType: string, value: number, labels?: Record<string, string>): void {
  if (metricsCallback) {
    try {
      metricsCallback(eventType, value, labels);
    } catch (error) {
      // Don't let metrics collection failures affect database operations
      console.warn('Database ping metrics callback failed:', error);
    }
  }
}

/**
 * Get a singleton instance of the better‑sqlite3 database with sensible pragmas.
 */
export function getDatabase() {
  if (!dbInstance) {
    const db = new DatabaseConstructor(process.env.DATABASE_PATH || '.data/dev.db');
    // Apply performance pragmas.
    db.pragma('journal_mode = WAL');
    db.pragma('synchronous = NORMAL');
    db.pragma('foreign_keys = ON');
    db.pragma('busy_timeout = 5000');
    dbInstance = db;
  }
  return dbInstance;
}

/**
 * Retrieve a prepared statement with deterministic failure handling.
 *
 * 1. Cache‑hit returns the prepared statement after a cheap validation step.
 *    If validation fails due to a stale schema (`SQLITE_SCHEMA`) the entry is evicted
 *    and a fresh preparation is performed.
 * 2. Cache‑miss triggers a guarded preparation sequence:
 *    - Concurrency guard ensures only one preparation per SQL string.
 *    - Retry loop (max 3 attempts) handles transient `SQLITE_BUSY` errors.
 *    - Permission checks surface a `DatabasePermissionError` without caching.
 *    - Any other preparation error surfaces a `DatabasePrepareError`.
 *
 * The public signature is unchanged – callers receive the prepared statement or
 * a thrown error they can handle deterministically.
 */
// Deterministic, synchronous prepared statement retrieval with failure handling.
export function getPreparedStatement(sql: string): any {
  // ----- Cache Hit Path -----
  if (statementCache.has(sql)) {
    cacheHits++;
    const cached = statementCache.get(sql);
    try {
      if (cached.reader) {
        cached.get();
      } else {
        cached.run();
      }
      return cached;
    } catch (e: any) {
      if (e.code === 'SQLITE_SCHEMA') {
        statementCache.delete(sql);
        cacheEvicts++;
        // fall through to preparation
      } else {
        throw e;
      }
    }
  }

  // ----- Cache Miss / Evicted Path -----
  cacheMisses++;
  const maxAttempts = 3;
  for (let attempt = 0; attempt < maxAttempts; attempt++) {
    try {
      const db = getDatabase();
      const stmt = db.prepare(sql);
      // Permission guard – attempt a harmless execution to surface read‑only errors.
      try {
        if (stmt.reader) {
          stmt.get();
        } else {
          stmt.run();
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
      if (err.code === 'SQLITE_BUSY') {
        if (attempt < maxAttempts - 1) {
          // simple synchronous back‑off
          const delay = 50 * (attempt + 1);
          const start = Date.now();
          while (Date.now() - start < delay) {}
          continue;
        }
        throw new DatabaseBusyError(sql, err);
      }
      // Any other error is a preparation failure.
      throw new DatabasePrepareError(sql, err);
    }
  }
  // Should never reach here.
  throw new DatabaseError('Unexpected preparation failure');
}
  

/**
 * Clear the statement cache and metrics – useful for testing or schema changes.
 */
export function clearStatementCache(): void {
  statementCache.clear();
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
 * Retrieve cache statistics including deterministic metrics.
 */
export function getStatementCacheStats() {
  return {
    size: statementCache.size,
    statements: Array.from(statementCache.keys()),
    hits: cacheHits,
    misses: cacheMisses,
    evicts: cacheEvicts,
  };
}

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
      
      // Validate result structure
      if (config.pingQuery === 'SELECT 1 AS health_check' && result?.health_check !== 1) {
        reject(new DatabasePingCorruptionError('Ping query returned unexpected result'));
        return;
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
 * Graceful shutdown.
 */
export function closeDatabase() {
  if (dbInstance) {
    statementCache.clear();
    dbInstance.close();
    dbInstance = null;
  }
}
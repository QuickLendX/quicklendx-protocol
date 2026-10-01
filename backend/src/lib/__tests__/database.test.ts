// Tests for deterministic getPreparedStatement implementation
import { 
  getPreparedStatement, 
  getDatabase, 
  clearStatementCache, 
  DatabasePermissionError, 
  DatabaseBusyError, 
  DatabasePrepareError,
  // New ping-related imports
  pingDatabase,
  pingDatabaseDetailed,
  DatabasePingError,
  DatabasePingTimeoutError,
  DatabasePingConnectionError,
  DatabasePingPermissionError,
  DatabasePingBusyError,
  DatabasePingCorruptionError,
  getPingStats,
  getPingState,
  resetPingMetrics,
  setPingMetricsCallback,
  getPingMetricsForPrometheus
} from '../database';

beforeEach(() => {
  clearStatementCache();
  jest.resetAllMocks();
});

afterEach(() => {
  jest.restoreAllMocks();
});

describe("getPreparedStatement success and caching", () => {
  test("prepares a statement once and reuses it from the cache", () => {
    const stmt = readerStatement();
    mockPrepare.mockReturnValue(stmt);

    const first = getPreparedStatement("SELECT 1");
    const second = getPreparedStatement("SELECT 1");

    expect(first).toBe(stmt);
    expect(second).toBe(first);
    expect(mockPrepare).toHaveBeenCalledTimes(1);
  });

  test("validates writer statements through the run() probe", () => {
    const stmt = writerStatement();
    mockPrepare.mockReturnValue(stmt);

    expect(getPreparedStatement("INSERT INTO t VALUES (1)")).toBe(stmt);
    expect(stmt.run).toHaveBeenCalled();
  });
});

describe("getPreparedStatement failure boundaries", () => {
  test("retries on SQLITE_BUSY and eventually succeeds", () => {
    const stmt = readerStatement();
    mockPrepare
      .mockImplementationOnce(() => {
        throw errorWithCode("busy", "SQLITE_BUSY");
      })
      .mockImplementationOnce(() => {
        throw errorWithCode("busy", "SQLITE_BUSY");
      })
      .mockReturnValue(stmt);

    expect(getPreparedStatement("SELECT 1")).toBe(stmt);
    expect(mockPrepare).toHaveBeenCalledTimes(3);
  });

  test("throws DatabaseBusyError after max retries", () => {
    mockPrepare.mockImplementation(() => {
      throw errorWithCode("busy", "SQLITE_BUSY");
    });

    expect(() => getPreparedStatement("SELECT 1")).toThrow(DatabaseBusyError);
    expect(mockPrepare).toHaveBeenCalledTimes(3);
  });

  test("throws DatabasePermissionError on a read-only statement and does not cache it", () => {
    const stmt = readerStatement(() => {
      throw errorWithCode("readonly", "SQLITE_READONLY");
    });
    mockPrepare.mockReturnValue(stmt);

    expect(() => getPreparedStatement("SELECT 1")).toThrow(
      DatabasePermissionError,
    );
    // The failed statement must be retried (not served from cache).
    expect(() => getPreparedStatement("SELECT 1")).toThrow(
      DatabasePermissionError,
    );
    expect(mockPrepare).toHaveBeenCalledTimes(2);
    expect(getStatementCacheStats().size).toBe(0);
  });

  test("evicts a stale cached statement when validation raises SQLITE_SCHEMA", () => {
    const stale = readerStatement(() => {
      throw errorWithCode("schema changed", "SQLITE_SCHEMA");
    });
    const fresh = readerStatement();
    mockPrepare.mockReturnValueOnce(stale).mockReturnValueOnce(fresh);

    // The first preparation caches the statement; the stale-schema failure is
    // only observable when the cached entry is validated on the next lookup.
    expect(getPreparedStatement("SELECT 1")).toBe(stale);

    expect(getPreparedStatement("SELECT 1")).toBe(fresh);
    expect(getStatementCacheStats().evicts).toBe(1);
    expect(mockPrepare).toHaveBeenCalledTimes(2);
  });

  test("wraps other preparation errors in DatabasePrepareError", () => {
    mockPrepare.mockImplementation(() => {
      throw errorWithCode("syntax error", "SQLITE_ERROR");
    });

    expect(() => getPreparedStatement("BAD SQL")).toThrow(DatabasePrepareError);
  });

  describe('getStatementCacheStats', () => {
    test('returns correct stats when cache is empty', () => {
      const stats = getStatementCacheStats();
      expect(stats.size).toBe(0);
      expect(stats.statements).toEqual([]);
      expect(stats.statements).toHaveLength(0);
      expect(stats.hits).toBe(0);
      expect(stats.misses).toBe(0);
      expect(stats.evicts).toBe(0);
    });

    test('returns correct stats after preparing statements', () => {
      process.env.DATABASE_PATH = ':memory:';
      const stmt1 = getPreparedStatement('SELECT 1');
      const stmt2 = getPreparedStatement('SELECT 2');
      const stats = getStatementCacheStats();
      expect(stats.size).toBe(2);
      expect(stats.statements).toContain('SELECT 1');
      expect(stats.statements).toContain('SELECT 2');
      expect(stats.hits).toBe(0);
      expect(stats.misses).toBe(2);
      expect(stats.evicts).toBe(0);
    });

    test('returns consistent stats across multiple calls', () => {
      process.env.DATABASE_PATH = ':memory:';
      getPreparedStatement('SELECT 1');
      const first = getStatementCacheStats();
      const second = getStatementCacheStats();
      expect(first).toEqual(second);
    });

    test('returns correct stats after clearing cache', () => {
      process.env.DATABASE_PATH = ':memory:';
      getPreparedStatement('SELECT 1');
      clearStatementCache();
      const stats = getStatementCacheStats();
      expect(stats.size).toBe(0);
      expect(stats.hits).toBe(0);
      expect(stats.misses).toBe(0);
      expect(stats.evicts).toBe(0);
    });

    test('statements array reflects current cache keys', () => {
      process.env.DATABASE_PATH = ':memory:';
      getPreparedStatement('SELECT 1');
      getPreparedStatement('SELECT 2');
      const stats = getStatementCacheStats();
      expect(stats.statements).toContain('SELECT 1');
      expect(stats.statements).toContain('SELECT 2');
      expect(stats.statements).toHaveLength(2);
    });

    test('counts a cache hit without changing size', () => {
      process.env.DATABASE_PATH = ':memory:';
      getPreparedStatement('SELECT 1');
      getPreparedStatement('SELECT 1');
      getPreparedStatement('SELECT 1');
      const stats = getStatementCacheStats();
      expect(stats.size).toBe(1);
      expect(stats.hits).toBe(2);
      expect(stats.misses).toBe(1);
      expect(stats.evicts).toBe(0);
    });

    test('a duplicate SQL string never inflates size or misses', () => {
      process.env.DATABASE_PATH = ':memory:';
      for (let i = 0; i < 5; i++) getPreparedStatement('SELECT 1');
      const stats = getStatementCacheStats();
      expect(stats.size).toBe(1);
      expect(stats.misses).toBe(1);
      expect(stats.hits).toBe(4);
    });

    test('a failed preparation is counted as a miss but never cached', () => {
      process.env.DATABASE_PATH = ':memory:';
      getPreparedStatement('SELECT 1');
      expect(() => getPreparedStatement('BAD SQL')).toThrow(DatabasePrepareError);
      const stats = getStatementCacheStats();
      // The rejected statement must not be observable in the cache.
      expect(stats.size).toBe(1);
      expect(stats.statements).toEqual(['SELECT 1']);
      expect(stats.misses).toBe(2);
    });

    test('returns a defensive copy that cannot corrupt the cache', () => {
      process.env.DATABASE_PATH = ':memory:';
      getPreparedStatement('SELECT 1');
      const first = getStatementCacheStats();
      first.statements.push('TAMPERED');
      first.hits = 999;
      const second = getStatementCacheStats();
      expect(second.statements).toEqual(['SELECT 1']);
      expect(second.hits).toBe(0);
      expect(second.size).toBe(1);
    });

    test('reports insertion order for cached statements', () => {
      process.env.DATABASE_PATH = ':memory:';
      getPreparedStatement('SELECT 3');
      getPreparedStatement('SELECT 1');
      getPreparedStatement('SELECT 2');
      expect(getStatementCacheStats().statements).toEqual(['SELECT 3', 'SELECT 1', 'SELECT 2']);
    });

    test('closeDatabase clears the cache and its metrics together', () => {
      process.env.DATABASE_PATH = ':memory:';
      getPreparedStatement('SELECT 1');
      getPreparedStatement('SELECT 1');
      expect(getStatementCacheStats().hits).toBe(1);

      closeDatabase();

      // Regression guard: closing used to leave the previous generation's
      // counters behind, reporting size 0 alongside non-zero hits/misses.
      const stats = getStatementCacheStats();
      expect(stats.size).toBe(0);
      expect(stats.statements).toEqual([]);
      expect(stats.hits).toBe(0);
      expect(stats.misses).toBe(0);
      expect(stats.evicts).toBe(0);
    });

    test('stats stay self-consistent across many prepares and clears', () => {
      process.env.DATABASE_PATH = ':memory:';
      for (let round = 0; round < 3; round++) {
        getPreparedStatement('SELECT 1');
        getPreparedStatement('SELECT 2');
        const stats = getStatementCacheStats();
        expect(stats.size).toBe(2);
        expect(stats.misses).toBe(2);
        expect(stats.hits).toBe(0);
        clearStatementCache();
        expect(getStatementCacheStats()).toEqual({
          size: 0,
          statements: [],
          hits: 0,
          misses: 0,
          evicts: 0,
        });
      }
    });
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Database Ping Tests - Comprehensive failure boundary coverage
// ──────────────────────────────────────────────────────────────────────────────

describe('pingDatabase backward compatibility', () => {
  test('returns true for successful ping', () => {
    // Use a real in-memory SQLite DB for this simple case
    process.env.DATABASE_PATH = ':memory:';
    
    const result = pingDatabase();
    expect(result).toBe(true);
    
    const stats = getPingStats();
    expect(stats.successCount).toBeGreaterThan(0);
    expect(stats.lastState).toBe('healthy');
  });

  test('returns false for database connection failure', () => {
    const mockPrepare = jest.fn(() => {
      const err: any = new Error('connection failed');
      err.code = 'SQLITE_CANTOPEN';
      throw err;
    });
    
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    const result = pingDatabase();
    expect(result).toBe(false);
    
    const stats = getPingStats();
    expect(stats.failureCount).toBeGreaterThan(0);
    expect(stats.lastState).toBe('connection_failed');
  });

  test('returns false for corrupted result', () => {
    const mockPrepare = jest.fn(() => ({
      get: () => ({ ok: 0 }), // Wrong result
    }));
    
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    const result = pingDatabase();
    expect(result).toBe(false);
    expect(getPingState()).toBe('corrupted');
  });
});

describe('pingDatabaseDetailed comprehensive failure scenarios', () => {
  beforeEach(() => {
    resetPingMetrics();
  });

  test('successful ping with metrics', async () => {
    process.env.DATABASE_PATH = ':memory:';
    
    const result = await pingDatabaseDetailed();
    
    expect(result.success).toBe(true);
    expect(result.latencyMs).toBeGreaterThan(0);
    expect(result.timestamp).toBeGreaterThan(0);
    expect(result.attempts).toBe(1);
    
    const stats = getPingStats();
    expect(stats.successCount).toBe(1);
    expect(stats.failureCount).toBe(0);
    expect(stats.successRate).toBe(1);
  });

  test('timeout error with retry', async () => {
    let callCount = 0;
    const mockPrepare = jest.fn(() => ({
      get: () => {
        callCount++;
        // Simulate timeout on first two calls, succeed on third
        if (callCount <= 2) {
          const err: any = new Error('timeout');
          err.message = 'timeout';
          throw err;
        }
        return { health_check: 1 };
      },
    }));
    
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    const result = await pingDatabaseDetailed({ 
      timeoutMs: 100,
      baseRetryDelayMs: 10 // Fast retries for testing
    });
    
    expect(result.success).toBe(true);
    expect(result.attempts).toBe(3);
    expect(mockPrepare).toHaveBeenCalledTimes(3);
  });

  test('exhausts retries for persistent timeout', async () => {
    const mockPrepare = jest.fn(() => ({
      get: () => {
        const err: any = new Error('persistent timeout');
        err.message = 'timed out';
        throw err;
      },
    }));
    
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    const result = await pingDatabaseDetailed({ 
      maxRetries: 2,
      baseRetryDelayMs: 1
    });
    
    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.attempts).toBe(3); // initial + 2 retries
      expect(result.error).toBeInstanceOf(DatabasePingTimeoutError);
      expect(result.error.retryable).toBe(true);
      expect(result.error.severity).toBe('warning');
    }
  });

  test('busy error with exponential backoff', async () => {
    let callCount = 0;
    const mockPrepare = jest.fn(() => ({
      get: () => {
        callCount++;
        if (callCount <= 1) {
          const err: any = new Error('busy');
          err.code = 'SQLITE_BUSY';
          throw err;
        }
        return { health_check: 1 };
      },
    }));
    
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    const startTime = Date.now();
    const result = await pingDatabaseDetailed({ baseRetryDelayMs: 50 });
    const endTime = Date.now();
    
    expect(result.success).toBe(true);
    expect(result.attempts).toBe(2);
    // Should have some delay due to backoff
    expect(endTime - startTime).toBeGreaterThanOrEqual(50);
  });

  test('permission error - non-retryable', async () => {
    const mockPrepare = jest.fn(() => ({
      get: () => {
        const err: any = new Error('permission denied');
        err.code = 'SQLITE_READONLY';
        throw err;
      },
    }));
    
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    const result = await pingDatabaseDetailed({ maxRetries: 3 });
    
    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.attempts).toBe(1); // No retries for non-retryable errors
      expect(result.error).toBeInstanceOf(DatabasePingPermissionError);
      expect(result.error.retryable).toBe(false);
      expect(result.error.severity).toBe('critical');
      expect(mockPrepare).toHaveBeenCalledTimes(1);
    }
  });

  test('corruption error - non-retryable critical', async () => {
    const mockPrepare = jest.fn(() => ({
      get: () => {
        const err: any = new Error('database is corrupt');
        err.code = 'SQLITE_CORRUPT';
        throw err;
      },
    }));
    
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    const result = await pingDatabaseDetailed();
    
    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.attempts).toBe(1);
      expect(result.error).toBeInstanceOf(DatabasePingCorruptionError);
      expect(result.error.code).toBe('PING_CORRUPTION');
      expect(result.error.severity).toBe('critical');
    }
  });

  test('connection error with retries', async () => {
    let callCount = 0;
    const mockPrepare = jest.fn(() => ({
      get: () => {
        callCount++;
        const err: any = new Error('cannot open');
        err.code = 'SQLITE_CANTOPEN';
        throw err;
      },
    }));
    
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    const result = await pingDatabaseDetailed({ maxRetries: 2, baseRetryDelayMs: 1 });
    
    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.attempts).toBe(3);
      expect(result.error).toBeInstanceOf(DatabasePingConnectionError);
      expect(result.error.retryable).toBe(true);
    }
    expect(mockPrepare).toHaveBeenCalledTimes(3);
  });

  test('custom ping query validation', async () => {
    const mockPrepare = jest.fn(() => ({
      get: () => ({ custom_result: 'ok' }),
    }));
    
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    const result = await pingDatabaseDetailed({ 
      pingQuery: 'SELECT \'ok\' AS custom_result'
    });
    
    expect(result.success).toBe(true);
    expect(mockPrepare).toHaveBeenCalledWith('SELECT \'ok\' AS custom_result');
  });

  test('unexpected result format triggers corruption error', async () => {
    const mockPrepare = jest.fn(() => ({
      get: () => ({ health_check: 'wrong_type' }), // Should be 1
    }));
    
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    const result = await pingDatabaseDetailed();
    
    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.error).toBeInstanceOf(DatabasePingCorruptionError);
    }
  });
});

describe('error classification', () => {
  beforeEach(() => {
    resetPingMetrics();
  });

  test('classifies SQLite error codes correctly', async () => {
    const testCases = [
      { code: 'SQLITE_BUSY', expectedType: DatabasePingBusyError, retryable: true },
      { code: 'SQLITE_LOCKED', expectedType: DatabasePingBusyError, retryable: true },
      { code: 'SQLITE_READONLY', expectedType: DatabasePingPermissionError, retryable: false },
      { code: 'SQLITE_PERM', expectedType: DatabasePingPermissionError, retryable: false },
      { code: 'SQLITE_CORRUPT', expectedType: DatabasePingCorruptionError, retryable: false },
      { code: 'SQLITE_NOTADB', expectedType: DatabasePingCorruptionError, retryable: false },
      { code: 'SQLITE_CANTOPEN', expectedType: DatabasePingConnectionError, retryable: true },
      { code: 'SQLITE_IOERR', expectedType: DatabasePingConnectionError, retryable: true },
    ];

    for (const { code, expectedType, retryable } of testCases) {
      resetPingMetrics();
      
      const mockPrepare = jest.fn(() => ({
        get: () => {
          const err: any = new Error(`Test error for ${code}`);
          err.code = code;
          throw err;
        },
      }));
      
      jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
        prepare: mockPrepare,
      } as any));

      const result = await pingDatabaseDetailed({ maxRetries: 0 });
      
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toBeInstanceOf(expectedType);
        expect(result.error.retryable).toBe(retryable);
      }
    }
  });

  test('classifies message patterns correctly', async () => {
    const testCases = [
      { message: 'operation timed out', expectedType: DatabasePingTimeoutError },
      { message: 'permission denied', expectedType: DatabasePingPermissionError },
      { message: 'database is busy', expectedType: DatabasePingBusyError },
      { message: 'file is corrupt', expectedType: DatabasePingCorruptionError },
      { message: 'not a database file', expectedType: DatabasePingCorruptionError },
      { message: 'unknown error', expectedType: DatabasePingConnectionError }, // default
    ];

    for (const { message, expectedType } of testCases) {
      resetPingMetrics();
      
      const mockPrepare = jest.fn(() => ({
        get: () => {
          throw new Error(message);
        },
      }));
      
      jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
        prepare: mockPrepare,
      } as any));

      const result = await pingDatabaseDetailed({ maxRetries: 0 });
      
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toBeInstanceOf(expectedType);
      }
    }
  });
});

describe('metrics and observability', () => {
  beforeEach(() => {
    resetPingMetrics();
  });

  test('tracks success metrics correctly', async () => {
    process.env.DATABASE_PATH = ':memory:';
    
    // Perform multiple successful pings
    await pingDatabaseDetailed();
    await pingDatabaseDetailed();
    pingDatabase(); // Test backward compatible version too
    
    const stats = getPingStats();
    expect(stats.successCount).toBe(3);
    expect(stats.failureCount).toBe(0);
    expect(stats.successRate).toBe(1);
    expect(stats.averageLatencyMs).toBeGreaterThan(0);
    expect(stats.lastState).toBe('healthy');
  });

  test('tracks failure metrics with error breakdown', async () => {
    const errorCodes = ['SQLITE_BUSY', 'SQLITE_READONLY', 'SQLITE_CORRUPT'];
    
    for (const code of errorCodes) {
      const mockPrepare = jest.fn(() => ({
        get: () => {
          const err: any = new Error(`Test ${code}`);
          err.code = code;
          throw err;
        },
      }));
      
      jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
        prepare: mockPrepare,
      } as any));

      await pingDatabaseDetailed({ maxRetries: 0 });
    }
    
    const stats = getPingStats();
    expect(stats.successCount).toBe(0);
    expect(stats.failureCount).toBe(3);
    expect(stats.successRate).toBe(0);
    expect(stats.errorBreakdown).toBeDefined();
    expect(stats.errorBreakdown['PING_BUSY']).toBe(1);
    expect(stats.errorBreakdown['PING_PERMISSION_DENIED']).toBe(1);
    expect(stats.errorBreakdown['PING_CORRUPTION']).toBe(1);
  });

  test('external metrics callback integration', async () => {
    const metricsEvents: Array<{eventType: string, value: number, labels?: Record<string, string>}> = [];
    
    setPingMetricsCallback((eventType, value, labels) => {
      metricsEvents.push({ eventType, value, labels });
    });
    
    process.env.DATABASE_PATH = ':memory:';
    await pingDatabaseDetailed();
    
    expect(metricsEvents.length).toBeGreaterThan(0);
    expect(metricsEvents.some(e => e.eventType === 'db_ping_success_total')).toBe(true);
    expect(metricsEvents.some(e => e.eventType === 'db_ping_duration_ms')).toBe(true);
    
    // Reset callback
    setPingMetricsCallback(null as any);
  });

  test('external metrics callback handles errors gracefully', async () => {
    const consoleSpy = jest.spyOn(console, 'warn').mockImplementation(() => {});
    
    setPingMetricsCallback(() => {
      throw new Error('Metrics callback failure');
    });
    
    process.env.DATABASE_PATH = ':memory:';
    // Should not throw despite callback failure
    const result = await pingDatabaseDetailed();
    
    expect(result.success).toBe(true);
    expect(consoleSpy).toHaveBeenCalledWith('Database ping metrics callback failed:', expect.any(Error));
    
    consoleSpy.mockRestore();
    setPingMetricsCallback(null as any);
  });

  test('Prometheus metrics format', () => {
    // Generate some test data
    resetPingMetrics();
    process.env.DATABASE_PATH = ':memory:';
    pingDatabase(); // Success
    
    const mockPrepare = jest.fn(() => {
      const err: any = new Error('busy');
      err.code = 'SQLITE_BUSY';
      throw err;
    });
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));
    pingDatabase(); // Failure
    
    const metrics = getPingMetricsForPrometheus();
    
    expect(metrics).toBeInstanceOf(Array);
    expect(metrics.length).toBeGreaterThan(0);
    
    const successMetric = metrics.find(m => m.name === 'qlx_db_ping_success_total');
    expect(successMetric).toBeDefined();
    expect(successMetric?.type).toBe('counter');
    expect(successMetric?.value).toBe(1);
    
    const failureMetric = metrics.find(m => m.name === 'qlx_db_ping_failure_total');
    expect(failureMetric).toBeDefined();
    expect(failureMetric?.value).toBe(1);
    
    const stateMetric = metrics.find(m => m.name === 'qlx_db_ping_state');
    expect(stateMetric).toBeDefined();
    expect(stateMetric?.value).toBe(3); // 'busy' state
    
    const errorMetric = metrics.find(m => m.name === 'qlx_db_ping_error_total');
    expect(errorMetric).toBeDefined();
    expect(errorMetric?.labels?.error_code).toBe('PING_BUSY');
  });
});

describe('concurrency and timing', () => {
  beforeEach(() => {
    resetPingMetrics();
  });

  test('concurrent pings handle state correctly', async () => {
    process.env.DATABASE_PATH = ':memory:';
    
    const promises = Array.from({ length: 5 }, () => pingDatabaseDetailed());
    const results = await Promise.all(promises);
    
    expect(results.every(r => r.success)).toBe(true);
    expect(getPingStats().successCount).toBe(5);
  });

  test('state transitions are deterministic', async () => {
    // Test sequence: healthy -> busy -> healthy
    process.env.DATABASE_PATH = ':memory:';
    await pingDatabaseDetailed();
    expect(getPingState()).toBe('healthy');
    
    const mockPrepare = jest.fn(() => ({
      get: () => {
        const err: any = new Error('busy');
        err.code = 'SQLITE_BUSY';
        throw err;
      },
    }));
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));
    
    await pingDatabaseDetailed({ maxRetries: 0 });
    expect(getPingState()).toBe('busy');
    
    // Reset to healthy
    process.env.DATABASE_PATH = ':memory:';
    jest.restoreAllMocks();
    await pingDatabaseDetailed();
    expect(getPingState()).toBe('healthy');
  });

  test('time since last ping calculation', async () => {
    const statsBefore = getPingStats();
    expect(statsBefore.timeSinceLastPing).toBe(-1); // Never pinged
    
    process.env.DATABASE_PATH = ':memory:';
    await pingDatabaseDetailed();
    
    const statsAfter = getPingStats();
    expect(statsAfter.timeSinceLastPing).toBeGreaterThanOrEqual(0);
    expect(statsAfter.timeSinceLastPing).toBeLessThan(100); // Should be very recent
  });
});

describe('configuration options', () => {
  beforeEach(() => {
    resetPingMetrics();
  });

  test('respects custom timeout', async () => {
    // Mock a slow operation
    const mockPrepare = jest.fn(() => ({
      get: () => {
        // This would timeout with a short timeout
        return new Promise(resolve => setTimeout(() => resolve({ health_check: 1 }), 200));
      },
    }));
    
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    const result = await pingDatabaseDetailed({ timeoutMs: 50 });
    
    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.error).toBeInstanceOf(DatabasePingTimeoutError);
    }
  });

  test('metrics can be disabled', async () => {
    process.env.DATABASE_PATH = ':memory:';
    
    await pingDatabaseDetailed({ enableMetrics: false });
    
    const stats = getPingStats();
    expect(stats.successCount).toBe(0); // Should not increment with metrics disabled
  });

  test('custom ping query works', async () => {
    const mockPrepare = jest.fn(() => ({
      get: () => ({ test_result: 'success' }),
    }));
    
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    await pingDatabaseDetailed({ 
      pingQuery: 'SELECT \'success\' AS test_result'
    });
    
    expect(mockPrepare).toHaveBeenCalledWith('SELECT \'success\' AS test_result');
  });
});

describe('boundary cases and edge conditions', () => {
  beforeEach(() => {
    resetPingMetrics();
  });

  test('handles zero retries configuration', async () => {
    const mockPrepare = jest.fn(() => ({
      get: () => {
        const err: any = new Error('fail');
        err.code = 'SQLITE_BUSY';
        throw err;
      },
    }));
    
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    const result = await pingDatabaseDetailed({ maxRetries: 0 });
    
    expect(result.success).toBe(false);
    expect(result.attempts).toBe(1);
    expect(mockPrepare).toHaveBeenCalledTimes(1);
  });

  test('handles very high retry count', async () => {
    let callCount = 0;
    const mockPrepare = jest.fn(() => ({
      get: () => {
        callCount++;
        if (callCount <= 10) {
          const err: any = new Error('busy');
          err.code = 'SQLITE_BUSY';
          throw err;
        }
        return { health_check: 1 };
      },
    }));
    
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    const result = await pingDatabaseDetailed({ 
      maxRetries: 15,
      baseRetryDelayMs: 1 // Fast retries for testing
    });
    
    expect(result.success).toBe(true);
    expect(result.attempts).toBe(11);
  });

  test('handles null and undefined database responses', async () => {
    const mockPrepare = jest.fn(() => ({
      get: () => null,
    }));
    
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    const result = await pingDatabaseDetailed();
    
    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.error).toBeInstanceOf(DatabasePingCorruptionError);
    }
  });

  test('resetPingMetrics clears all state', () => {
    // Generate some metrics
    process.env.DATABASE_PATH = ':memory:';
    pingDatabase();
    
    let stats = getPingStats();
    expect(stats.successCount).toBeGreaterThan(0);
    
    resetPingMetrics();
    stats = getPingStats();
    expect(stats.successCount).toBe(0);
    expect(stats.failureCount).toBe(0);
    expect(stats.retryCount).toBe(0);
    expect(stats.totalAttempts).toBe(0);
    expect(stats.lastState).toBe('unknown');
    expect(stats.timeSinceLastPing).toBe(-1);
    expect(Object.values(stats.errorBreakdown).every(count => count === 0)).toBe(true);
  });
});

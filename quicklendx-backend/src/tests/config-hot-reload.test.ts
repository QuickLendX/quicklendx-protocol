import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  getConfig,
  resetConfig,
  reloadConfig,
  onReload,
  setupSignalHandlers,
  reloadListenerCount,
  resetSignalHandlers,
  clearReloadListeners,
} from '../config/loader';
import {
  setupLagMonitorReload,
  getWarnThreshold,
  getCriticalThreshold,
  getLagSeverity,
} from '../services/lagMonitor';
import { setupRateLimitReload, getRateLimitPoints, getMaxRequestsPerMinute } from '../middleware/rate-limit';

/**
 * Minimal set of env vars required for ConfigSchema.parse() to pass.
 */
const BASE_ENV: Record<string, string> = {
  NODE_ENV: 'test',
  PORT: '3000',
  LOG_LEVEL: 'info',
  DATABASE_URL: 'postgresql://localhost:5432/db',
  JWT_SECRET: 'test-secret-key-must-be-long-enough-to-pass-validation-here',
  API_KEY: 'test-api-key-must-be-long-enough',
  ENCRYPTION_KEY: 'test-encryption-key-must-be-long-enough-to-pass-validation',
  STELLAR_NETWORK_URL: 'https://horizon-testnet.stellar.org',
  STELLAR_NETWORK_PASSPHRASE: 'Test SDF Network ; September 2015',
};

let originalEnv: Record<string, string | undefined>;

function applyEnv(vars: Record<string, string>): void {
  for (const [k, v] of Object.entries(vars)) {
    process.env[k] = v;
  }
}

beforeEach(() => {
  originalEnv = { ...process.env };
  resetConfig();
  resetSignalHandlers();
  clearReloadListeners();
  process.removeAllListeners('SIGHUP');
  applyEnv(BASE_ENV);
});

afterEach(() => {
  vi.restoreAllMocks();
  process.env = originalEnv;
  resetConfig();
  resetSignalHandlers();
  clearReloadListeners();
  process.removeAllListeners('SIGHUP');
});

// ---------------------------------------------------------------------------
// Basic reload
// ---------------------------------------------------------------------------

describe('config hot reload', () => {
  it('should change a hot-reloadable value via reloadConfig', () => {
    const config1 = getConfig();
    expect(config1.RATE_LIMIT_POINTS).toBe(1000);

    process.env.RATE_LIMIT_POINTS = '500';
    reloadConfig();

    const config2 = getConfig();
    expect(config2.RATE_LIMIT_POINTS).toBe(500);
  });

  it('should reload LAG_WARN_THRESHOLD and LAG_CRITICAL_THRESHOLD', () => {
    const config1 = getConfig();
    expect(config1.LAG_WARN_THRESHOLD).toBe(10);
    expect(config1.LAG_CRITICAL_THRESHOLD).toBe(100);

    process.env.LAG_WARN_THRESHOLD = '25';
    process.env.LAG_CRITICAL_THRESHOLD = '200';
    reloadConfig();

    const config2 = getConfig();
    expect(config2.LAG_WARN_THRESHOLD).toBe(25);
    expect(config2.LAG_CRITICAL_THRESHOLD).toBe(200);
  });

  it('should reload RPC_ALLOWED_HOSTS as an array', () => {
    process.env.RPC_ALLOWED_HOSTS = 'alpha.example.com,beta.example.com';
    reloadConfig();

    const config = getConfig();
    expect(config.RPC_ALLOWED_HOSTS).toEqual(['alpha.example.com', 'beta.example.com']);
  });

  // -----------------------------------------------------------------------
  // Invalid value — keep prior
  // -----------------------------------------------------------------------

  it('should keep the prior value when reload produces an invalid value', () => {
    const config1 = getConfig();
    expect(config1.RATE_LIMIT_POINTS).toBe(1000);

    process.env.LAG_CRITICAL_THRESHOLD = '-1';
    reloadConfig();

    const config2 = getConfig();
    expect(config2.LAG_CRITICAL_THRESHOLD).toBe(100);
  });

  it('should keep the prior value when reload has a missing required secret', () => {
    const config1 = getConfig();
    expect(config1.JWT_SECRET).toBe(BASE_ENV.JWT_SECRET);

    delete process.env.JWT_SECRET;
    reloadConfig();

    const config = getConfig();
    expect(config.JWT_SECRET).toBe(BASE_ENV.JWT_SECRET);
  });

  it('should keep the prior value when an env var has wrong type', () => {
    const config1 = getConfig();
    expect(config1.MAX_REQUESTS_PER_MINUTE).toBe(100);

    process.env.MAX_REQUESTS_PER_MINUTE = 'not-a-number';
    reloadConfig();

    const config = getConfig();
    expect(config.MAX_REQUESTS_PER_MINUTE).toBe(100);
  });

  // -----------------------------------------------------------------------
  // Secrets are immutable post-boot
  // -----------------------------------------------------------------------

  it('should not change secret values on reload', () => {
    const config1 = getConfig();
    expect(config1.JWT_SECRET).toBe(BASE_ENV.JWT_SECRET);
    expect(config1.API_KEY).toBe(BASE_ENV.API_KEY);
    expect(config1.ENCRYPTION_KEY).toBe(BASE_ENV.ENCRYPTION_KEY);

    process.env.JWT_SECRET = 'this-is-a-new-long-secret-that-should-not-be-picked-up-1234567890';
    process.env.API_KEY = 'new-api-key-that-should-be-ignored';
    process.env.ENCRYPTION_KEY = 'new-encryption-key-that-should-not-be-applied-1234567890';
    reloadConfig();

    const config2 = getConfig();
    expect(config2.JWT_SECRET).toBe(BASE_ENV.JWT_SECRET);
    expect(config2.API_KEY).toBe(BASE_ENV.API_KEY);
    expect(config2.ENCRYPTION_KEY).toBe(BASE_ENV.ENCRYPTION_KEY);
  });

  it('should not change DATABASE_URL on reload', () => {
    const config1 = getConfig();
    expect(config1.DATABASE_URL).toBe(BASE_ENV.DATABASE_URL);

    process.env.DATABASE_URL = 'postgresql://evil:5432/db';
    reloadConfig();

    const config = getConfig();
    expect(config.DATABASE_URL).toBe(BASE_ENV.DATABASE_URL);
  });

  // -----------------------------------------------------------------------
  // Multiple subscribers
  // -----------------------------------------------------------------------

  it('should notify multiple subscribers on reload', () => {
    const received: number[] = [];
    const unsub1 = onReload(() => {
      received.push(1);
    });
    const unsub2 = onReload(() => {
      received.push(2);
    });

    process.env.RATE_LIMIT_POINTS = '777';
    reloadConfig();

    expect(received).toEqual([1, 2]);

    unsub1();
    unsub2();
  });

  it('should allow subscribers to unsubscribe', () => {
    const received: number[] = [];
    const unsub = onReload(() => {
      received.push(1);
    });
    unsub();

    process.env.RATE_LIMIT_POINTS = '888';
    reloadConfig();

    expect(received).toEqual([]);
  });

  // -----------------------------------------------------------------------
  // lagMonitor wiring
  // -----------------------------------------------------------------------

  it('should update lagMonitor thresholds on reload', () => {
    const unsub = setupLagMonitorReload();

    process.env.LAG_WARN_THRESHOLD = '42';
    process.env.LAG_CRITICAL_THRESHOLD = '99';
    reloadConfig();

    expect(getWarnThreshold()).toBe(42);
    expect(getCriticalThreshold()).toBe(99);

    unsub();
  });

  it('should compute lag severity based on updated thresholds', () => {
    const unsub = setupLagMonitorReload();

    process.env.LAG_WARN_THRESHOLD = '5';
    process.env.LAG_CRITICAL_THRESHOLD = '20';
    reloadConfig();

    expect(getLagSeverity(3)).toBe('ok');
    expect(getLagSeverity(5)).toBe('warn');
    expect(getLagSeverity(20)).toBe('critical');
    expect(getLagSeverity(50)).toBe('critical');

    unsub();
  });

  // -----------------------------------------------------------------------
  // rate-limit middleware wiring
  // -----------------------------------------------------------------------

  it('should update rate-limit state on reload', () => {
    const unsub = setupRateLimitReload();
    expect(getRateLimitPoints()).toBe(1000);
    expect(getMaxRequestsPerMinute()).toBe(100);

    process.env.RATE_LIMIT_POINTS = '2500';
    process.env.MAX_REQUESTS_PER_MINUTE = '200';
    reloadConfig();

    expect(getRateLimitPoints()).toBe(2500);
    expect(getMaxRequestsPerMinute()).toBe(200);

    unsub();
  });

  // -----------------------------------------------------------------------
  // SIGHUP signal handler
  // -----------------------------------------------------------------------

  it('should reload config when SIGHUP is emitted', () => {
    setupSignalHandlers();

    process.env.RATE_LIMIT_POINTS = '333';
    process.emit('SIGHUP', 'SIGHUP');

    const config = getConfig();
    expect(config.RATE_LIMIT_POINTS).toBe(333);
  });

  it('should be idempotent across rapid SIGHUP signals', () => {
    setupSignalHandlers();

    process.env.RATE_LIMIT_POINTS = '111';
    process.emit('SIGHUP', 'SIGHUP');

    process.env.RATE_LIMIT_POINTS = '222';
    process.emit('SIGHUP', 'SIGHUP');

    process.env.RATE_LIMIT_POINTS = '333';
    process.emit('SIGHUP', 'SIGHUP');

    const config = getConfig();
    expect(config.RATE_LIMIT_POINTS).toBe(333);
  });

  it('should only bind one SIGHUP listener when setupSignalHandlers is called multiple times', () => {
    setupSignalHandlers();
    setupSignalHandlers();
    setupSignalHandlers();

    const listeners = process.listeners('SIGHUP');
    expect(listeners.length).toBe(1);
  });

  it('should not crash when a reload subscriber throws', () => {
    setupSignalHandlers();
    onReload(() => {
      throw new Error('subscriber failure');
    });

    expect(() => {
      process.emit('SIGHUP', 'SIGHUP');
    }).not.toThrow();
  });

  // -----------------------------------------------------------------------
  // Never logs secret values
  // -----------------------------------------------------------------------

  it('should not log secret values on reload', () => {
    const consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

    process.env.RATE_LIMIT_POINTS = '444';
    reloadConfig();

    const logCalls = consoleSpy.mock.calls.map((args) => args.join(' '));
    const errorCalls = consoleErrorSpy.mock.calls.map((args) => args.join(' '));
    const allOutput = [...logCalls, ...errorCalls].join('\n');

    expect(allOutput).not.toContain(BASE_ENV.JWT_SECRET);
    expect(allOutput).not.toContain(BASE_ENV.API_KEY);
    expect(allOutput).not.toContain(BASE_ENV.ENCRYPTION_KEY);

    consoleSpy.mockRestore();
    consoleErrorSpy.mockRestore();
  });

  // -----------------------------------------------------------------------
  // reloadListenerCount
  // -----------------------------------------------------------------------

  it('should track subscriber count via reloadListenerCount', () => {
    expect(reloadListenerCount()).toBe(0);

    const unsub1 = onReload(() => {});
    expect(reloadListenerCount()).toBe(1);

    const unsub2 = onReload(() => {});
    expect(reloadListenerCount()).toBe(2);

    unsub1();
    expect(reloadListenerCount()).toBe(1);

    unsub2();
    expect(reloadListenerCount()).toBe(0);
  });

  // -----------------------------------------------------------------------
  // Edge cases
  // -----------------------------------------------------------------------

  it('should handle reload when no env has been overridden (graceful no-op)', () => {
    const config1 = getConfig();
    reloadConfig();
    const config2 = getConfig();
    expect(config2).toEqual(config1);
  });

  // -----------------------------------------------------------------------
  // Non-test profile logging path
  // -----------------------------------------------------------------------

  it('should log reload info when profile is not test', () => {
    const savedNodeEnv = process.env.NODE_ENV;
    process.env.NODE_ENV = 'development';

    const consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});

    resetConfig();
    const config1 = getConfig();

    process.env.RATE_LIMIT_POINTS = '999';
    reloadConfig();

    const config2 = getConfig();
    expect(config2.RATE_LIMIT_POINTS).toBe(999);

    const logOutput = consoleSpy.mock.calls.map((args) => args.join(' ')).join('\n');
    expect(logOutput).toContain('Configuration reloaded via SIGHUP');

    consoleSpy.mockRestore();
    process.env.NODE_ENV = savedNodeEnv;
  });

  // -----------------------------------------------------------------------
  // lagMonitor remaining functions
  // -----------------------------------------------------------------------

  it('should handle observeLag and getLagLedgers', async () => {
    const { observeLag, getLagLedgers } = await import('../services/lagMonitor');

    expect(await getLagLedgers()).toBe(0);

    observeLag(42);
    expect(await getLagLedgers()).toBe(42);

    observeLag(99);
    expect(await getLagLedgers()).toBe(99);
  });
});

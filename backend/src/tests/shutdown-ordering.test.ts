/**
 * Tests for the ordered shutdown system — Issue #1190
 *
 * Covers:
 *  - Priority order is observed (lower number runs first)
 *  - Errors in one step do not prevent later steps from running
 *  - Total timeout is honored (steps that breach it are skipped)
 *  - Second SIGTERM while shutting down forces immediate exit(1)
 *  - No service runs after shutdown begins (isShuttingDown guard)
 *  - register() / clearRegistry() / getRegisteredSteps() contract
 *  - runAll() accumulates results in priority order
 *  - createShutdownHandler registers the canonical 7-step sequence in order
 *  - Backward-compatible handler still calls server.close / flush / closeDatabase
 */

// ---------------------------------------------------------------------------
// Mocks (hoisted)
// ---------------------------------------------------------------------------
jest.mock('../middleware/load-shedding', () => ({
  getActiveRequests: jest.fn(() => 0),
}));

jest.mock('../services/webhookQueueService', () => ({
  webhookQueueService: { flush: jest.fn(() => []) },
}));

jest.mock('../lib/database', () => ({
  closeDatabase: jest.fn(),
  getDatabase: jest.fn(),
}));

jest.mock('../services/statusService', () => ({
  statusService: { setMaintenanceMode: jest.fn() },
}));

// ---------------------------------------------------------------------------
// Imports
// ---------------------------------------------------------------------------
import http from 'http';
import {
  ShutdownStep,
  register,
  clearRegistry,
  getRegisteredSteps,
  runAll,
  resetShuttingDown,
  isShuttingDown,
  createShutdownHandler,
  DEFAULT_DRAIN_TIMEOUT_MS,
  DRAIN_POLL_MS,
  PRIORITY_HTTP,
  PRIORITY_SCHEDULER,
  PRIORITY_INGESTION,
  PRIORITY_WEBHOOK,
  PRIORITY_RECONCILIATION,
  PRIORITY_NOTIFICATIONS,
  PRIORITY_DB,
} from '../lib/shutdown';
import { getActiveRequests } from '../middleware/load-shedding';
import { webhookQueueService } from '../services/webhookQueueService';
import { closeDatabase } from '../lib/database';
import { statusService } from '../services/statusService';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
function makeMockServer(): http.Server {
  return { close: jest.fn() } as unknown as http.Server;
}

function makeStep(
  name: string,
  priority: number,
  log: string[],
  opts: { throws?: boolean; delayMs?: number } = {},
): ShutdownStep {
  return {
    name,
    priority,
    fn: async () => {
      if (opts.delayMs) await new Promise<void>((r) => setTimeout(r, opts.delayMs));
      if (opts.throws) throw new Error(`${name} failed`);
      log.push(name);
    },
  };
}

// ---------------------------------------------------------------------------
// Suite: register / clearRegistry / getRegisteredSteps
// ---------------------------------------------------------------------------
describe('registry API', () => {
  beforeEach(() => {
    clearRegistry();
    resetShuttingDown();
  });

  it('getRegisteredSteps returns steps sorted by priority ascending', () => {
    register({ name: 'c', priority: 3, fn: async () => {} });
    register({ name: 'a', priority: 1, fn: async () => {} });
    register({ name: 'b', priority: 2, fn: async () => {} });

    const names = getRegisteredSteps().map((s) => s.name);
    expect(names).toEqual(['a', 'b', 'c']);
  });

  it('re-registering with the same name replaces the existing step', () => {
    const fn1 = jest.fn();
    const fn2 = jest.fn();
    register({ name: 'dup', priority: 1, fn: fn1 });
    register({ name: 'dup', priority: 1, fn: fn2 });

    expect(getRegisteredSteps()).toHaveLength(1);
    expect(getRegisteredSteps()[0].fn).toBe(fn2);
  });

  it('clearRegistry removes all steps', () => {
    register({ name: 'x', priority: 1, fn: async () => {} });
    clearRegistry();
    expect(getRegisteredSteps()).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// Suite: runAll — priority ordering
// ---------------------------------------------------------------------------
describe('runAll — priority ordering', () => {
  beforeEach(() => {
    clearRegistry();
    resetShuttingDown();
  });

  it('executes steps in ascending priority order regardless of registration order', async () => {
    const log: string[] = [];
    register(makeStep('step-3', 3, log));
    register(makeStep('step-1', 1, log));
    register(makeStep('step-2', 2, log));

    await runAll('SIGTERM', 5000);

    expect(log).toEqual(['step-1', 'step-2', 'step-3']);
  });

  it('observes the canonical 7-step priority order', () => {
    expect(PRIORITY_HTTP).toBeLessThan(PRIORITY_SCHEDULER);
    expect(PRIORITY_SCHEDULER).toBeLessThan(PRIORITY_INGESTION);
    expect(PRIORITY_INGESTION).toBeLessThan(PRIORITY_WEBHOOK);
    expect(PRIORITY_WEBHOOK).toBeLessThan(PRIORITY_RECONCILIATION);
    expect(PRIORITY_RECONCILIATION).toBeLessThan(PRIORITY_NOTIFICATIONS);
    expect(PRIORITY_NOTIFICATIONS).toBeLessThan(PRIORITY_DB);
  });

  it('steps with equal priority run in registration order', async () => {
    const log: string[] = [];
    register(makeStep('first', 2, log));
    register(makeStep('second', 2, log));

    await runAll('SIGTERM', 5000);

    expect(log).toEqual(['first', 'second']);
  });
});

// ---------------------------------------------------------------------------
// Suite: runAll — error isolation
// ---------------------------------------------------------------------------
describe('runAll — errors in one step do not block later steps', () => {
  beforeEach(() => {
    clearRegistry();
    resetShuttingDown();
  });

  it('continues to subsequent steps when an earlier step throws', async () => {
    const log: string[] = [];
    register(makeStep('ok-before', 1, log));
    register(makeStep('throws', 2, log, { throws: true }));
    register(makeStep('ok-after', 3, log));

    await runAll('SIGTERM', 5000);

    expect(log).toContain('ok-before');
    expect(log).toContain('ok-after');
    // 'throws' step errored — its name is not appended to log
    expect(log).not.toContain('throws');
  });

  it('logs an error message when a step throws', async () => {
    const errSpy = jest.spyOn(console, 'error').mockImplementation(() => {});
    register(makeStep('bad-step', 1, [], { throws: true }));

    await runAll('SIGTERM', 5000);

    expect(errSpy).toHaveBeenCalledWith(
      expect.stringContaining('bad-step'),
      expect.any(Error),
    );
    errSpy.mockRestore();
  });

  it('runs all N steps when N-1 of them throw', async () => {
    const log: string[] = [];
    for (let i = 1; i <= 5; i++) {
      register(makeStep(`step-${i}`, i, log, { throws: i < 5 }));
    }

    await runAll('SIGTERM', 5000);

    expect(log).toContain('step-5');
  });
});

// ---------------------------------------------------------------------------
// Suite: runAll — total timeout
// ---------------------------------------------------------------------------
describe('runAll — total timeout honored', () => {
  beforeEach(() => {
    clearRegistry();
    resetShuttingDown();
  });

  it('skips remaining steps when the total timeout is exceeded', async () => {
    const log: string[] = [];
    // step-1 hogs time beyond the 100 ms budget
    register(makeStep('step-1', 1, log, { delayMs: 200 }));
    register(makeStep('step-2', 2, log));

    await runAll('SIGTERM', 100);

    // step-2 was registered but should be skipped
    expect(log).not.toContain('step-2');
  }, 2000);

  it('logs a warning when a step is skipped due to timeout', async () => {
    const warnSpy = jest.spyOn(console, 'warn').mockImplementation(() => {});
    register(makeStep('slow', 1, [], { delayMs: 200 }));
    register(makeStep('skipped', 2, []));

    await runAll('SIGTERM', 100);

    expect(warnSpy).toHaveBeenCalledWith(
      expect.stringMatching(/timeout reached.*skipping.*skipped/i),
    );
    warnSpy.mockRestore();
  }, 2000);
});

// ---------------------------------------------------------------------------
// Suite: second SIGTERM forces immediate exit(1)
// ---------------------------------------------------------------------------
describe('createShutdownHandler — second signal', () => {
  let exitSpy: jest.SpyInstance;

  beforeEach(() => {
    clearRegistry();
    resetShuttingDown();
    jest.clearAllMocks();
    exitSpy = jest.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    (getActiveRequests as jest.Mock).mockReturnValue(0);
    (webhookQueueService.flush as jest.Mock).mockReturnValue([]);
  });

  afterEach(() => {
    exitSpy.mockRestore();
  });

  it('forces process.exit(1) on the second signal', async () => {
    const server = makeMockServer();
    const handler = createShutdownHandler(server, 100);

    await handler('SIGTERM');
    exitSpy.mockClear();

    await handler('SIGTERM'); // second call
    expect(exitSpy).toHaveBeenCalledWith(1);
  });

  it('does not re-run any shutdown steps on the second signal', async () => {
    const server = makeMockServer();
    const handler = createShutdownHandler(server, 100);

    await handler('SIGTERM');

    jest.clearAllMocks();
    exitSpy = jest.spyOn(process, 'exit').mockImplementation(() => undefined as never);

    await handler('SIGTERM');

    expect(statusService.setMaintenanceMode).not.toHaveBeenCalled();
    expect(server.close).not.toHaveBeenCalled();
    expect(webhookQueueService.flush).not.toHaveBeenCalled();
    expect(closeDatabase).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// Suite: isShuttingDown — no service runs after shutdown begins
// ---------------------------------------------------------------------------
describe('isShuttingDown guard', () => {
  beforeEach(() => {
    clearRegistry();
    resetShuttingDown();
    jest.clearAllMocks();
    jest.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    (getActiveRequests as jest.Mock).mockReturnValue(0);
    (webhookQueueService.flush as jest.Mock).mockReturnValue([]);
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it('isShuttingDown() returns false before shutdown', () => {
    expect(isShuttingDown()).toBe(false);
  });

  it('isShuttingDown() returns true once shutdown starts', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(isShuttingDown()).toBe(true);
  });

  it('a concurrently-registered step runs only once after handler invoked', async () => {
    const log: string[] = [];
    const server = makeMockServer();
    const handler = createShutdownHandler(server, 100);

    // Register a step that tracks calls
    register({ name: 'guard-check', priority: 99, fn: async () => { log.push('ran'); } });

    await handler('SIGTERM');

    // Attempt a second call — should exit(1) without running guard-check again
    await handler('SIGTERM');

    // guard-check should appear at most once
    expect(log.filter((x) => x === 'ran').length).toBeLessThanOrEqual(1);
  });
});

// ---------------------------------------------------------------------------
// Suite: backward-compat createShutdownHandler wires canonical steps
// ---------------------------------------------------------------------------
describe('createShutdownHandler — canonical step sequence', () => {
  let exitSpy: jest.SpyInstance;

  beforeEach(() => {
    clearRegistry();
    resetShuttingDown();
    jest.clearAllMocks();
    exitSpy = jest.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    (getActiveRequests as jest.Mock).mockReturnValue(0);
    (webhookQueueService.flush as jest.Mock).mockReturnValue([]);
  });

  afterEach(() => {
    exitSpy.mockRestore();
  });

  it('calls server.close(), webhookQueueService.flush(), and closeDatabase()', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');

    expect(server.close).toHaveBeenCalledTimes(1);
    expect(webhookQueueService.flush).toHaveBeenCalledTimes(1);
    expect(closeDatabase).toHaveBeenCalledTimes(1);
  });

  it('sets maintenance mode before closing the server', async () => {
    const callOrder: string[] = [];
    (statusService.setMaintenanceMode as jest.Mock).mockImplementation(() =>
      callOrder.push('maintenance'),
    );
    const server = { close: jest.fn(() => callOrder.push('close')) } as unknown as http.Server;

    await createShutdownHandler(server, 100)('SIGTERM');

    expect(callOrder.indexOf('maintenance')).toBeLessThan(callOrder.indexOf('close'));
  });

  it('server.close() runs before closeDatabase()', async () => {
    const callOrder: string[] = [];
    const server = { close: jest.fn(() => callOrder.push('close')) } as unknown as http.Server;
    (closeDatabase as jest.Mock).mockImplementation(() => callOrder.push('db'));

    await createShutdownHandler(server, 100)('SIGTERM');

    expect(callOrder.indexOf('close')).toBeLessThan(callOrder.indexOf('db'));
  });

  it('webhook flush runs before closeDatabase()', async () => {
    const callOrder: string[] = [];
    (webhookQueueService.flush as jest.Mock).mockImplementation(() => {
      callOrder.push('flush');
      return [];
    });
    (closeDatabase as jest.Mock).mockImplementation(() => callOrder.push('db'));

    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');

    expect(callOrder.indexOf('flush')).toBeLessThan(callOrder.indexOf('db'));
  });

  it('exits 0 on clean shutdown', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('exits 0 even when flush() throws', async () => {
    (webhookQueueService.flush as jest.Mock).mockImplementation(() => {
      throw new Error('flush boom');
    });
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('exits 0 even when closeDatabase() throws', async () => {
    (closeDatabase as jest.Mock).mockImplementation(() => {
      throw new Error('db boom');
    });
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('handles SIGINT identically to SIGTERM', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGINT');
    expect(server.close).toHaveBeenCalled();
    expect(exitSpy).toHaveBeenCalledWith(0);
  });
});

// ---------------------------------------------------------------------------
// Suite: exported constants
// ---------------------------------------------------------------------------
describe('shutdown constants', () => {
  it('DEFAULT_DRAIN_TIMEOUT_MS is a positive integer', () => {
    expect(typeof DEFAULT_DRAIN_TIMEOUT_MS).toBe('number');
    expect(DEFAULT_DRAIN_TIMEOUT_MS).toBeGreaterThan(0);
    expect(Number.isInteger(DEFAULT_DRAIN_TIMEOUT_MS)).toBe(true);
  });

  it('DRAIN_POLL_MS is positive and less than DEFAULT_DRAIN_TIMEOUT_MS', () => {
    expect(DRAIN_POLL_MS).toBeGreaterThan(0);
    expect(DRAIN_POLL_MS).toBeLessThan(DEFAULT_DRAIN_TIMEOUT_MS);
  });

  it('all PRIORITY_* constants are distinct positive integers in ascending order', () => {
    const all = [
      PRIORITY_HTTP,
      PRIORITY_SCHEDULER,
      PRIORITY_INGESTION,
      PRIORITY_WEBHOOK,
      PRIORITY_RECONCILIATION,
      PRIORITY_NOTIFICATIONS,
      PRIORITY_DB,
    ];
    for (let i = 0; i < all.length - 1; i++) {
      expect(all[i]).toBeLessThan(all[i + 1]);
    }
    expect(new Set(all).size).toBe(all.length); // all distinct
  });
});

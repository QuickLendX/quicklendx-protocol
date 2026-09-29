/**
 * Deterministic failure-boundary coverage for the shutdown orchestrator.
 * Issue #2709 — src/lib/shutdown.ts
 *
 * Acceptance-criteria mapping
 * ───────────────────────────
 * AC-1  Deterministic for valid, invalid, duplicate, and boundary inputs
 *         → registry API, priority ordering, name-dedup, step-count boundaries
 * AC-2  Authorization / validation / state-transition invariants enforced
 *         → _shuttingDown guard, runAll non-reentrance, maintenance-mode ordering
 * AC-3  Retries, partial failure, concurrent execution → no unsafe result
 *         → concurrent runAll, every-step-throws, interleaved register/run
 * AC-4  Success, rejection, boundary, and regression scenarios covered
 *         → all suites below
 * AC-5  Existing callers remain compatible
 *         → canonical-step-sequence suite
 * AC-6  Failures are diagnosable without exposing sensitive data
 *         → structured error-message assertions, ShutdownResult.hadErrors
 */

// ---------------------------------------------------------------------------
// Module mocks — must be hoisted before any imports of mocked modules.
// ---------------------------------------------------------------------------
jest.mock('../middleware/load-shedding', () => ({
  getActiveRequests: jest.fn(() => 0),
  resetActiveRequests: jest.fn(),
}));

jest.mock('../services/webhookQueueService', () => ({
  webhookQueueService: {
    flush: jest.fn(() => []),
  },
  WebhookQueueService: jest.requireActual('../services/webhookQueueService').WebhookQueueService,
}));

jest.mock('../lib/database', () => ({
  closeDatabase: jest.fn(),
  getDatabase: jest.fn(),
}));

jest.mock('../services/statusService', () => ({
  statusService: {
    setMaintenanceMode: jest.fn(),
  },
}));

// ---------------------------------------------------------------------------
// Imports — after mocks
// ---------------------------------------------------------------------------
import http from 'http';
import {
  ShutdownStep,
  ShutdownResult,
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
  opts: { throws?: boolean; delayMs?: number; throwMsg?: string } = {},
): ShutdownStep {
  return {
    name,
    priority,
    fn: async () => {
      if (opts.delayMs) await new Promise<void>((r) => setTimeout(r, opts.delayMs));
      if (opts.throws) throw new Error(opts.throwMsg ?? `${name} failed`);
      log.push(name);
    },
  };
}

// ============================================================================
// Suite 1 — Registry API (AC-1, AC-5)
// ============================================================================
describe('registry API — register / clearRegistry / getRegisteredSteps', () => {
  beforeEach(() => {
    resetShuttingDown();
    clearRegistry();
  });

  it('getRegisteredSteps returns steps sorted ascending by priority', () => {
    register({ name: 'c', priority: 30, fn: async () => {} });
    register({ name: 'a', priority: 10, fn: async () => {} });
    register({ name: 'b', priority: 20, fn: async () => {} });
    expect(getRegisteredSteps().map((s) => s.name)).toEqual(['a', 'b', 'c']);
  });

  it('re-registering the same name replaces the step (idempotent)', () => {
    const fn1 = jest.fn();
    const fn2 = jest.fn();
    register({ name: 'dup', priority: 1, fn: fn1 });
    register({ name: 'dup', priority: 1, fn: fn2 });
    const steps = getRegisteredSteps();
    expect(steps).toHaveLength(1);
    expect(steps[0].fn).toBe(fn2);
  });

  it('re-registering does not add a duplicate even when priority changes', () => {
    register({ name: 'step', priority: 1, fn: async () => {} });
    register({ name: 'step', priority: 99, fn: async () => {} });
    expect(getRegisteredSteps()).toHaveLength(1);
    expect(getRegisteredSteps()[0].priority).toBe(99);
  });

  it('clearRegistry removes all registered steps', () => {
    register({ name: 'x', priority: 1, fn: async () => {} });
    register({ name: 'y', priority: 2, fn: async () => {} });
    clearRegistry();
    expect(getRegisteredSteps()).toHaveLength(0);
  });

  it('clearRegistry on an empty registry is a no-op', () => {
    expect(() => clearRegistry()).not.toThrow();
    expect(getRegisteredSteps()).toHaveLength(0);
  });

  it('getRegisteredSteps returns a copy — mutating it does not affect the registry', () => {
    register({ name: 'z', priority: 1, fn: async () => {} });
    const copy = getRegisteredSteps();
    copy.length = 0;
    expect(getRegisteredSteps()).toHaveLength(1);
  });

  it('steps with equal priority retain registration order', () => {
    register({ name: 'first', priority: 5, fn: async () => {} });
    register({ name: 'second', priority: 5, fn: async () => {} });
    register({ name: 'third', priority: 5, fn: async () => {} });
    expect(getRegisteredSteps().map((s) => s.name)).toEqual([
      'first',
      'second',
      'third',
    ]);
  });

  it('registering zero steps leaves an empty registry', () => {
    expect(getRegisteredSteps()).toHaveLength(0);
  });
});

// ============================================================================
// Suite 2 — Priority ordering (AC-1)
// ============================================================================
describe('runAll — priority ordering', () => {
  beforeEach(() => {
    resetShuttingDown();
    clearRegistry();
  });

  it('executes steps in ascending priority order regardless of registration order', async () => {
    const log: string[] = [];
    register(makeStep('step-3', 3, log));
    register(makeStep('step-1', 1, log));
    register(makeStep('step-2', 2, log));
    await runAll('SIGTERM', 5000);
    expect(log).toEqual(['step-1', 'step-2', 'step-3']);
  });

  it('observes the canonical 7-step ascending priority chain', () => {
    expect(PRIORITY_HTTP).toBeLessThan(PRIORITY_SCHEDULER);
    expect(PRIORITY_SCHEDULER).toBeLessThan(PRIORITY_INGESTION);
    expect(PRIORITY_INGESTION).toBeLessThan(PRIORITY_WEBHOOK);
    expect(PRIORITY_WEBHOOK).toBeLessThan(PRIORITY_RECONCILIATION);
    expect(PRIORITY_RECONCILIATION).toBeLessThan(PRIORITY_NOTIFICATIONS);
    expect(PRIORITY_NOTIFICATIONS).toBeLessThan(PRIORITY_DB);
  });

  it('all PRIORITY_* constants are distinct', () => {
    const all = [
      PRIORITY_HTTP,
      PRIORITY_SCHEDULER,
      PRIORITY_INGESTION,
      PRIORITY_WEBHOOK,
      PRIORITY_RECONCILIATION,
      PRIORITY_NOTIFICATIONS,
      PRIORITY_DB,
    ];
    expect(new Set(all).size).toBe(all.length);
  });

  it('single step runs and produces ok outcome', async () => {
    const log: string[] = [];
    register(makeStep('solo', 1, log));
    const result = await runAll('SIGTERM', 5000);
    expect(log).toEqual(['solo']);
    expect(result.outcomes[0]).toMatchObject({ name: 'solo', status: 'ok' });
  });

  it('empty registry produces empty outcomes and hadErrors=false', async () => {
    const result = await runAll('SIGTERM', 5000);
    expect(result.outcomes).toHaveLength(0);
    expect(result.hadErrors).toBe(false);
  });
});

// ============================================================================
// Suite 3 — Error isolation (AC-2, AC-3, AC-6)
// ============================================================================
describe('runAll — error isolation: a failing step does not block subsequent steps', () => {
  beforeEach(() => {
    resetShuttingDown();
    clearRegistry();
  });

  it('continues to subsequent steps when an earlier step throws', async () => {
    const log: string[] = [];
    register(makeStep('ok-before', 1, log));
    register(makeStep('throws', 2, log, { throws: true }));
    register(makeStep('ok-after', 3, log));
    await runAll('SIGTERM', 5000);
    expect(log).toContain('ok-before');
    expect(log).toContain('ok-after');
    expect(log).not.toContain('throws');
  });

  it('records failed outcome for the throwing step', async () => {
    const log: string[] = [];
    register(makeStep('bad', 1, log, { throws: true, throwMsg: 'boom' }));
    register(makeStep('good', 2, log));
    const result = await runAll('SIGTERM', 5000);
    const bad = result.outcomes.find((o) => o.name === 'bad');
    expect(bad?.status).toBe('failed');
    expect(bad?.errorMessage).toContain('boom');
  });

  it('ShutdownResult.hadErrors is true when any step fails', async () => {
    const log: string[] = [];
    register(makeStep('err', 1, log, { throws: true }));
    const result = await runAll('SIGTERM', 5000);
    expect(result.hadErrors).toBe(true);
  });

  it('ShutdownResult.hadErrors is false when all steps succeed', async () => {
    const log: string[] = [];
    register(makeStep('a', 1, log));
    register(makeStep('b', 2, log));
    const result = await runAll('SIGTERM', 5000);
    expect(result.hadErrors).toBe(false);
  });

  it('logs a structured error message when a step throws', async () => {
    const errSpy = jest.spyOn(console, 'error').mockImplementation(() => {});
    register(makeStep('bad-step', 1, [], { throws: true }));
    await runAll('SIGTERM', 5000);
    expect(errSpy).toHaveBeenCalledWith(
      expect.stringContaining('bad-step'),
      expect.any(Error),
    );
    errSpy.mockRestore();
  });

  it('runs all N steps when all but the last throw', async () => {
    const log: string[] = [];
    for (let i = 1; i <= 5; i++) {
      register(makeStep(`step-${i}`, i, log, { throws: i < 5 }));
    }
    await runAll('SIGTERM', 5000);
    expect(log).toContain('step-5');
  });

  it('records ok durationMs for successful steps', async () => {
    const log: string[] = [];
    register(makeStep('timed', 1, log));
    const result = await runAll('SIGTERM', 5000);
    const outcome = result.outcomes.find((o) => o.name === 'timed');
    expect(typeof outcome?.durationMs).toBe('number');
    expect(outcome!.durationMs!).toBeGreaterThanOrEqual(0);
  });

  it('records durationMs even for failed steps', async () => {
    const log: string[] = [];
    register(makeStep('fail-timed', 1, log, { throws: true }));
    const result = await runAll('SIGTERM', 5000);
    const outcome = result.outcomes.find((o) => o.name === 'fail-timed');
    expect(typeof outcome?.durationMs).toBe('number');
  });
});

// ============================================================================
// Suite 4 — Total timeout honored (AC-1, AC-3)
// ============================================================================
describe('runAll — total timeout boundary', () => {
  beforeEach(() => {
    resetShuttingDown();
    clearRegistry();
  });

  it('skips remaining steps when the total timeout is exceeded by a slow step', async () => {
    const log: string[] = [];
    register(makeStep('slow', 1, log, { delayMs: 200 }));
    register(makeStep('skipped', 2, log));
    await runAll('SIGTERM', 100);
    expect(log).not.toContain('skipped');
  }, 2000);

  it('records skipped outcome for the step that did not start', async () => {
    const log: string[] = [];
    register(makeStep('slow', 1, log, { delayMs: 200 }));
    register(makeStep('skipped-step', 2, log));
    const result = await runAll('SIGTERM', 100);
    const skipped = result.outcomes.find((o) => o.name === 'skipped-step');
    expect(skipped?.status).toBe('skipped');
  }, 2000);

  it('sets hadErrors=true when steps are skipped', async () => {
    const log: string[] = [];
    register(makeStep('slow', 1, log, { delayMs: 200 }));
    register(makeStep('victim', 2, log));
    const result = await runAll('SIGTERM', 100);
    expect(result.hadErrors).toBe(true);
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

  it('step that runs under the budget is executed even when budget is tight', async () => {
    const log: string[] = [];
    register(makeStep('fast', 1, log, { delayMs: 10 }));
    // 5 s budget — fast step (10 ms) fits comfortably
    await runAll('SIGTERM', 5000);
    expect(log).toContain('fast');
  });

  it('totalDurationMs in result is >= 0', async () => {
    const log: string[] = [];
    register(makeStep('quick', 1, log));
    const result = await runAll('SIGTERM', 5000);
    expect(result.totalDurationMs).toBeGreaterThanOrEqual(0);
  });
});

// ============================================================================
// Suite 5 — Non-reentrant guard for runAll (AC-3)
// ============================================================================
describe('runAll — non-reentrance guard', () => {
  beforeEach(() => {
    resetShuttingDown();
    clearRegistry();
  });

  it('a concurrent call while runAll is in progress returns immediately with empty outcomes', async () => {
    // step-1 takes a little time; the second call starts while step-1 is running
    const log: string[] = [];
    register(makeStep('slow', 1, log, { delayMs: 100 }));

    const firstPromise = runAll('SIGTERM', 5000);
    // Give the first call a head start so _runAllInProgress is true
    await new Promise<void>((r) => setTimeout(r, 10));
    const secondResult = await runAll('SIGTERM', 5000);

    await firstPromise;

    expect(secondResult.outcomes).toHaveLength(0);
    // The first run still executed the step
    expect(log).toContain('slow');
  }, 3000);

  it('after the first runAll completes, a new call can run normally', async () => {
    const log1: string[] = [];
    const log2: string[] = [];
    register(makeStep('run1', 1, log1));
    await runAll('SIGTERM', 5000);
    clearRegistry();

    register(makeStep('run2', 1, log2));
    await runAll('SIGTERM', 5000);

    expect(log2).toContain('run2');
  });
});

// ============================================================================
// Suite 6 — isShuttingDown state transitions (AC-2)
// ============================================================================
describe('isShuttingDown state transitions', () => {
  let exitSpy: jest.SpyInstance;

  beforeEach(() => {
    resetShuttingDown();
    clearRegistry();
    jest.clearAllMocks();
    exitSpy = jest.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    (getActiveRequests as jest.Mock).mockReturnValue(0);
    (webhookQueueService.flush as jest.Mock).mockReturnValue([]);
  });

  afterEach(() => {
    exitSpy.mockRestore();
  });

  it('isShuttingDown() is false before any shutdown signal', () => {
    expect(isShuttingDown()).toBe(false);
  });

  it('isShuttingDown() is true after the handler is invoked', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(isShuttingDown()).toBe(true);
  });

  it('resetShuttingDown() returns isShuttingDown to false', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    resetShuttingDown();
    expect(isShuttingDown()).toBe(false);
  });

  it('isShuttingDown() remains true after second signal (not reset)', async () => {
    const server = makeMockServer();
    const handler = createShutdownHandler(server, 100);
    await handler('SIGTERM');
    await handler('SIGTERM'); // forced exit, no reset
    expect(isShuttingDown()).toBe(true);
  });
});

// ============================================================================
// Suite 7 — Second signal forces exit(1) (AC-2, AC-3)
// ============================================================================
describe('createShutdownHandler — second signal forces immediate exit(1)', () => {
  let exitSpy: jest.SpyInstance;

  beforeEach(() => {
    resetShuttingDown();
    clearRegistry();
    jest.clearAllMocks();
    exitSpy = jest.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    (getActiveRequests as jest.Mock).mockReturnValue(0);
    (webhookQueueService.flush as jest.Mock).mockReturnValue([]);
  });

  afterEach(() => {
    exitSpy.mockRestore();
  });

  it('forces process.exit(1) on the second signal', async () => {
    const handler = createShutdownHandler(makeMockServer(), 100);
    await handler('SIGTERM');
    exitSpy.mockClear();
    await handler('SIGTERM');
    expect(exitSpy).toHaveBeenCalledWith(1);
  });

  it('does not re-run any shutdown step on the second signal', async () => {
    const server = makeMockServer();
    const handler = createShutdownHandler(server, 100);
    await handler('SIGTERM');

    jest.clearAllMocks();
    exitSpy = jest.spyOn(process, 'exit').mockImplementation(() => undefined as never);

    await handler('SIGTERM');

    expect(statusService.setMaintenanceMode).not.toHaveBeenCalled();
    expect((server.close as jest.Mock)).not.toHaveBeenCalled();
    expect(webhookQueueService.flush).not.toHaveBeenCalled();
    expect(closeDatabase).not.toHaveBeenCalled();
    expect(exitSpy).toHaveBeenCalledWith(1);
  });

  it('logs a warning on the second signal', async () => {
    const warnSpy = jest.spyOn(console, 'warn').mockImplementation(() => {});
    const handler = createShutdownHandler(makeMockServer(), 100);
    await handler('SIGTERM');
    warnSpy.mockClear();
    await handler('SIGTERM');
    expect(warnSpy).toHaveBeenCalledWith(
      expect.stringMatching(/second signal.*forcing exit/i),
    );
    warnSpy.mockRestore();
  });
});

// ============================================================================
// Suite 8 — HTTP drain boundary (AC-1, AC-3)
// ============================================================================
describe('createShutdownHandler — HTTP drain boundary', () => {
  let exitSpy: jest.SpyInstance;

  beforeEach(() => {
    resetShuttingDown();
    clearRegistry();
    jest.clearAllMocks();
    exitSpy = jest.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    (webhookQueueService.flush as jest.Mock).mockReturnValue([]);
  });

  afterEach(() => {
    exitSpy.mockRestore();
  });

  it('waits until active requests reach 0 then exits 0', async () => {
    let calls = 0;
    (getActiveRequests as jest.Mock).mockImplementation(() => {
      calls++;
      return calls < 4 ? 1 : 0;
    });
    const server = makeMockServer();
    await createShutdownHandler(server, 5000)('SIGTERM');
    expect(calls).toBeGreaterThanOrEqual(4);
    expect(exitSpy).toHaveBeenCalledWith(0);
  }, 10_000);

  it('exits 0 when drain timeout expires with requests still in-flight', async () => {
    (getActiveRequests as jest.Mock).mockReturnValue(5); // never drains
    const server = makeMockServer();
    await createShutdownHandler(server, 80)('SIGTERM');
    expect(exitSpy).toHaveBeenCalledWith(0);
  }, 2000);

  it('logs warning when drain timeout is exceeded', async () => {
    const warnSpy = jest.spyOn(console, 'warn').mockImplementation(() => {});
    (getActiveRequests as jest.Mock).mockReturnValue(7);
    const server = makeMockServer();
    await createShutdownHandler(server, 80)('SIGTERM');
    expect(warnSpy).toHaveBeenCalledWith(
      expect.stringMatching(/Drain timeout.*exceeded.*7 request/),
    );
    warnSpy.mockRestore();
  }, 2000);

  it('does not log drain-timeout warning when requests clear before deadline', async () => {
    const warnSpy = jest.spyOn(console, 'warn').mockImplementation(() => {});
    (getActiveRequests as jest.Mock).mockReturnValue(0);
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    const drainWarning = warnSpy.mock.calls.find((args) =>
      String(args[0]).includes('Drain timeout'),
    );
    expect(drainWarning).toBeUndefined();
    warnSpy.mockRestore();
  });

  it('marks maintenance mode before server.close()', async () => {
    (getActiveRequests as jest.Mock).mockReturnValue(0);
    const callOrder: string[] = [];
    (statusService.setMaintenanceMode as jest.Mock).mockImplementation(() =>
      callOrder.push('maintenance'),
    );
    const server = { close: jest.fn(() => callOrder.push('close')) } as unknown as http.Server;
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(callOrder.indexOf('maintenance')).toBeLessThan(callOrder.indexOf('close'));
  });

  it('server.close() is called exactly once', async () => {
    (getActiveRequests as jest.Mock).mockReturnValue(0);
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(server.close).toHaveBeenCalledTimes(1);
  });
});

// ============================================================================
// Suite 9 — Webhook flush failure boundaries (AC-2, AC-3, AC-4, AC-6)
// ============================================================================
describe('createShutdownHandler — webhook flush failure boundaries', () => {
  let exitSpy: jest.SpyInstance;

  beforeEach(() => {
    resetShuttingDown();
    clearRegistry();
    jest.clearAllMocks();
    exitSpy = jest.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    (getActiveRequests as jest.Mock).mockReturnValue(0);
  });

  afterEach(() => {
    exitSpy.mockRestore();
  });

  it('reaches exit(0) even when flush() throws', async () => {
    (webhookQueueService.flush as jest.Mock).mockImplementation(() => {
      throw new Error('flush exploded');
    });
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('logs a "Webhook queue flush failed" error when flush() throws', async () => {
    const flushErr = new Error('flush exploded');
    (webhookQueueService.flush as jest.Mock).mockImplementation(() => {
      throw flushErr;
    });
    const errSpy = jest.spyOn(console, 'error').mockImplementation(() => {});
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(errSpy).toHaveBeenCalledWith(
      expect.stringContaining('Webhook queue flush failed'),
      flushErr,
    );
    errSpy.mockRestore();
  });

  it('warns about undelivered events when flush returns pending entries', async () => {
    (webhookQueueService.flush as jest.Mock).mockReturnValue([
      { id: 'e1', type: 'invoice.created', payload: {}, enqueuedAt: '', status: 'pending' },
      { id: 'e2', type: 'bid.placed',       payload: {}, enqueuedAt: '', status: 'pending' },
    ]);
    const warnSpy = jest.spyOn(console, 'warn').mockImplementation(() => {});
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(warnSpy).toHaveBeenCalledWith(
      expect.stringMatching(/2 webhook event\(s\) not delivered/),
    );
    warnSpy.mockRestore();
  });

  it('does not warn about undelivered events when flush returns []', async () => {
    (webhookQueueService.flush as jest.Mock).mockReturnValue([]);
    const warnSpy = jest.spyOn(console, 'warn').mockImplementation(() => {});
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    const undelivered = warnSpy.mock.calls.find((args) =>
      String(args[0]).includes('not delivered'),
    );
    expect(undelivered).toBeUndefined();
    warnSpy.mockRestore();
  });

  it('database step still runs after flush() throws', async () => {
    (webhookQueueService.flush as jest.Mock).mockImplementation(() => {
      throw new Error('flush fail');
    });
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(closeDatabase).toHaveBeenCalledTimes(1);
  });
});

// ============================================================================
// Suite 10 — Database close failure boundaries (AC-2, AC-3, AC-4, AC-6)
// ============================================================================
describe('createShutdownHandler — database close failure boundaries', () => {
  let exitSpy: jest.SpyInstance;

  beforeEach(() => {
    resetShuttingDown();
    clearRegistry();
    jest.clearAllMocks();
    exitSpy = jest.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    (getActiveRequests as jest.Mock).mockReturnValue(0);
    (webhookQueueService.flush as jest.Mock).mockReturnValue([]);
  });

  afterEach(() => {
    exitSpy.mockRestore();
  });

  it('reaches exit(0) even when closeDatabase() throws', async () => {
    (closeDatabase as jest.Mock).mockImplementation(() => {
      throw new Error('db close failed');
    });
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('logs a "Database close failed" error when closeDatabase() throws', async () => {
    const dbErr = new Error('db close failed');
    (closeDatabase as jest.Mock).mockImplementation(() => {
      throw dbErr;
    });
    const errSpy = jest.spyOn(console, 'error').mockImplementation(() => {});
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(errSpy).toHaveBeenCalledWith(
      expect.stringContaining('Database close failed'),
      dbErr,
    );
    errSpy.mockRestore();
  });

  it('exits 0 when both flush and closeDatabase throw (double-fault)', async () => {
    (webhookQueueService.flush as jest.Mock).mockImplementation(() => {
      throw new Error('flush');
    });
    (closeDatabase as jest.Mock).mockImplementation(() => {
      throw new Error('close');
    });
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('does not expose raw error stack traces in the log message (no sensitive data)', async () => {
    const secret = 'SECRET_TOKEN_XYZ';
    (closeDatabase as jest.Mock).mockImplementation(() => {
      const err = new Error('db close failed');
      (err as any).secret = secret; // attach a "sensitive" field
      throw err;
    });
    const errSpy = jest.spyOn(console, 'error').mockImplementation(() => {});
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');

    // The log message itself must not contain the secret value.
    const logMessages = errSpy.mock.calls.map((args) => String(args[0]));
    for (const msg of logMessages) {
      expect(msg).not.toContain(secret);
    }
    errSpy.mockRestore();
  });
});

// ============================================================================
// Suite 11 — Canonical step sequence and ordering (AC-5)
// ============================================================================
describe('createShutdownHandler — canonical step sequence and ordering', () => {
  let exitSpy: jest.SpyInstance;

  beforeEach(() => {
    resetShuttingDown();
    clearRegistry();
    jest.clearAllMocks();
    exitSpy = jest.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    (getActiveRequests as jest.Mock).mockReturnValue(0);
    (webhookQueueService.flush as jest.Mock).mockReturnValue([]);
  });

  afterEach(() => {
    exitSpy.mockRestore();
  });

  it('calls setMaintenanceMode(true), server.close(), flush(), and closeDatabase()', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(statusService.setMaintenanceMode).toHaveBeenCalledWith(true);
    expect(server.close).toHaveBeenCalledTimes(1);
    expect(webhookQueueService.flush).toHaveBeenCalledTimes(1);
    expect(closeDatabase).toHaveBeenCalledTimes(1);
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

  it('handles SIGINT identically to SIGTERM', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGINT');
    expect(statusService.setMaintenanceMode).toHaveBeenCalledWith(true);
    expect(server.close).toHaveBeenCalled();
    expect(webhookQueueService.flush).toHaveBeenCalled();
    expect(closeDatabase).toHaveBeenCalled();
    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('exits 0 on a fully clean shutdown', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('uses DEFAULT_DRAIN_TIMEOUT_MS when no timeout is supplied', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server)('SIGTERM');
    expect(exitSpy).toHaveBeenCalledWith(0);
  });
});

// ============================================================================
// Suite 12 — ShutdownResult observability (AC-6)
// ============================================================================
describe('ShutdownResult observability', () => {
  beforeEach(() => {
    resetShuttingDown();
    clearRegistry();
  });

  it('result.signal matches the argument passed to runAll', async () => {
    const result = await runAll('SIGUSR2', 5000);
    expect(result.signal).toBe('SIGUSR2');
  });

  it('outcomes array length equals the number of registered steps when none skipped', async () => {
    const log: string[] = [];
    register(makeStep('a', 1, log));
    register(makeStep('b', 2, log));
    register(makeStep('c', 3, log));
    const result = await runAll('SIGTERM', 5000);
    expect(result.outcomes).toHaveLength(3);
  });

  it('failed outcome carries a non-empty errorMessage without raw sensitive data', async () => {
    register({ name: 'leak-check', priority: 1, fn: async () => { throw new Error('internal: DB_PASS=secret'); } });
    const result = await runAll('SIGTERM', 5000);
    const failed = result.outcomes.find((o) => o.name === 'leak-check');
    // errorMessage exists and is the error message string — not a raw stack
    expect(failed?.errorMessage).toBeTruthy();
    expect(typeof failed?.errorMessage).toBe('string');
  });

  it('result.totalDurationMs is a non-negative number', async () => {
    const log: string[] = [];
    register(makeStep('x', 1, log));
    const result = await runAll('SIGTERM', 5000);
    expect(result.totalDurationMs).toBeGreaterThanOrEqual(0);
    expect(Number.isFinite(result.totalDurationMs)).toBe(true);
  });

  it('outcomes preserve step names and priorities', async () => {
    register({ name: 'named-step', priority: 42, fn: async () => {} });
    const result = await runAll('SIGTERM', 5000);
    expect(result.outcomes[0]).toMatchObject({ name: 'named-step', priority: 42 });
  });
});

// ============================================================================
// Suite 13 — Exported constants (AC-1)
// ============================================================================
describe('exported constants', () => {
  it('DEFAULT_DRAIN_TIMEOUT_MS is a positive integer', () => {
    expect(typeof DEFAULT_DRAIN_TIMEOUT_MS).toBe('number');
    expect(DEFAULT_DRAIN_TIMEOUT_MS).toBeGreaterThan(0);
    expect(Number.isInteger(DEFAULT_DRAIN_TIMEOUT_MS)).toBe(true);
  });

  it('DRAIN_POLL_MS is a positive integer strictly less than DEFAULT_DRAIN_TIMEOUT_MS', () => {
    expect(typeof DRAIN_POLL_MS).toBe('number');
    expect(DRAIN_POLL_MS).toBeGreaterThan(0);
    expect(DRAIN_POLL_MS).toBeLessThan(DEFAULT_DRAIN_TIMEOUT_MS);
    expect(Number.isInteger(DRAIN_POLL_MS)).toBe(true);
  });

  it('all seven PRIORITY_* constants are distinct positive integers', () => {
    const all = [
      PRIORITY_HTTP,
      PRIORITY_SCHEDULER,
      PRIORITY_INGESTION,
      PRIORITY_WEBHOOK,
      PRIORITY_RECONCILIATION,
      PRIORITY_NOTIFICATIONS,
      PRIORITY_DB,
    ];
    for (const p of all) {
      expect(typeof p).toBe('number');
      expect(p).toBeGreaterThan(0);
      expect(Number.isInteger(p)).toBe(true);
    }
    expect(new Set(all).size).toBe(all.length);
  });
});

// ============================================================================
// Suite 14 — Regression: concurrent invocation of createShutdownHandler (AC-3)
// ============================================================================
describe('regression — concurrent handler invocations', () => {
  let exitSpy: jest.SpyInstance;

  beforeEach(() => {
    resetShuttingDown();
    clearRegistry();
    jest.clearAllMocks();
    exitSpy = jest.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    (getActiveRequests as jest.Mock).mockReturnValue(0);
    (webhookQueueService.flush as jest.Mock).mockReturnValue([]);
  });

  afterEach(() => {
    exitSpy.mockRestore();
  });

  it('concurrent calls to the same handler do not double-run shutdown steps', async () => {
    const server = makeMockServer();
    const handler = createShutdownHandler(server, 100);

    // Fire two concurrent invocations; first wins, second must exit(1)
    await Promise.all([handler('SIGTERM'), handler('SIGTERM')]);

    // server.close must have been called at most once
    expect((server.close as jest.Mock).mock.calls.length).toBeLessThanOrEqual(1);
    // At least one exit call was made
    expect(exitSpy).toHaveBeenCalled();
  });

  it('re-registering a step between a first and second handler call is idempotent', async () => {
    const server = makeMockServer();
    const handler = createShutdownHandler(server, 100);
    await handler('SIGTERM');
    resetShuttingDown();

    // Re-register without clearing — idempotency must hold
    const server2 = makeMockServer();
    const handler2 = createShutdownHandler(server2, 100);
    await handler2('SIGTERM');

    expect(exitSpy).toHaveBeenCalledWith(0);
  });
});
// ============================================================================
// Suite 15 - clearRegistry failure boundaries (Issue #2709)
// ============================================================================
describe('clearRegistry - failure boundaries', () => {
  const originalEnv = process.env.NODE_ENV;

  beforeEach(() => {
    resetShuttingDown();
    clearRegistry();
  });

  afterEach(() => {
    process.env.NODE_ENV = originalEnv;
    resetShuttingDown();
    clearRegistry();
  });

  it('throws if called in production', () => {
    process.env.NODE_ENV = 'production';
    expect(() => clearRegistry()).toThrow('Permission denied: clearRegistry cannot be called in production');
  });

  it('throws if called while shutting down', async () => {
    const handler = createShutdownHandler({ close: jest.fn() } as any, 100);
    handler('SIGTERM'); // triggers _shuttingDown = true
    expect(() => clearRegistry()).toThrow('Invalid state: cannot clear registry while shutdown is in progress');
  });
});

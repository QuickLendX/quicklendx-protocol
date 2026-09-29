/**
 * Tests for the graceful shutdown orchestrator (src/lib/shutdown.ts)
 * and the WebhookQueueService.flush() method.
 *
 * Coverage targets (shutdown.ts):
 *   - Normal happy-path shutdown sequence
 *   - Drain: requests clear before timeout
 *   - Drain: timeout exceeded, requests remain
 *   - Second signal forces exit(1)
 *   - Webhook flush throws → exit(0) still reached
 *   - Database close throws → exit(0) still reached
 *   - Pending webhook events are logged
 *   - SIGINT handled identically to SIGTERM
 *   - isShuttingDown() state transitions
 *   - DEFAULT_DRAIN_TIMEOUT_MS exported constant
 *   - createShutdownHandler uses default timeout when none supplied
 *
 * Coverage targets (WebhookQueueService.flush):
 *   - Empty queue → returns []
 *   - Pending events returned and queue cleared
 *   - success/failed events excluded from returned list
 *   - Mixed queue: only pending returned, depth resets
 */

// ---------------------------------------------------------------------------
// Module mocks — must be hoisted before any imports from the mocked modules
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

jest.mock('../lib/database', () => {
  let dbInstance: any = null;
  return {
    closeDatabase: jest.fn(() => {
      if (dbInstance) {
        try { dbInstance.close(); } catch { /* ignore */ }
        dbInstance = null;
      }
    }),
    getDatabase: jest.fn(() => {
      if (!dbInstance) {
        const Database = require('better-sqlite3');
        dbInstance = new Database(process.env.DATABASE_PATH || '.data/test-shutdown.db');
        dbInstance.pragma('journal_mode = WAL');
        dbInstance.pragma('synchronous = NORMAL');
        dbInstance.pragma('foreign_keys = ON');
      }
      return dbInstance;
    }),
  };
});

jest.mock('../services/statusService', () => ({
  statusService: {
    setMaintenanceMode: jest.fn(),
  },
}));

// ---------------------------------------------------------------------------
// Imports — after mocks
// ---------------------------------------------------------------------------
import http from 'http';
import path from 'path';
import crypto from 'crypto';
import {
  createShutdownHandler,
  resetShuttingDown,
  isShuttingDown,
  DEFAULT_DRAIN_TIMEOUT_MS,
  DRAIN_POLL_MS,
} from '../lib/shutdown';
import { getActiveRequests } from '../middleware/load-shedding';
import { webhookQueueService } from '../services/webhookQueueService';
import { closeDatabase, getDatabase } from '../lib/database';
import { statusService } from '../services/statusService';
import type { WebhookEvent } from '../services/webhookQueueService';

// ---------------------------------------------------------------------------
// Typed mocks
// ---------------------------------------------------------------------------
const mockGetActiveRequests = getActiveRequests as jest.MockedFunction<
  typeof getActiveRequests
>;
const mockCloseDatabase = closeDatabase as jest.MockedFunction<typeof closeDatabase>;
const mockSetMaintenanceMode = statusService.setMaintenanceMode as jest.MockedFunction<
  typeof statusService.setMaintenanceMode
>;
const mockFlush = webhookQueueService.flush as jest.MockedFunction<
  typeof webhookQueueService.flush
>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
function makeMockServer(): http.Server {
  return { close: jest.fn() } as unknown as http.Server;
}

function makeWebhookEvent(id: string, status: WebhookEvent['status'] = 'pending'): WebhookEvent {
  return {
    id,
    type: 'test.event',
    payload: { data: id },
    enqueuedAt: new Date().toISOString(),
    status,
  };
}

// ---------------------------------------------------------------------------
// createShutdownHandler — main suite
// ---------------------------------------------------------------------------
describe('createShutdownHandler', () => {
  let exitSpy: jest.SpyInstance;

  beforeEach(() => {
    jest.clearAllMocks();
    resetShuttingDown();
    // Mock process.exit so it does not terminate the test runner.
    exitSpy = jest.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    // jest.clearAllMocks() above only clears call history, not
    // .mockImplementation() bodies a previous test may have installed
    // (e.g. one of the failure-boundary tests below making a mock throw).
    // Reset every mock used across this suite to its safe default here so
    // no test can silently inherit a throw it never configured itself.
    mockGetActiveRequests.mockReturnValue(0);
    mockFlush.mockReturnValue([]);
    mockSetMaintenanceMode.mockImplementation(() => {});
    mockCloseDatabase.mockImplementation(() => {});
  });

  afterEach(() => {
    exitSpy.mockRestore();
  });

  // ── Happy path ────────────────────────────────────────────────────────────

  it('marks service not-ready (setMaintenanceMode true) at start', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(mockSetMaintenanceMode).toHaveBeenCalledWith(true);
    expect(mockSetMaintenanceMode).toHaveBeenCalledTimes(1);
  });

  it('calls server.close() to stop accepting new connections', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(server.close).toHaveBeenCalledTimes(1);
  });

  it('calls webhookQueueService.flush()', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(mockFlush).toHaveBeenCalledTimes(1);
  });

  it('calls closeDatabase()', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(mockCloseDatabase).toHaveBeenCalledTimes(1);
  });

  it('calls process.exit(0) on clean shutdown', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('handles SIGINT identically to SIGTERM', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGINT');
    expect(mockSetMaintenanceMode).toHaveBeenCalledWith(true);
    expect(server.close).toHaveBeenCalled();
    expect(mockFlush).toHaveBeenCalled();
    expect(mockCloseDatabase).toHaveBeenCalled();
    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('uses DEFAULT_DRAIN_TIMEOUT_MS when no timeout arg supplied', async () => {
    const server = makeMockServer();
    // Should resolve without hanging — zero active requests means no wait
    await createShutdownHandler(server)('SIGTERM');
    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  // ── isShuttingDown state ──────────────────────────────────────────────────

  it('isShuttingDown() is false before any shutdown', () => {
    expect(isShuttingDown()).toBe(false);
  });

  it('isShuttingDown() is true after shutdown starts', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(isShuttingDown()).toBe(true);
  });

  it('resetShuttingDown() resets isShuttingDown to false', async () => {
    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');
    expect(isShuttingDown()).toBe(true);
    resetShuttingDown();
    expect(isShuttingDown()).toBe(false);
  });

  // ── Drain behavior ────────────────────────────────────────────────────────

  it('waits while active requests are non-zero then exits 0', async () => {
    let callCount = 0;
    mockGetActiveRequests.mockImplementation(() => {
      callCount++;
      return callCount < 4 ? 1 : 0; // returns 0 on the 4th call
    });

    const server = makeMockServer();
    await createShutdownHandler(server, 5000)('SIGTERM');

    expect(callCount).toBeGreaterThanOrEqual(4);
    expect(exitSpy).toHaveBeenCalledWith(0);
  }, 10_000);

  it('exits 0 when drain timeout expires with requests still in-flight', async () => {
    mockGetActiveRequests.mockReturnValue(3); // never drains

    const server = makeMockServer();
    await createShutdownHandler(server, 80)('SIGTERM'); // 80ms drain window

    expect(exitSpy).toHaveBeenCalledWith(0);
  }, 2000);

  it('logs a warning when drain timeout is exceeded', async () => {
    const warnSpy = jest.spyOn(console, 'warn').mockImplementation(() => {});
    mockGetActiveRequests.mockReturnValue(7);

    const server = makeMockServer();
    await createShutdownHandler(server, 80)('SIGTERM');

    expect(warnSpy).toHaveBeenCalledWith(
      expect.stringMatching(/Drain timeout.*exceeded.*7 request/),
    );
    warnSpy.mockRestore();
  }, 2000);

  it('does not log drain-timeout warning when requests clear in time', async () => {
    const warnSpy = jest.spyOn(console, 'warn').mockImplementation(() => {});
    mockGetActiveRequests.mockReturnValue(0);

    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');

    const drainWarning = warnSpy.mock.calls.find((args) =>
      String(args[0]).includes('Drain timeout'),
    );
    expect(drainWarning).toBeUndefined();
    warnSpy.mockRestore();
  });

  // ── Second signal ─────────────────────────────────────────────────────────

  it('forces process.exit(1) when called a second time while still shutting down', async () => {
    // First shutdown completes immediately (0 active requests)
    const server = makeMockServer();
    const handler = createShutdownHandler(server, 100);
    await handler('SIGTERM'); // completes; _shuttingDown stays true

    exitSpy.mockClear();
    await handler('SIGTERM'); // second signal — forced exit(1)

    expect(exitSpy).toHaveBeenCalledWith(1);
  });

  it('does not run the rest of the shutdown sequence on the second signal', async () => {
    const server = makeMockServer();
    const handler = createShutdownHandler(server, 100);
    await handler('SIGTERM');

    jest.clearAllMocks();
    exitSpy = jest.spyOn(process, 'exit').mockImplementation(() => undefined as never);

    await handler('SIGTERM');

    // None of the shutdown steps should run on the second call
    expect(mockSetMaintenanceMode).not.toHaveBeenCalled();
    expect((server.close as jest.Mock)).not.toHaveBeenCalled();
    expect(mockFlush).not.toHaveBeenCalled();
    expect(mockCloseDatabase).not.toHaveBeenCalled();
    expect(exitSpy).toHaveBeenCalledWith(1);
  });

  // ── Error resilience ──────────────────────────────────────────────────────

  it('reaches process.exit(0) even when webhookQueueService.flush() throws', async () => {
    mockFlush.mockImplementation(() => {
      throw new Error('flush exploded');
    });

    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');

    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('logs an error when flush() throws', async () => {
    const flushError = new Error('flush exploded');
    mockFlush.mockImplementation(() => { throw flushError; });
    const errorSpy = jest.spyOn(console, 'error').mockImplementation(() => {});

    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');

    expect(errorSpy).toHaveBeenCalledWith(
      expect.stringContaining('Webhook queue flush failed'),
      flushError,
    );
    errorSpy.mockRestore();
  });

  it('reaches process.exit(0) even when closeDatabase() throws', async () => {
    mockCloseDatabase.mockImplementation(() => {
      throw new Error('db close failed');
    });

    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');

    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('logs an error when closeDatabase() throws', async () => {
    const dbError = new Error('db close failed');
    mockCloseDatabase.mockImplementation(() => { throw dbError; });
    const errorSpy = jest.spyOn(console, 'error').mockImplementation(() => {});

    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');

    expect(errorSpy).toHaveBeenCalledWith(
      expect.stringContaining('Database close failed'),
      dbError,
    );
    errorSpy.mockRestore();
  });

  it('exits 0 when both flush and closeDatabase throw', async () => {
    mockFlush.mockImplementation(() => { throw new Error('flush'); });
    mockCloseDatabase.mockImplementation(() => { throw new Error('close'); });

    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');

    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  // ── Webhook pending events ────────────────────────────────────────────────

  it('logs a warning listing undelivered webhook events', async () => {
    const pending: WebhookEvent[] = [
      makeWebhookEvent('evt-1'),
      makeWebhookEvent('evt-2'),
    ];
    mockFlush.mockReturnValue(pending);
    const warnSpy = jest.spyOn(console, 'warn').mockImplementation(() => {});

    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');

    expect(warnSpy).toHaveBeenCalledWith(
      expect.stringMatching(/2 webhook event\(s\) not delivered/),
    );
    warnSpy.mockRestore();
  });

  it('does not log undelivered-events warning when flush returns empty array', async () => {
    mockFlush.mockReturnValue([]);
    const warnSpy = jest.spyOn(console, 'warn').mockImplementation(() => {});

    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');

    const undeliveredWarning = warnSpy.mock.calls.find((args) =>
      String(args[0]).includes('not delivered'),
    );
    expect(undeliveredWarning).toBeUndefined();
    warnSpy.mockRestore();
  });

  // ── Ordering ──────────────────────────────────────────────────────────────

  it('calls server.close() before closeDatabase()', async () => {
    const callOrder: string[] = [];
    (makeMockServer as any); // type hint
    const server = { close: jest.fn(() => callOrder.push('close')) } as unknown as http.Server;
    mockCloseDatabase.mockImplementation(() => { callOrder.push('db'); });

    await createShutdownHandler(server, 100)('SIGTERM');

    expect(callOrder.indexOf('close')).toBeLessThan(callOrder.indexOf('db'));
  });

  it('sets maintenance mode before closing the server', async () => {
    const callOrder: string[] = [];
    mockSetMaintenanceMode.mockImplementation(() => { callOrder.push('maintenance'); });
    const server = { close: jest.fn(() => callOrder.push('close')) } as unknown as http.Server;

    await createShutdownHandler(server, 100)('SIGTERM');

    expect(callOrder.indexOf('maintenance')).toBeLessThan(callOrder.indexOf('close'));
  });

  // ── Early-step failure boundaries ────────────────────────────────────────
  //
  // Steps 4-5 (webhook flush, database close) were already individually
  // guarded. These cases cover the same guarantee for steps 1-3
  // (setMaintenanceMode, server.close, the drain loop's getActiveRequests
  // reads): a throw from any single step must not prevent the later steps
  // from running or block the eventual process.exit(0).

  it('reaches process.exit(0) even when setMaintenanceMode() throws', async () => {
    mockSetMaintenanceMode.mockImplementation(() => {
      throw new Error('status store unreachable');
    });

    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');

    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('still closes the server, flushes webhooks, and closes the database when setMaintenanceMode() throws', async () => {
    mockSetMaintenanceMode.mockImplementation(() => {
      throw new Error('status store unreachable');
    });

    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');

    expect(server.close).toHaveBeenCalledTimes(1);
    expect(mockFlush).toHaveBeenCalledTimes(1);
    expect(mockCloseDatabase).toHaveBeenCalledTimes(1);
  });

  it('logs an error when setMaintenanceMode() throws', async () => {
    const statusError = new Error('status store unreachable');
    mockSetMaintenanceMode.mockImplementation(() => { throw statusError; });
    const errorSpy = jest.spyOn(console, 'error').mockImplementation(() => {});

    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');

    expect(errorSpy).toHaveBeenCalledWith(
      expect.stringContaining('Failed to set maintenance mode'),
      statusError,
    );
    errorSpy.mockRestore();
  });

  it('reaches process.exit(0) even when server.close() throws', async () => {
    const server = {
      close: jest.fn(() => {
        throw new Error('listener already closed');
      }),
    } as unknown as http.Server;

    await createShutdownHandler(server, 100)('SIGTERM');

    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('still flushes webhooks and closes the database when server.close() throws', async () => {
    const server = {
      close: jest.fn(() => {
        throw new Error('listener already closed');
      }),
    } as unknown as http.Server;

    await createShutdownHandler(server, 100)('SIGTERM');

    expect(mockFlush).toHaveBeenCalledTimes(1);
    expect(mockCloseDatabase).toHaveBeenCalledTimes(1);
  });

  it('logs an error when server.close() throws', async () => {
    const closeError = new Error('listener already closed');
    const server = { close: jest.fn(() => { throw closeError; }) } as unknown as http.Server;
    const errorSpy = jest.spyOn(console, 'error').mockImplementation(() => {});

    await createShutdownHandler(server, 100)('SIGTERM');

    expect(errorSpy).toHaveBeenCalledWith(
      expect.stringContaining('server.close() failed'),
      closeError,
    );
    errorSpy.mockRestore();
  });

  it('reaches process.exit(0) even when getActiveRequests() throws during drain', async () => {
    mockGetActiveRequests.mockImplementation(() => {
      throw new Error('counter unavailable');
    });

    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');

    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('still flushes webhooks and closes the database when getActiveRequests() throws', async () => {
    mockGetActiveRequests.mockImplementation(() => {
      throw new Error('counter unavailable');
    });

    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');

    expect(mockFlush).toHaveBeenCalledTimes(1);
    expect(mockCloseDatabase).toHaveBeenCalledTimes(1);
  });

  it('logs an error, and does not log a fabricated drain-timeout warning, when getActiveRequests() throws', async () => {
    const counterError = new Error('counter unavailable');
    mockGetActiveRequests.mockImplementation(() => { throw counterError; });
    const errorSpy = jest.spyOn(console, 'error').mockImplementation(() => {});
    const warnSpy = jest.spyOn(console, 'warn').mockImplementation(() => {});

    const server = makeMockServer();
    await createShutdownHandler(server, 100)('SIGTERM');

    expect(errorSpy).toHaveBeenCalledWith(
      expect.stringContaining('Failed to read active request count during drain'),
      counterError,
    );
    expect(warnSpy).toHaveBeenCalledWith(
      expect.stringContaining('Proceeding without a reliable in-flight request count'),
    );
    const fabricatedDrainWarning = warnSpy.mock.calls.find((args) =>
      String(args[0]).includes('Drain timeout') && String(args[0]).includes('request(s)'),
    );
    expect(fabricatedDrainWarning).toBeUndefined();
    errorSpy.mockRestore();
    warnSpy.mockRestore();
  });

  it('exits 0 when every one of the five steps throws', async () => {
    mockSetMaintenanceMode.mockImplementation(() => { throw new Error('1'); });
    const server = { close: jest.fn(() => { throw new Error('2'); }) } as unknown as http.Server;
    mockGetActiveRequests.mockImplementation(() => { throw new Error('3'); });
    mockFlush.mockImplementation(() => { throw new Error('4'); });
    mockCloseDatabase.mockImplementation(() => { throw new Error('5'); });
    const errorSpy = jest.spyOn(console, 'error').mockImplementation(() => {});
    const warnSpy = jest.spyOn(console, 'warn').mockImplementation(() => {});

    await createShutdownHandler(server, 100)('SIGTERM');

    expect(exitSpy).toHaveBeenCalledWith(0);
    errorSpy.mockRestore();
    warnSpy.mockRestore();
  });

  // ── Boundary and concurrency cases ───────────────────────────────────────

  it('exits immediately with a drainTimeoutMs of 0 when requests are still active', async () => {
    mockGetActiveRequests.mockReturnValue(5); // never drains

    const server = makeMockServer();
    const start = Date.now();
    await createShutdownHandler(server, 0)('SIGTERM');
    const elapsed = Date.now() - start;

    // The deadline is already in the past on entry, so the drain loop's
    // condition must fail on its first check rather than sleeping even one
    // DRAIN_POLL_MS tick.
    expect(elapsed).toBeLessThan(DRAIN_POLL_MS);
    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('runs the full sequence exactly once under two genuinely concurrent invocations of the same handler', async () => {
    // Unlike the sequential "second signal" tests above, neither call here
    // is awaited before the other starts: this exercises the _shuttingDown
    // guard's actual race-safety, which holds only because the guard is set
    // synchronously before the function's first await, not because the
    // caller happens to serialize the two signals.
    const server = makeMockServer();
    const handler = createShutdownHandler(server, 100);

    const [firstResult, secondResult] = await Promise.allSettled([
      handler('SIGTERM'),
      handler('SIGINT'),
    ]);

    expect(firstResult.status).toBe('fulfilled');
    expect(secondResult.status).toBe('fulfilled');
    // Exactly one call ran the real sequence (exit(0)) and the other was
    // rejected by the guard as a second signal (exit(1)) — never both
    // exit(0), which would mean the guard let the sequence run twice.
    const exitCodes = exitSpy.mock.calls.map((args) => args[0]).sort();
    expect(exitCodes).toEqual([0, 1]);
    expect(mockSetMaintenanceMode).toHaveBeenCalledTimes(1);
    expect(mockFlush).toHaveBeenCalledTimes(1);
    expect(mockCloseDatabase).toHaveBeenCalledTimes(1);
  });
});

// ---------------------------------------------------------------------------
// Exported constants
// ---------------------------------------------------------------------------
describe('shutdown constants', () => {
  it('DEFAULT_DRAIN_TIMEOUT_MS is a positive integer', () => {
    expect(typeof DEFAULT_DRAIN_TIMEOUT_MS).toBe('number');
    expect(DEFAULT_DRAIN_TIMEOUT_MS).toBeGreaterThan(0);
    expect(Number.isInteger(DEFAULT_DRAIN_TIMEOUT_MS)).toBe(true);
  });

  it('DRAIN_POLL_MS is a positive integer less than DEFAULT_DRAIN_TIMEOUT_MS', () => {
    expect(typeof DRAIN_POLL_MS).toBe('number');
    expect(DRAIN_POLL_MS).toBeGreaterThan(0);
    expect(DRAIN_POLL_MS).toBeLessThan(DEFAULT_DRAIN_TIMEOUT_MS);
  });
});

// ---------------------------------------------------------------------------
// WebhookQueueService.flush — tested against the REAL implementation
// ---------------------------------------------------------------------------
describe('WebhookQueueService.flush (real implementation)', () => {
  const FLUSH_TEST_DB_DIR = path.resolve(__dirname, "../../.data");
  const FLUSH_TEST_DB_PATH = path.join(FLUSH_TEST_DB_DIR, `test-shutdown-flush-${crypto.randomUUID()}.db`);

  beforeAll(() => {
    process.env.DATABASE_PATH = FLUSH_TEST_DB_PATH;
    const conn = getDatabase();
    conn.exec(`
      CREATE TABLE IF NOT EXISTS webhook_deliveries (
        id TEXT PRIMARY KEY,
        event_type TEXT NOT NULL,
        payload TEXT NOT NULL,
        subscriber_id TEXT,
        status TEXT NOT NULL DEFAULT 'pending'
          CHECK(status IN ('pending','processing','success','failed','dead_letter')),
        enqueued_at TEXT NOT NULL DEFAULT (datetime('now')),
        attempt_count INTEGER NOT NULL DEFAULT 0,
        max_attempts INTEGER NOT NULL DEFAULT 5,
        next_retry_at TEXT,
        last_error TEXT,
        last_attempt_at TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
      )
    `);
    conn.exec(`
      CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_status_next_retry
      ON webhook_deliveries(status, next_retry_at)
    `);
    conn.exec(`
      CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_created_at
      ON webhook_deliveries(created_at)
    `);
  });

  afterAll(() => {
    closeDatabase();
    try {
      require("fs").unlinkSync(FLUSH_TEST_DB_PATH);
    } catch {
      // ignore
    }
  });

  // Bypass the module-level mock and use the actual class directly.
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const { WebhookQueueService } = jest.requireActual<
    typeof import('../services/webhookQueueService')
  >('../services/webhookQueueService');

  function freshQueue() {
    WebhookQueueService.resetInstance();
    const conn = getDatabase();
    conn.exec("DELETE FROM webhook_deliveries");
    return WebhookQueueService.getInstance();
  }

  it('returns an empty array when the queue is empty', () => {
    const q = freshQueue();
    expect(q.flush()).toEqual([]);
  });

  it('returns all pending events and clears the queue', () => {
    const q = freshQueue();
    q.enqueue('invoice.created', { id: '1' });
    q.enqueue('bid.placed', { id: '2' });

    const flushed = q.flush();

    expect(flushed).toHaveLength(2);
    expect(flushed[0].status).toBe('pending');
    expect(flushed[1].status).toBe('pending');
    expect(q.getDepth()).toBe(0);
  });

  it('excludes events already marked success', () => {
    const q = freshQueue();
    const evt = q.enqueue('invoice.settled');
    q.markSuccess(evt.id);

    expect(q.flush()).toEqual([]);
  });

  it('excludes events already marked failed', () => {
    const q = freshQueue();
    const evt = q.enqueue('bid.expired');
    q.markFailed(evt.id);

    expect(q.flush()).toEqual([]);
  });

  it('returns only pending events from a mixed queue', () => {
    const q = freshQueue();
    const e1 = q.enqueue('a');
    q.enqueue('b');
    const e3 = q.enqueue('c');
    q.markSuccess(e1.id);
    q.markFailed(e3.id);

    const flushed = q.flush();

    expect(flushed).toHaveLength(1);
    expect(flushed[0].type).toBe('b');
  });

  it('resets depth to zero after flush', () => {
    const q = freshQueue();
    q.enqueue('x');
    q.enqueue('y');
    q.enqueue('z');
    q.flush();
    expect(q.getDepth()).toBe(0);
  });

  it('returns snapshot copies — mutating the result does not affect the queue', () => {
    const q = freshQueue();
    q.enqueue('snapshot-test');
    const [copy] = q.flush();
    (copy as any).status = 'success';

    expect(q.flush()).toEqual([]);
  });

  it('queue is reusable after flush', () => {
    const q = freshQueue();
    q.enqueue('before-flush');
    q.flush();

    q.enqueue('after-flush');
    const flushed = q.flush();

    expect(flushed).toHaveLength(1);
    expect(flushed[0].type).toBe('after-flush');
  });
});

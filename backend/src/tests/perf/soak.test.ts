import crypto from 'crypto';
import path from 'path';
import fs from 'fs';
import { SoakHarness } from './harness';
import { webhookQueueService } from '../../services/webhookQueueService';
import { lagMonitor } from '../../services/lagMonitor';
import { webhookDeliveryRepo } from '../../services/webhookDeliveryRepo';
import { closeDatabase } from '../../lib/database';
import { statusService } from '../../services/statusService';

// Allow long runs in CI
const envDuration = process.env.SOAK_DURATION_MS ? parseInt(process.env.SOAK_DURATION_MS, 10) : undefined;
const defaultMs = process.env.CI ? 60 * 60 * 1000 : 60 * 1000;
const DURATION_MS = envDuration ?? defaultMs;

jest.setTimeout(DURATION_MS + 30_000);

const TEST_DB_DIR = path.resolve(__dirname, '../../../.data');
const TEST_DB_PATH = path.join(TEST_DB_DIR, `test-soak-${crypto.randomUUID()}.db`);

beforeAll(() => {
  process.env.DATABASE_PATH = TEST_DB_PATH;
  closeDatabase();
  webhookDeliveryRepo.ensureSchema();
  statusService.setMockCurrentLedger(100000);
  statusService.updateLastIndexedLedger(100000);
});

afterAll(() => {
  closeDatabase();
  try {
    if (fs.existsSync(TEST_DB_PATH)) fs.unlinkSync(TEST_DB_PATH);
    try { fs.unlinkSync(TEST_DB_PATH + '-wal'); } catch {}
    try { fs.unlinkSync(TEST_DB_PATH + '-shm'); } catch {}
  } catch {}
});

test('soak: run indexer + webhook pipeline and assert stability', async () => {
  const harness = new SoakHarness({ produceRatePerSecond: 50, consumeIntervalMs: 500, sampleIntervalMs: 1000 });
  const startMem = process.memoryUsage().rss;
  const start = Date.now();

  harness.start();

  // Run for duration
  await new Promise<void>((resolve) => setTimeout(() => resolve(), DURATION_MS));

  // Stop and give one last sample
  harness.stop();
  await new Promise((r) => setTimeout(r, 250));

  const samples = harness.getSamples();
  expect(samples.length).toBeGreaterThan(0);

  const queueStats = webhookQueueService.getStats();
  const capacity = queueStats.capacity;
  const maxDepth = samples.reduce((m, s) => Math.max(m, s.queueDepth), 0);

  // Queue depth stays bounded (below configured capacity)
  expect(maxDepth).toBeLessThanOrEqual(capacity);

  // RSS growth must be < 50 MB/hour scaled to run duration (with 10MB base buffer for V8 overhead)
  const endMem = process.memoryUsage().rss;
  const elapsedHours = (Date.now() - start) / (1000 * 60 * 60);
  const allowedGrowthBytes = 50 * 1024 * 1024 * elapsedHours;
  const baseBufferBytes = 10 * 1024 * 1024; // 10MB V8 buffer
  const growth = endMem - startMem;
  expect(growth).toBeLessThanOrEqual(Math.ceil(allowedGrowthBytes + baseBufferBytes));

  // Lag returns to zero by end of run
  const finalLagStatus = await lagMonitor.getLagStatus();
  expect(finalLagStatus.lag).toBe(0);

  // Clean up any remaining timers/instances to let Jest exit cleanly
  // Reset singleton webhookQueueService
  try {
    // @ts-ignore
    const { WebhookQueueService } = require('../../src/services/webhookQueueService');
    if (WebhookQueueService && WebhookQueueService.resetInstance) WebhookQueueService.resetInstance();
  } catch {}
});

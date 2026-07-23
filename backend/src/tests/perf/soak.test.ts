import { SoakHarness } from './harness';
import { webhookQueueService } from '../../services/webhookQueueService';
import { lagMonitor } from '../../services/lagMonitor';

// Allow long runs in CI
const envDuration = process.env.SOAK_DURATION_MS ? parseInt(process.env.SOAK_DURATION_MS, 10) : undefined;
const defaultMs = process.env.CI ? 60 * 60 * 1000 : 60 * 1000;
const DURATION_MS = envDuration ?? defaultMs;

jest.setTimeout(DURATION_MS + 30_000);

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

  // RSS growth must be < 50 MB/hour scaled to run duration
  const endMem = process.memoryUsage().rss;
  const elapsedHours = (Date.now() - start) / (1000 * 60 * 60);
  const allowedGrowthBytes = 50 * 1024 * 1024 * Math.max(1e-6, elapsedHours);
  const growth = endMem - startMem;
  expect(growth).toBeLessThanOrEqual(Math.ceil(allowedGrowthBytes));

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

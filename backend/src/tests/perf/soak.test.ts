import { SoakHarness } from "./harness";
import {
  webhookQueueService,
  WebhookQueueService,
} from "../../services/webhookQueueService";
import {
  webhookDeliveryRepo,
  type WebhookDelivery,
} from "../../services/webhookDeliveryRepo";
import { lagMonitor } from "../../services/lagMonitor";
import { statusService } from "../../services/statusService";

// Keep CI runs fast by default while still allowing longer runs via SOAK_DURATION_MS
const envDuration = process.env.SOAK_DURATION_MS
  ? parseInt(process.env.SOAK_DURATION_MS, 10)
  : undefined;
const defaultMs = 1500;
const DURATION_MS =
  envDuration !== undefined && !Number.isNaN(envDuration) && envDuration > 0
    ? envDuration
    : defaultMs;

describe("perf soak", () => {
  const initialLedger = statusService.getLastIndexedLedger();
  const originalRepo = {
    create: webhookDeliveryRepo.create.bind(webhookDeliveryRepo),
    getPending: webhookDeliveryRepo.getPending.bind(webhookDeliveryRepo),
    markSuccess: webhookDeliveryRepo.markSuccess.bind(webhookDeliveryRepo),
    getStats: webhookDeliveryRepo.getStats.bind(webhookDeliveryRepo),
    incrementOverflow:
      webhookDeliveryRepo.incrementOverflow.bind(webhookDeliveryRepo),
    getOverflowCount:
      webhookDeliveryRepo.getOverflowCount.bind(webhookDeliveryRepo),
  };

  beforeEach(() => {
    let seq = 0;
    let overflowCount = 0;
    let successCount = 0;
    const pending = new Map<string, WebhookDelivery>();

    webhookDeliveryRepo.create = ({ eventType, payload, subscriberId }) => {
      const id = `soak-${++seq}`;
      const now = new Date().toISOString();
      const delivery: WebhookDelivery = {
        id,
        eventType,
        payload,
        subscriberId: subscriberId ?? null,
        status: "pending",
        enqueuedAt: now,
        attemptCount: 0,
        maxAttempts: 5,
        nextRetryAt: null,
        lastError: null,
        lastAttemptAt: null,
        createdAt: now,
        updatedAt: now,
      };
      pending.set(id, delivery);
      return delivery;
    };

    webhookDeliveryRepo.getPending = () => Array.from(pending.values());

    webhookDeliveryRepo.markSuccess = (id: string) => {
      const deleted = pending.delete(id);
      if (deleted) successCount += 1;
      return deleted;
    };

    webhookDeliveryRepo.getStats = () => {
      const first = pending.values().next().value as
        | WebhookDelivery
        | undefined;
      return {
        pending: pending.size,
        processing: 0,
        success: successCount,
        failed: 0,
        dead_letter: 0,
        oldestPending: first ? first.enqueuedAt : null,
      };
    };

    webhookDeliveryRepo.incrementOverflow = () => {
      overflowCount += 1;
    };

    webhookDeliveryRepo.getOverflowCount = () => overflowCount;

    statusService.setMockCurrentLedger(initialLedger);
    statusService.updateLastIndexedLedger(initialLedger);
    lagMonitor.reset();
    webhookQueueService.flush();
  });

  afterEach(() => {
    webhookDeliveryRepo.create = originalRepo.create;
    webhookDeliveryRepo.getPending = originalRepo.getPending;
    webhookDeliveryRepo.markSuccess = originalRepo.markSuccess;
    webhookDeliveryRepo.getStats = originalRepo.getStats;
    webhookDeliveryRepo.incrementOverflow = originalRepo.incrementOverflow;
    webhookDeliveryRepo.getOverflowCount = originalRepo.getOverflowCount;

    statusService.setMockCurrentLedger(null);
    statusService.updateLastIndexedLedger(initialLedger);
    lagMonitor.reset();
    WebhookQueueService.resetInstance();
  });

  test(
    "soak: run indexer + webhook pipeline and assert stability",
    async () => {
      const harness = new SoakHarness({
        produceRatePerSecond: 50,
        consumeIntervalMs: 100,
        sampleIntervalMs: 200,
      });
      const startMem = process.memoryUsage().rss;
      const start = Date.now();

      harness.start();

      try {
        await new Promise<void>((resolve) =>
          setTimeout(() => resolve(), DURATION_MS)
        );
      } finally {
        harness.stop();
      }

      // Allow any in-flight async sampler callback to settle
      await new Promise((r) => setTimeout(r, 100));

      const samples = harness.getSamples();
      expect(samples.length).toBeGreaterThan(0);

      const queueStats = webhookQueueService.getStats();
      const capacity = queueStats.capacity;
      const maxDepth = samples.reduce((m, s) => Math.max(m, s.queueDepth), 0);

      // Queue depth stays bounded (below configured capacity)
      expect(maxDepth).toBeLessThanOrEqual(capacity);

      // RSS growth must stay bounded (< 50 MB floor, or < 50 MB/hour for long runs)
      const endMem = process.memoryUsage().rss;
      const elapsedHours = (Date.now() - start) / (1000 * 60 * 60);
      const allowedGrowthBytes = 50 * 1024 * 1024 * Math.max(1, elapsedHours);
      const growth = endMem - startMem;
      expect(growth).toBeLessThanOrEqual(Math.ceil(allowedGrowthBytes));

      // Lag returns to zero by end of run
      const finalLagStatus = await lagMonitor.getLagStatus();
      expect(finalLagStatus.lag).toBe(0);
    },
    DURATION_MS + 30_000
  );
});

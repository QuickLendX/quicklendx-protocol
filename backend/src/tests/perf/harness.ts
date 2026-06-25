import { webhookQueueService } from '../../services/webhookQueueService';
import { lagMonitor } from '../../services/lagMonitor';

export interface Sample {
  t: string;
  rss: number;
  heapUsed: number;
  heapTotal: number;
  queueDepth: number;
  lag: number;
}

export class SoakHarness {
  private samples: Sample[] = [];
  private producer?: NodeJS.Timeout;
  private consumer?: NodeJS.Timeout;
  private sampler?: NodeJS.Timeout;

  constructor(private opts: { produceRatePerSecond?: number; consumeIntervalMs?: number; sampleIntervalMs?: number } = {}) {}

  start(): void {
    const produceRate = this.opts.produceRatePerSecond ?? 50; // events/sec
    const produceInterval = Math.max(1, Math.floor(1000 / produceRate));
    const consumeInterval = this.opts.consumeIntervalMs ?? 500;
    const sampleInterval = this.opts.sampleIntervalMs ?? 1000;

    this.producer = setInterval(() => {
      try {
        webhookQueueService.enqueue('soak:event', { ts: new Date().toISOString() });
      } catch {
        // capacity exceeded — countable condition; don't throw in harness
      }
    }, produceInterval);

    this.consumer = setInterval(() => {
      try {
        // Drain a handful (simulate slow consumer)
        const pending = webhookQueueService.flush();
        // mark them processed (already removed by flush). In real pipeline we'd mark success.
        // noop
      } catch {
        // ignore errors
      }
    }, consumeInterval);

    this.sampler = setInterval(async () => {
      try {
        const stats = webhookQueueService.getStats();
        const lagStatus = await lagMonitor.getLagStatus();
        const mem = process.memoryUsage();
        this.samples.push({
          t: new Date().toISOString(),
          rss: mem.rss,
          heapUsed: mem.heapUsed,
          heapTotal: mem.heapTotal,
          queueDepth: stats.size,
          lag: lagStatus.lag,
        });
      } catch {
        // ignore sampling errors
      }
    }, sampleInterval);
  }

  stop(): void {
    if (this.producer) clearInterval(this.producer);
    if (this.consumer) clearInterval(this.consumer);
    if (this.sampler) clearInterval(this.sampler);
  }

  getSamples(): Sample[] {
    return this.samples.slice();
  }
}

export function formatBytes(n: number): string {
  return `${Math.round(n / 1024)} KB`;
}
/**
 * Lightweight latency measurement harness.
 * Runs N requests against a supertest agent and returns p50/p95/p99.
 */
import request from "supertest";
import { Express } from "express";

export interface LatencyStats {
  p50: number;
  p95: number;
  p99: number;
  min: number;
  max: number;
  samples: number;
}

export async function measure(
  app: Express,
  path: string,
  iterations = 200
): Promise<LatencyStats> {
  const agent = request(app);
  const latencies: number[] = [];

  for (let i = 0; i < iterations; i++) {
    const start = process.hrtime.bigint();
    await agent.get(path);
    const elapsed = Number(process.hrtime.bigint() - start) / 1e6; // ms
    latencies.push(elapsed);
  }

  latencies.sort((a, b) => a - b);

  const pct = (p: number) =>
    latencies[Math.ceil((p / 100) * latencies.length) - 1];

  return {
    p50: pct(50),
    p95: pct(95),
    p99: pct(99),
    min: latencies[0],
    max: latencies[latencies.length - 1],
    samples: latencies.length,
  };
}

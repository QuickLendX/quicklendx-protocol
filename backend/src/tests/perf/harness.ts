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

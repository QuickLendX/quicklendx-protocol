import { describe, it, expect, beforeEach } from "@jest/globals";
import {
  RetentionStore,
  RetentionConfig,
  DEFAULT_CONFIG,
  cleanRawEvents,
  cleanSnapshots,
  cleanWebhookLogs,
  cleanOperationalLogs,
  runAllCleanupJobs,
  RawEvent,
  Snapshot,
  WebhookLog,
  OperationalLog,
} from "../src/services/retention";

// ── Helpers ───────────────────────────────────────────────────────────────────

const DAY_MS = 24 * 60 * 60 * 1000;
const NOW = 1_000_000_000_000; // fixed reference timestamp

/** Config with short TTLs for deterministic tests. */
const cfg = (ttlMs: number, reconciliationWindowMs = 0, batchSize = 1000): RetentionConfig => ({
  ttlMs,
  reconciliationWindowMs,
  batchSize,
});

const rawEvent = (id: string, ageMs: number, complianceHold = false): RawEvent => ({
  id,
  ledger: 1,
  type: "invoice_created",
  payload: {},
  createdAt: NOW - ageMs,
  complianceHold,
});

const snapshot = (id: string, ageMs: number): Snapshot => ({
  id,
  table: "invoices",
  createdAt: NOW - ageMs,
});

const webhookLog = (id: string, ageMs: number): WebhookLog => ({
  id,
  endpoint: "https://example.com/hook",
  statusCode: 200,
  createdAt: NOW - ageMs,
});

const opLog = (id: string, ageMs: number): OperationalLog => ({
  id,
  level: "info",
  message: "test",
  createdAt: NOW - ageMs,
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("retention service", () => {
  let store: RetentionStore;

  beforeEach(() => {
    store = new RetentionStore();
  });

  // ── DEFAULT_CONFIG ─────────────────────────────────────────────────────────

  describe("DEFAULT_CONFIG", () => {
    it("raw_events TTL is 90 days", () => {
      expect(DEFAULT_CONFIG.raw_events.ttlMs).toBe(90 * DAY_MS);
    });
    it("snapshots TTL is 30 days", () => {
      expect(DEFAULT_CONFIG.snapshots.ttlMs).toBe(30 * DAY_MS);
    });
    it("webhook_logs TTL is 14 days", () => {
      expect(DEFAULT_CONFIG.webhook_logs.ttlMs).toBe(14 * DAY_MS);
    });
    it("operational_logs TTL is 7 days", () => {
      expect(DEFAULT_CONFIG.operational_logs.ttlMs).toBe(7 * DAY_MS);
    });
    it("all categories have a positive reconciliationWindowMs", () => {
      for (const c of Object.values(DEFAULT_CONFIG)) {
        expect(c.reconciliationWindowMs).toBeGreaterThan(0);
      }
    });
    it("all categories have a positive batchSize", () => {
      for (const c of Object.values(DEFAULT_CONFIG)) {
        expect(c.batchSize).toBeGreaterThan(0);
      }
    });
  });

  // ── cleanRawEvents ─────────────────────────────────────────────────────────

  describe("cleanRawEvents", () => {
    it("deletes events older than TTL", () => {
      store.rawEvents = [rawEvent("old", 100 * DAY_MS)];
      const result = cleanRawEvents(store, cfg(90 * DAY_MS), { now: NOW });
      expect(result.deleted).toBe(1);
      expect(store.rawEvents).toHaveLength(0);
    });

    it("keeps events younger than TTL", () => {
      store.rawEvents = [rawEvent("new", 10 * DAY_MS)];
      const result = cleanRawEvents(store, cfg(90 * DAY_MS), { now: NOW });
      expect(result.deleted).toBe(0);
      expect(store.rawEvents).toHaveLength(1);
    });

    it("never deletes events with complianceHold=true", () => {
      store.rawEvents = [rawEvent("held", 200 * DAY_MS, true)];
      const result = cleanRawEvents(store, cfg(90 * DAY_MS), { now: NOW });
      expect(result.deleted).toBe(0);
      expect(result.skippedHold).toBe(1);
      expect(store.rawEvents).toHaveLength(1);
    });

    it("deletes non-held events but preserves held ones in the same batch", () => {
      store.rawEvents = [
        rawEvent("held", 200 * DAY_MS, true),
        rawEvent("old", 100 * DAY_MS, false),
      ];
      const result = cleanRawEvents(store, cfg(90 * DAY_MS), { now: NOW });
      expect(result.deleted).toBe(1);
      expect(result.skippedHold).toBe(1);
      expect(store.rawEvents.map((e) => e.id)).toEqual(["held"]);
    });

    it("keeps events within the reconciliation window even if older than TTL", () => {
      const window = 2 * DAY_MS;
      // age = 1 day < window = 2 days → must be kept
      store.rawEvents = [rawEvent("recent", 1 * DAY_MS)];
      const result = cleanRawEvents(store, cfg(0, window), { now: NOW });
      expect(result.deleted).toBe(0);
      expect(result.skippedWindow).toBe(1);
    });

    it("respects batchSize limit", () => {
      store.rawEvents = Array.from({ length: 10 }, (_, i) =>
        rawEvent(`e${i}`, 100 * DAY_MS)
      );
      const result = cleanRawEvents(store, cfg(90 * DAY_MS, 0, 3), { now: NOW });
      expect(result.deleted).toBe(3);
      expect(store.rawEvents).toHaveLength(7);
    });

    it("dry-run returns correct counts without mutating store", () => {
      store.rawEvents = [rawEvent("old", 100 * DAY_MS)];
      const result = cleanRawEvents(store, cfg(90 * DAY_MS), { now: NOW, dryRun: true });
      expect(result.dryRun).toBe(true);
      expect(result.deleted).toBe(1);
      expect(store.rawEvents).toHaveLength(1); // unchanged
    });

    it("dry-run includes deletedIds", () => {
      store.rawEvents = [rawEvent("old1", 100 * DAY_MS), rawEvent("old2", 100 * DAY_MS)];
      const result = cleanRawEvents(store, cfg(90 * DAY_MS), { now: NOW, dryRun: true });
      expect(result.deletedIds).toEqual(["old1", "old2"]);
    });

    it("returns category=raw_events", () => {
      const result = cleanRawEvents(store, cfg(90 * DAY_MS), { now: NOW });
      expect(result.category).toBe("raw_events");
    });

    it("empty store returns zero counts", () => {
      const result = cleanRawEvents(store, cfg(90 * DAY_MS), { now: NOW });
      expect(result.deleted).toBe(0);
      expect(result.skippedHold).toBe(0);
      expect(result.skippedWindow).toBe(0);
    });

    it("record exactly at TTL boundary is eligible", () => {
      store.rawEvents = [rawEvent("boundary", 90 * DAY_MS)];
      const result = cleanRawEvents(store, cfg(90 * DAY_MS), { now: NOW });
      expect(result.deleted).toBe(1);
    });

    it("record one ms before TTL is not eligible", () => {
      store.rawEvents = [rawEvent("almost", 90 * DAY_MS - 1)];
      const result = cleanRawEvents(store, cfg(90 * DAY_MS), { now: NOW });
      expect(result.deleted).toBe(0);
    });
  });

  // ── cleanSnapshots ─────────────────────────────────────────────────────────

  describe("cleanSnapshots", () => {
    it("deletes snapshots older than TTL", () => {
      store.snapshots = [snapshot("s1", 31 * DAY_MS)];
      const result = cleanSnapshots(store, cfg(30 * DAY_MS), { now: NOW });
      expect(result.deleted).toBe(1);
      expect(store.snapshots).toHaveLength(0);
    });

    it("keeps snapshots younger than TTL", () => {
      store.snapshots = [snapshot("s1", 5 * DAY_MS)];
      const result = cleanSnapshots(store, cfg(30 * DAY_MS), { now: NOW });
      expect(result.deleted).toBe(0);
      expect(store.snapshots).toHaveLength(1);
    });

    it("respects reconciliation window", () => {
      const window = 1 * DAY_MS;
      store.snapshots = [snapshot("s1", 30 * 60 * 1000)]; // 30 min old < 1 day window
      const result = cleanSnapshots(store, cfg(0, window), { now: NOW });
      expect(result.deleted).toBe(0);
      expect(result.skippedWindow).toBe(1);
    });

    it("respects batchSize", () => {
      store.snapshots = Array.from({ length: 5 }, (_, i) => snapshot(`s${i}`, 31 * DAY_MS));
      const result = cleanSnapshots(store, cfg(30 * DAY_MS, 0, 2), { now: NOW });
      expect(result.deleted).toBe(2);
      expect(store.snapshots).toHaveLength(3);
    });

    it("dry-run does not mutate store", () => {
      store.snapshots = [snapshot("s1", 31 * DAY_MS)];
      cleanSnapshots(store, cfg(30 * DAY_MS), { now: NOW, dryRun: true });
      expect(store.snapshots).toHaveLength(1);
    });

    it("returns category=snapshots", () => {
      const result = cleanSnapshots(store, cfg(30 * DAY_MS), { now: NOW });
      expect(result.category).toBe("snapshots");
    });

    it("skippedHold is always 0 (snapshots have no compliance hold)", () => {
      store.snapshots = [snapshot("s1", 31 * DAY_MS)];
      const result = cleanSnapshots(store, cfg(30 * DAY_MS), { now: NOW });
      expect(result.skippedHold).toBe(0);
    });
  });

  // ── cleanWebhookLogs ───────────────────────────────────────────────────────

  describe("cleanWebhookLogs", () => {
    it("deletes webhook logs older than TTL", () => {
      store.webhookLogs = [webhookLog("w1", 15 * DAY_MS)];
      const result = cleanWebhookLogs(store, cfg(14 * DAY_MS), { now: NOW });
      expect(result.deleted).toBe(1);
      expect(store.webhookLogs).toHaveLength(0);
    });

    it("keeps webhook logs younger than TTL", () => {
      store.webhookLogs = [webhookLog("w1", 3 * DAY_MS)];
      const result = cleanWebhookLogs(store, cfg(14 * DAY_MS), { now: NOW });
      expect(result.deleted).toBe(0);
    });

    it("respects reconciliation window", () => {
      const window = 30 * 60 * 1000; // 30 min
      store.webhookLogs = [webhookLog("w1", 10 * 60 * 1000)]; // 10 min old
      const result = cleanWebhookLogs(store, cfg(0, window), { now: NOW });
      expect(result.deleted).toBe(0);
      expect(result.skippedWindow).toBe(1);
    });

    it("respects batchSize", () => {
      store.webhookLogs = Array.from({ length: 6 }, (_, i) => webhookLog(`w${i}`, 15 * DAY_MS));
      const result = cleanWebhookLogs(store, cfg(14 * DAY_MS, 0, 4), { now: NOW });
      expect(result.deleted).toBe(4);
      expect(store.webhookLogs).toHaveLength(2);
    });

    it("dry-run does not mutate store", () => {
      store.webhookLogs = [webhookLog("w1", 15 * DAY_MS)];
      cleanWebhookLogs(store, cfg(14 * DAY_MS), { now: NOW, dryRun: true });
      expect(store.webhookLogs).toHaveLength(1);
    });

    it("returns category=webhook_logs", () => {
      const result = cleanWebhookLogs(store, cfg(14 * DAY_MS), { now: NOW });
      expect(result.category).toBe("webhook_logs");
    });
  });

  // ── cleanOperationalLogs ───────────────────────────────────────────────────

  describe("cleanOperationalLogs", () => {
    it("deletes operational logs older than TTL", () => {
      store.operationalLogs = [opLog("o1", 8 * DAY_MS)];
      const result = cleanOperationalLogs(store, cfg(7 * DAY_MS), { now: NOW });
      expect(result.deleted).toBe(1);
      expect(store.operationalLogs).toHaveLength(0);
    });

    it("keeps operational logs younger than TTL", () => {
      store.operationalLogs = [opLog("o1", 2 * DAY_MS)];
      const result = cleanOperationalLogs(store, cfg(7 * DAY_MS), { now: NOW });
      expect(result.deleted).toBe(0);
    });

    it("respects reconciliation window", () => {
      const window = 15 * 60 * 1000; // 15 min
      store.operationalLogs = [opLog("o1", 5 * 60 * 1000)]; // 5 min old
      const result = cleanOperationalLogs(store, cfg(0, window), { now: NOW });
      expect(result.deleted).toBe(0);
      expect(result.skippedWindow).toBe(1);
    });

    it("respects batchSize", () => {
      store.operationalLogs = Array.from({ length: 10 }, (_, i) => opLog(`o${i}`, 8 * DAY_MS));
      const result = cleanOperationalLogs(store, cfg(7 * DAY_MS, 0, 5), { now: NOW });
      expect(result.deleted).toBe(5);
      expect(store.operationalLogs).toHaveLength(5);
    });

    it("dry-run does not mutate store", () => {
      store.operationalLogs = [opLog("o1", 8 * DAY_MS)];
      cleanOperationalLogs(store, cfg(7 * DAY_MS), { now: NOW, dryRun: true });
      expect(store.operationalLogs).toHaveLength(1);
    });

    it("returns category=operational_logs", () => {
      const result = cleanOperationalLogs(store, cfg(7 * DAY_MS), { now: NOW });
      expect(result.category).toBe("operational_logs");
    });
  });

  // ── runAllCleanupJobs ──────────────────────────────────────────────────────

  describe("runAllCleanupJobs", () => {
    it("returns results for all four categories", () => {
      const results = runAllCleanupJobs(store, {}, { now: NOW });
      expect(results).toHaveLength(4);
      const categories = results.map((r) => r.category);
      expect(categories).toContain("raw_events");
      expect(categories).toContain("snapshots");
      expect(categories).toContain("webhook_logs");
      expect(categories).toContain("operational_logs");
    });

    it("cleans all categories in one call", () => {
      store.rawEvents = [rawEvent("e1", 100 * DAY_MS)];
      store.snapshots = [snapshot("s1", 31 * DAY_MS)];
      store.webhookLogs = [webhookLog("w1", 15 * DAY_MS)];
      store.operationalLogs = [opLog("o1", 8 * DAY_MS)];

      runAllCleanupJobs(
        store,
        {
          raw_events: cfg(90 * DAY_MS),
          snapshots: cfg(30 * DAY_MS),
          webhook_logs: cfg(14 * DAY_MS),
          operational_logs: cfg(7 * DAY_MS),
        },
        { now: NOW }
      );

      expect(store.rawEvents).toHaveLength(0);
      expect(store.snapshots).toHaveLength(0);
      expect(store.webhookLogs).toHaveLength(0);
      expect(store.operationalLogs).toHaveLength(0);
    });

    it("dry-run across all categories mutates nothing", () => {
      store.rawEvents = [rawEvent("e1", 100 * DAY_MS)];
      store.snapshots = [snapshot("s1", 31 * DAY_MS)];
      store.webhookLogs = [webhookLog("w1", 15 * DAY_MS)];
      store.operationalLogs = [opLog("o1", 8 * DAY_MS)];

      runAllCleanupJobs(
        store,
        {
          raw_events: cfg(90 * DAY_MS),
          snapshots: cfg(30 * DAY_MS),
          webhook_logs: cfg(14 * DAY_MS),
          operational_logs: cfg(7 * DAY_MS),
        },
        { now: NOW, dryRun: true }
      );

      expect(store.rawEvents).toHaveLength(1);
      expect(store.snapshots).toHaveLength(1);
      expect(store.webhookLogs).toHaveLength(1);
      expect(store.operationalLogs).toHaveLength(1);
    });

    it("uses DEFAULT_CONFIG when no config override provided", () => {
      // With default TTLs (7–90 days) and NOW as reference, records aged 0 ms
      // should never be deleted.
      store.rawEvents = [rawEvent("e1", 0)];
      const results = runAllCleanupJobs(store, {}, { now: NOW });
      const rawResult = results.find((r) => r.category === "raw_events")!;
      expect(rawResult.deleted).toBe(0);
    });
  });

  // ── Safety: does not delete required records ───────────────────────────────

  describe("safety — does not delete required records", () => {
    it("compliance-held raw events survive any TTL", () => {
      store.rawEvents = [rawEvent("held", 365 * DAY_MS, true)];
      cleanRawEvents(store, cfg(1), { now: NOW }); // TTL = 1 ms
      expect(store.rawEvents).toHaveLength(1);
    });

    it("records inside reconciliation window survive even with TTL=0", () => {
      const window = 1 * DAY_MS;
      store.rawEvents = [rawEvent("recent", 1 * 60 * 1000)]; // 1 min old
      cleanRawEvents(store, cfg(0, window), { now: NOW });
      expect(store.rawEvents).toHaveLength(1);
    });

    it("mix of held, windowed, and eligible — only eligible are deleted", () => {
      store.rawEvents = [
        rawEvent("held", 200 * DAY_MS, true),   // compliance hold
        rawEvent("recent", 1 * 60 * 1000),       // inside window
        rawEvent("old", 100 * DAY_MS),            // eligible
      ];
      const result = cleanRawEvents(
        store,
        cfg(90 * DAY_MS, 1 * DAY_MS),
        { now: NOW }
      );
      expect(result.deleted).toBe(1);
      expect(result.skippedHold).toBe(1);
      expect(result.skippedWindow).toBe(1);
      expect(store.rawEvents.map((e) => e.id).sort()).toEqual(["held", "recent"]);
    });

    it("batchSize cap leaves remaining records intact for next run", () => {
      store.rawEvents = Array.from({ length: 5 }, (_, i) =>
        rawEvent(`e${i}`, 100 * DAY_MS)
      );
      cleanRawEvents(store, cfg(90 * DAY_MS, 0, 2), { now: NOW });
      // 2 deleted, 3 remain — all 3 are still eligible for the next run
      expect(store.rawEvents).toHaveLength(3);
      const result2 = cleanRawEvents(store, cfg(90 * DAY_MS, 0, 2), { now: NOW });
      expect(result2.deleted).toBe(2);
      expect(store.rawEvents).toHaveLength(1);
    });

    it("deletedIds in result exactly matches what was removed from store", () => {
      store.rawEvents = [
        rawEvent("keep", 10 * DAY_MS),
        rawEvent("del1", 100 * DAY_MS),
        rawEvent("del2", 100 * DAY_MS),
      ];
      const result = cleanRawEvents(store, cfg(90 * DAY_MS), { now: NOW });
      expect(result.deletedIds.sort()).toEqual(["del1", "del2"]);
      expect(store.rawEvents.map((e) => e.id)).toEqual(["keep"]);
    });

    it("snapshot reconciliation window protects records needed for backfill", () => {
      const window = 1 * 60 * 60 * 1000; // 1 h
      store.snapshots = [snapshot("backfill", 30 * 60 * 1000)]; // 30 min old
      cleanSnapshots(store, cfg(0, window), { now: NOW });
      expect(store.snapshots).toHaveLength(1);
    });

    it("webhook log reconciliation window protects in-flight retries", () => {
      const window = 30 * 60 * 1000; // 30 min
      store.webhookLogs = [webhookLog("retry", 5 * 60 * 1000)]; // 5 min old
      cleanWebhookLogs(store, cfg(0, window), { now: NOW });
      expect(store.webhookLogs).toHaveLength(1);
    });

    it("operational log reconciliation window protects recent debug logs", () => {
      const window = 15 * 60 * 1000; // 15 min
      store.operationalLogs = [opLog("debug", 2 * 60 * 1000)]; // 2 min old
      cleanOperationalLogs(store, cfg(0, window), { now: NOW });
      expect(store.operationalLogs).toHaveLength(1);
    });
  });

  // ── CleanupResult shape ────────────────────────────────────────────────────

  describe("CleanupResult shape", () => {
    it("all fields are present on a result", () => {
      const result = cleanRawEvents(store, cfg(90 * DAY_MS), { now: NOW });
      expect(result).toHaveProperty("category");
      expect(result).toHaveProperty("deleted");
      expect(result).toHaveProperty("skippedHold");
      expect(result).toHaveProperty("skippedWindow");
      expect(result).toHaveProperty("dryRun");
      expect(result).toHaveProperty("deletedIds");
    });

    it("dryRun defaults to false", () => {
      const result = cleanRawEvents(store, cfg(90 * DAY_MS), { now: NOW });
      expect(result.dryRun).toBe(false);
    });

    it("deletedIds is an array", () => {
      const result = cleanRawEvents(store, cfg(90 * DAY_MS), { now: NOW });
      expect(Array.isArray(result.deletedIds)).toBe(true);
    });
  });
});

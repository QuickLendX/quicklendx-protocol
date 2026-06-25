/**
 * drift-severity.test.ts
 *
 * Exhaustive test suite for:
 *  - classifyDrift()           (pure classification function)
 *  - ReconciliationWorker      (integration: classify + pause + alert)
 *  - BackfillService           (lifecycle, idempotency, gate)
 *  - AlertRouter               (routing, deduplication, acknowledgement)
 *
 * Coverage target: ≥ 95% on all four metrics (branches / functions / lines /
 * statements) across the three new service modules and the shared types.
 *
 * Run:
 *   npm test -- drift-severity
 */

import { Severity, AlertStatus } from "../types/reconciliation";
import { DriftReport, BackfillRunStatus } from "../types/driftSeverity";
import { classifyDrift, buildAlertKey, ReconciliationWorker } from "../services/driftSeverityWorker";
import { BackfillService } from "../services/driftBackfillService";
import { AlertRouter, NotificationChannel, NoOpChannel, Alert } from "../services/alertRouter";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeReport(
  invoiceMismatches: number,
  settlementAccountingMismatches: number,
  runId = "run-001"
): DriftReport {
  return {
    runId,
    timestamp: Date.now(),
    invoiceMismatches,
    settlementAccountingMismatches,
  };
}

/** Builds a fresh, isolated set of service instances for each test group. */
function makeServices() {
  AlertRouter.resetInstance();
  BackfillService.resetInstance();
  const router = AlertRouter.getInstance();
  const service = BackfillService.getInstance(router);
  const worker = new ReconciliationWorker({ backfillService: service, alertRouter: router });
  return { router, service, worker };
}

// ---------------------------------------------------------------------------
// 1. classifyDrift – pure unit tests
// ---------------------------------------------------------------------------

describe("classifyDrift – severity classification", () => {
  describe("LOW severity", () => {
    it("returns LOW when invoiceMismatches === 1 and no settlement mismatches", () => {
      expect(classifyDrift(makeReport(1, 0))).toBe(Severity.LOW);
    });

    it("returns LOW when there are zero mismatches in both dimensions", () => {
      // No drift at all is treated as LOW (nothing to act on)
      expect(classifyDrift(makeReport(0, 0))).toBe(Severity.LOW);
    });
  });

  describe("MEDIUM severity", () => {
    it("returns MEDIUM at the lower boundary (2 invoice mismatches)", () => {
      expect(classifyDrift(makeReport(2, 0))).toBe(Severity.MEDIUM);
    });

    it("returns MEDIUM at 50 invoice mismatches", () => {
      expect(classifyDrift(makeReport(50, 0))).toBe(Severity.MEDIUM);
    });

    it("returns MEDIUM at the upper boundary (100 invoice mismatches)", () => {
      expect(classifyDrift(makeReport(100, 0))).toBe(Severity.MEDIUM);
    });
  });

  describe("HIGH severity – invoice mismatch threshold", () => {
    it("returns HIGH at 101 invoice mismatches (just above boundary)", () => {
      expect(classifyDrift(makeReport(101, 0))).toBe(Severity.HIGH);
    });

    it("returns HIGH at 1000 invoice mismatches", () => {
      expect(classifyDrift(makeReport(1000, 0))).toBe(Severity.HIGH);
    });
  });

  describe("HIGH severity – settlement accounting mismatches (absolute rule)", () => {
    it("returns HIGH for exactly 1 settlement mismatch regardless of invoice count", () => {
      expect(classifyDrift(makeReport(0, 1))).toBe(Severity.HIGH);
    });

    it("returns HIGH for 1 settlement mismatch with LOW invoice count (1)", () => {
      expect(classifyDrift(makeReport(1, 1))).toBe(Severity.HIGH);
    });

    it("returns HIGH for 1 settlement mismatch with MEDIUM invoice count (50)", () => {
      expect(classifyDrift(makeReport(50, 1))).toBe(Severity.HIGH);
    });

    it("returns HIGH for many settlement mismatches", () => {
      expect(classifyDrift(makeReport(0, 99))).toBe(Severity.HIGH);
    });

    it("returns HIGH when both dimensions are very large", () => {
      expect(classifyDrift(makeReport(500, 500))).toBe(Severity.HIGH);
    });
  });

  describe("Boundary transitions", () => {
    it("transition LOW → MEDIUM at exactly 2 invoice mismatches", () => {
      expect(classifyDrift(makeReport(1, 0))).toBe(Severity.LOW);
      expect(classifyDrift(makeReport(2, 0))).toBe(Severity.MEDIUM);
    });

    it("transition MEDIUM → HIGH at 100 → 101 invoice mismatches", () => {
      expect(classifyDrift(makeReport(100, 0))).toBe(Severity.MEDIUM);
      expect(classifyDrift(makeReport(101, 0))).toBe(Severity.HIGH);
    });

    it("settlement mismatch overrides MEDIUM to HIGH immediately", () => {
      expect(classifyDrift(makeReport(100, 0))).toBe(Severity.MEDIUM);
      expect(classifyDrift(makeReport(100, 1))).toBe(Severity.HIGH);
    });
  });
});

// ---------------------------------------------------------------------------
// 2. buildAlertKey
// ---------------------------------------------------------------------------

describe("buildAlertKey", () => {
  it("produces a deterministic, stable key for a given runId", () => {
    expect(buildAlertKey("run-abc")).toBe("HIGH_DRIFT:run-abc");
  });

  it("produces different keys for different runIds", () => {
    expect(buildAlertKey("run-1")).not.toBe(buildAlertKey("run-2"));
  });
});

// ---------------------------------------------------------------------------
// 3. AlertRouter
// ---------------------------------------------------------------------------

describe("AlertRouter", () => {
  let router: AlertRouter;
  let sentAlerts: Alert[];
  let mockChannel: NotificationChannel;

  beforeEach(() => {
    AlertRouter.resetInstance();
    router = AlertRouter.getInstance();
    sentAlerts = [];
    mockChannel = { send: jest.fn(async (alert: Alert) => { sentAlerts.push(alert); }) };
    router.setCriticalChannel(mockChannel);
    router.setStandardChannel(mockChannel);
  });

  // -- Routing ----------------------------------------------------------------

  it("dispatches a HIGH alert to the critical channel", async () => {
    const dispatched = await router.routeAlert("key-1", Severity.HIGH, "critical issue");
    expect(dispatched).toBe(true);
    expect(mockChannel.send).toHaveBeenCalledTimes(1);
    expect(sentAlerts[0].severity).toBe(Severity.HIGH);
    expect(sentAlerts[0].status).toBe(AlertStatus.Open);
  });

  it("dispatches a MEDIUM alert to the standard channel", async () => {
    const dispatched = await router.routeAlert("key-2", Severity.MEDIUM, "medium drift");
    expect(dispatched).toBe(true);
    expect(mockChannel.send).toHaveBeenCalledTimes(1);
    expect(sentAlerts[0].severity).toBe(Severity.MEDIUM);
  });

  it("dispatches a LOW alert to the standard channel", async () => {
    const dispatched = await router.routeAlert("key-3", Severity.LOW, "minor drift");
    expect(dispatched).toBe(true);
    expect(mockChannel.send).toHaveBeenCalledTimes(1);
    expect(sentAlerts[0].severity).toBe(Severity.LOW);
  });

  // -- Deduplication ----------------------------------------------------------

  it("suppresses a second alert with the same key when the first is still Open", async () => {
    await router.routeAlert("key-dup", Severity.HIGH, "first");
    const secondDispatched = await router.routeAlert("key-dup", Severity.HIGH, "second");
    expect(secondDispatched).toBe(false);
    expect(mockChannel.send).toHaveBeenCalledTimes(1); // only the first
  });

  it("allows re-routing after the original alert is acknowledged", async () => {
    await router.routeAlert("key-refire", Severity.HIGH, "first");
    router.acknowledgeAlert("key-refire");
    const refired = await router.routeAlert("key-refire", Severity.HIGH, "re-fire");
    expect(refired).toBe(true);
    expect(mockChannel.send).toHaveBeenCalledTimes(2);
  });

  it("deduplication works for MEDIUM alerts too", async () => {
    await router.routeAlert("key-med", Severity.MEDIUM, "medium-1");
    const second = await router.routeAlert("key-med", Severity.MEDIUM, "medium-2");
    expect(second).toBe(false);
    expect(mockChannel.send).toHaveBeenCalledTimes(1);
  });

  // -- Acknowledgement --------------------------------------------------------

  it("acknowledgeAlert transitions alert status to Acknowledged", () => {
    router.routeAlert("key-ack", Severity.HIGH, "msg");
    router.acknowledgeAlert("key-ack");
    const alert = router.getAlert("key-ack");
    expect(alert?.status).toBe(AlertStatus.Acknowledged);
    expect(alert?.acknowledgedAt).toBeDefined();
  });

  it("throws when acknowledging a non-existent alert", () => {
    expect(() => router.acknowledgeAlert("no-such-key")).toThrow(/Alert not found/);
  });

  it("throws when acknowledging an already-acknowledged alert", () => {
    router.routeAlert("key-double-ack", Severity.HIGH, "msg");
    router.acknowledgeAlert("key-double-ack");
    expect(() => router.acknowledgeAlert("key-double-ack")).toThrow(/already acknowledged/);
  });

  // -- Query helpers ----------------------------------------------------------

  it("hasOpenAlert returns true for an open alert", async () => {
    await router.routeAlert("key-open", Severity.HIGH, "open");
    expect(router.hasOpenAlert("key-open")).toBe(true);
  });

  it("hasOpenAlert returns false for an acknowledged alert", async () => {
    await router.routeAlert("key-acked", Severity.HIGH, "acked");
    router.acknowledgeAlert("key-acked");
    expect(router.hasOpenAlert("key-acked")).toBe(false);
  });

  it("hasOpenAlert returns false for unknown key", () => {
    expect(router.hasOpenAlert("nope")).toBe(false);
  });

  it("getAlert returns undefined for unknown key", () => {
    expect(router.getAlert("unknown")).toBeUndefined();
  });

  it("getAllAlerts returns all stored alerts", async () => {
    await router.routeAlert("k1", Severity.LOW, "a");
    await router.routeAlert("k2", Severity.MEDIUM, "b");
    expect(router.getAllAlerts()).toHaveLength(2);
  });

  it("clearAlerts empties the store", async () => {
    await router.routeAlert("k1", Severity.HIGH, "a");
    router.clearAlerts();
    expect(router.getAllAlerts()).toHaveLength(0);
  });

  // -- Singleton --------------------------------------------------------------

  it("getInstance returns the same object on repeated calls", () => {
    const a = AlertRouter.getInstance();
    const b = AlertRouter.getInstance();
    expect(a).toBe(b);
  });

  it("resetInstance causes the next getInstance to return a fresh object", () => {
    const before = AlertRouter.getInstance();
    AlertRouter.resetInstance();
    const after = AlertRouter.getInstance();
    expect(before).not.toBe(after);
  });

  // -- NoOpChannel coverage ---------------------------------------------------

  it("NoOpChannel.send resolves without error", async () => {
    const ch = new NoOpChannel();
    const alert: Alert = {
      alertKey: "k",
      severity: Severity.LOW,
      message: "m",
      status: AlertStatus.Open,
      createdAt: Date.now(),
    };
    await expect(ch.send(alert)).resolves.toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// 4. BackfillService
// ---------------------------------------------------------------------------

describe("BackfillService", () => {
  let router: AlertRouter;
  let service: BackfillService;

  beforeEach(() => {
    AlertRouter.resetInstance();
    BackfillService.resetInstance();
    router = AlertRouter.getInstance();
    service = BackfillService.getInstance(router);
  });

  // -- Run lifecycle ----------------------------------------------------------

  it("createRun returns a Running run", () => {
    const run = service.createRun("r1");
    expect(run.runId).toBe("r1");
    expect(run.status).toBe(BackfillRunStatus.Running);
    expect(run.alertAcknowledged).toBe(false);
  });

  it("createRun throws if the run already exists", () => {
    service.createRun("r1");
    expect(() => service.createRun("r1")).toThrow(/already exists/);
  });

  it("getRun returns undefined for unknown id", () => {
    expect(service.getRun("nope")).toBeUndefined();
  });

  it("getRun returns the stored run", () => {
    service.createRun("r2");
    expect(service.getRun("r2")).toBeDefined();
  });

  // -- pauseRun – idempotency -------------------------------------------------

  it("pauseRun transitions a Running run to Paused", async () => {
    service.createRun("r3");
    await service.pauseRun("r3", "high drift");
    expect(service.getRun("r3")?.status).toBe(BackfillRunStatus.Paused);
    expect(service.getRun("r3")?.pauseReason).toBe("high drift");
  });

  it("pauseRun is idempotent: calling twice does not throw", async () => {
    service.createRun("r4");
    await service.pauseRun("r4", "reason");
    await expect(service.pauseRun("r4", "reason again")).resolves.toBeUndefined();
    expect(service.getRun("r4")?.status).toBe(BackfillRunStatus.Paused);
  });

  it("pauseRun auto-creates the run when it does not exist", async () => {
    await service.pauseRun("r-new", "auto-pause");
    const run = service.getRun("r-new");
    expect(run).toBeDefined();
    expect(run?.status).toBe(BackfillRunStatus.Paused);
    expect(run?.pauseReason).toBe("auto-pause");
  });

  it("pauseRun is a no-op for a Completed run", async () => {
    service.createRun("r5");
    await service.completeRun("r5");
    await service.pauseRun("r5", "too late");
    expect(service.getRun("r5")?.status).toBe(BackfillRunStatus.Completed);
  });

  it("pauseRun is a no-op for a Failed run", async () => {
    service.createRun("r6");
    await service.failRun("r6", "boom");
    await service.pauseRun("r6", "too late");
    expect(service.getRun("r6")?.status).toBe(BackfillRunStatus.Failed);
  });

  // -- markAlertAcknowledged gate ---------------------------------------------

  it("markAlertAcknowledged throws for unknown run", () => {
    expect(() => service.markAlertAcknowledged("no-run", "k")).toThrow(/Run not found/);
  });

  it("markAlertAcknowledged throws when run is not Paused", async () => {
    service.createRun("r7");
    expect(() => service.markAlertAcknowledged("r7", "k")).toThrow(/not paused/);
  });

  it("markAlertAcknowledged throws when alert is not in the router", async () => {
    service.createRun("r8");
    await service.pauseRun("r8", "reason");
    expect(() => service.markAlertAcknowledged("r8", "missing-alert")).toThrow(/Alert not found in router/);
  });

  it("markAlertAcknowledged throws when alert is still Open", async () => {
    service.createRun("r9");
    await service.pauseRun("r9", "reason");
    await router.routeAlert("alert-key-r9", Severity.HIGH, "msg");
    expect(() => service.markAlertAcknowledged("r9", "alert-key-r9")).toThrow(/not been acknowledged/);
  });

  it("markAlertAcknowledged succeeds when alert is Acknowledged", async () => {
    service.createRun("r10");
    await service.pauseRun("r10", "reason");
    await router.routeAlert("alert-key-r10", Severity.HIGH, "msg");
    router.acknowledgeAlert("alert-key-r10");
    expect(() => service.markAlertAcknowledged("r10", "alert-key-r10")).not.toThrow();
    expect(service.getRun("r10")?.alertAcknowledged).toBe(true);
  });

  // -- resumeRun gate ---------------------------------------------------------

  it("resumeRun throws for unknown run", async () => {
    await expect(service.resumeRun("no-run")).rejects.toThrow(/Run not found/);
  });

  it("resumeRun throws when run is not Paused", async () => {
    service.createRun("r11");
    await expect(service.resumeRun("r11")).rejects.toThrow(/not paused/);
  });

  it("resumeRun throws when alert has not been acknowledged", async () => {
    service.createRun("r12");
    await service.pauseRun("r12", "reason");
    await expect(service.resumeRun("r12")).rejects.toThrow(/not been acknowledged/);
  });

  it("resumeRun succeeds and transitions back to Running when gate is satisfied", async () => {
    service.createRun("r13");
    await service.pauseRun("r13", "reason");
    await router.routeAlert("alert-key-r13", Severity.HIGH, "msg");
    router.acknowledgeAlert("alert-key-r13");
    service.markAlertAcknowledged("r13", "alert-key-r13");
    await service.resumeRun("r13");
    const run = service.getRun("r13");
    expect(run?.status).toBe(BackfillRunStatus.Running);
    expect(run?.pauseReason).toBeUndefined();
    expect(run?.alertAcknowledged).toBe(false);
  });

  // -- Terminal transitions ---------------------------------------------------

  it("completeRun marks run Completed", async () => {
    service.createRun("r14");
    await service.completeRun("r14");
    expect(service.getRun("r14")?.status).toBe(BackfillRunStatus.Completed);
  });

  it("completeRun throws for unknown run", async () => {
    await expect(service.completeRun("nope")).rejects.toThrow(/Run not found/);
  });

  it("failRun marks run Failed with reason", async () => {
    service.createRun("r15");
    await service.failRun("r15", "indexer crashed");
    const run = service.getRun("r15");
    expect(run?.status).toBe(BackfillRunStatus.Failed);
    expect(run?.pauseReason).toBe("indexer crashed");
  });

  it("failRun throws for unknown run", async () => {
    await expect(service.failRun("nope", "reason")).rejects.toThrow(/Run not found/);
  });

  // -- Utilities --------------------------------------------------------------

  it("clearRuns empties the run store", () => {
    service.createRun("r16");
    service.clearRuns();
    expect(service.getRun("r16")).toBeUndefined();
  });

  // -- Singleton --------------------------------------------------------------

  it("getInstance returns the same object on repeated calls", () => {
    const a = BackfillService.getInstance();
    const b = BackfillService.getInstance();
    expect(a).toBe(b);
  });

  it("resetInstance causes the next getInstance to return a fresh object", () => {
    const before = BackfillService.getInstance();
    BackfillService.resetInstance();
    const after = BackfillService.getInstance();
    expect(before).not.toBe(after);
  });
});

// ---------------------------------------------------------------------------
// 5. ReconciliationWorker – integration tests
// ---------------------------------------------------------------------------

describe("ReconciliationWorker", () => {
  let router: AlertRouter;
  let service: BackfillService;
  let worker: ReconciliationWorker;
  let sentAlerts: Alert[];
  let mockChannel: NotificationChannel;

  beforeEach(() => {
    ({ router, service, worker } = makeServices());
    sentAlerts = [];
    mockChannel = { send: jest.fn(async (alert: Alert) => { sentAlerts.push(alert); }) };
    router.setCriticalChannel(mockChannel);
    router.setStandardChannel(mockChannel);
  });

  // -- LOW drift --------------------------------------------------------------

  it("processes LOW drift: returns LOW, does not pause, does not alert (0 mismatches)", async () => {
    const severity = await worker.processDriftReport(makeReport(0, 0, "run-L0"));
    expect(severity).toBe(Severity.LOW);
    expect(service.getRun("run-L0")).toBeUndefined();
    expect(sentAlerts).toHaveLength(0);
  });

  it("processes LOW drift: returns LOW and routes a standard alert (1 mismatch)", async () => {
    const severity = await worker.processDriftReport(makeReport(1, 0, "run-L1"));
    expect(severity).toBe(Severity.LOW);
    expect(service.getRun("run-L1")).toBeUndefined(); // not paused
    expect(sentAlerts).toHaveLength(1);
    expect(sentAlerts[0].severity).toBe(Severity.LOW);
  });

  // -- MEDIUM drift -----------------------------------------------------------

  it("processes MEDIUM drift: returns MEDIUM and routes a standard alert", async () => {
    const severity = await worker.processDriftReport(makeReport(50, 0, "run-M"));
    expect(severity).toBe(Severity.MEDIUM);
    expect(service.getRun("run-M")).toBeUndefined();
    expect(sentAlerts).toHaveLength(1);
    expect(sentAlerts[0].severity).toBe(Severity.MEDIUM);
  });

  // -- HIGH drift – invoice threshold -----------------------------------------

  it("processes HIGH drift (>100 invoices): pauses run and routes critical alert", async () => {
    service.createRun("run-H1");
    const severity = await worker.processDriftReport(makeReport(101, 0, "run-H1"));
    expect(severity).toBe(Severity.HIGH);
    expect(service.getRun("run-H1")?.status).toBe(BackfillRunStatus.Paused);
    expect(sentAlerts).toHaveLength(1);
    expect(sentAlerts[0].severity).toBe(Severity.HIGH);
  });

  // -- HIGH drift – settlement mismatch absolute rule -------------------------

  it("processes HIGH drift (settlement mismatch): pauses run and routes critical alert", async () => {
    service.createRun("run-H2");
    const severity = await worker.processDriftReport(makeReport(0, 1, "run-H2"));
    expect(severity).toBe(Severity.HIGH);
    expect(service.getRun("run-H2")?.status).toBe(BackfillRunStatus.Paused);
    expect(sentAlerts).toHaveLength(1);
    expect(sentAlerts[0].severity).toBe(Severity.HIGH);
  });

  // -- HIGH drift – auto-pause when run does not exist yet --------------------

  it("HIGH drift auto-pauses even when run is not pre-registered", async () => {
    // run "run-lazy" was never explicitly created
    const severity = await worker.processDriftReport(makeReport(200, 0, "run-lazy"));
    expect(severity).toBe(Severity.HIGH);
    expect(service.getRun("run-lazy")?.status).toBe(BackfillRunStatus.Paused);
  });

  // -- Idempotency: already-paused run ----------------------------------------

  it("idempotent: consecutive HIGH reports do not throw even if run is already paused", async () => {
    service.createRun("run-idem");
    // First report → pauses
    await worker.processDriftReport(makeReport(101, 0, "run-idem"));
    // Second report → run is already paused; should be silent no-op for pause
    await expect(
      worker.processDriftReport(makeReport(101, 0, "run-idem"))
    ).resolves.toBe(Severity.HIGH);
    expect(service.getRun("run-idem")?.status).toBe(BackfillRunStatus.Paused);
  });

  // -- Alert deduplication: consecutive HIGH runs with same drift -------------

  it("alert deduplication: second HIGH run with same runId does not re-fire alert", async () => {
    service.createRun("run-dedup");
    // First pass
    await worker.processDriftReport(makeReport(101, 0, "run-dedup"));
    const countAfterFirst = sentAlerts.length;
    expect(countAfterFirst).toBe(1);
    // Second pass (same runId, same drift) → deduplicated
    await worker.processDriftReport(makeReport(101, 0, "run-dedup"));
    expect(sentAlerts).toHaveLength(countAfterFirst); // no new alerts
  });

  it("alert deduplication: three consecutive MEDIUM reports produce only one alert", async () => {
    for (let i = 0; i < 3; i++) {
      await worker.processDriftReport(makeReport(50, 0, "run-med-dedup"));
    }
    expect(sentAlerts).toHaveLength(1);
  });

  // -- Authorization gate: resume blocked before acknowledgement --------------

  it("operator cannot resume a paused run before acknowledging the alert", async () => {
    service.createRun("run-gate");
    await worker.processDriftReport(makeReport(101, 0, "run-gate"));
    await expect(service.resumeRun("run-gate")).rejects.toThrow(/not been acknowledged/);
  });

  it("operator can resume after the full ack flow is completed", async () => {
    service.createRun("run-flow");
    await worker.processDriftReport(makeReport(200, 0, "run-flow"));

    // Step 1: acknowledge alert via alertRouter
    const alertKey = buildAlertKey("run-flow");
    router.acknowledgeAlert(alertKey);

    // Step 2: mark acknowledged in backfill service
    service.markAlertAcknowledged("run-flow", alertKey);

    // Step 3: resume
    await expect(service.resumeRun("run-flow")).resolves.toBeUndefined();
    expect(service.getRun("run-flow")?.status).toBe(BackfillRunStatus.Running);
  });

  // -- Different run IDs get independent alerts --------------------------------

  it("different runIds get independent alert keys (no cross-contamination)", async () => {
    service.createRun("run-A");
    service.createRun("run-B");
    await worker.processDriftReport(makeReport(101, 0, "run-A"));
    await worker.processDriftReport(makeReport(101, 0, "run-B"));
    expect(sentAlerts).toHaveLength(2); // one per run
  });
});

// ---------------------------------------------------------------------------
// 6. End-to-end operator recovery workflow
// ---------------------------------------------------------------------------

describe("End-to-end: HIGH drift → pause → acknowledge → resume", () => {
  it("completes the full operator recovery flow without errors", async () => {
    const { router, service, worker } = makeServices();
    const runId = "e2e-run";
    const alertKey = buildAlertKey(runId);

    // Pre-register the run
    service.createRun(runId);
    expect(service.getRun(runId)?.status).toBe(BackfillRunStatus.Running);

    // Worker detects HIGH drift and triggers auto-pause
    const severity = await worker.processDriftReport(makeReport(200, 2, runId));
    expect(severity).toBe(Severity.HIGH);
    expect(service.getRun(runId)?.status).toBe(BackfillRunStatus.Paused);
    expect(router.hasOpenAlert(alertKey)).toBe(true);

    // Operator cannot resume yet
    await expect(service.resumeRun(runId)).rejects.toThrow();

    // Operator acknowledges the alert in the alert router
    router.acknowledgeAlert(alertKey);
    expect(router.hasOpenAlert(alertKey)).toBe(false);

    // Operator marks acknowledgement in the backfill service
    service.markAlertAcknowledged(runId, alertKey);

    // Operator resumes the run
    await service.resumeRun(runId);
    expect(service.getRun(runId)?.status).toBe(BackfillRunStatus.Running);

    // A new identical HIGH drift triggers a fresh alert (old one was acked)
    const refiredAlerts: Alert[] = [];
    router.setCriticalChannel({ send: async (a) => { refiredAlerts.push(a); } });
    await worker.processDriftReport(makeReport(200, 2, runId));
    expect(refiredAlerts).toHaveLength(1); // new alert, dedup key is fresh
  });
});

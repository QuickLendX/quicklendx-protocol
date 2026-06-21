/**
 * Unit tests for LagMonitor alerting, hysteresis, and degraded-mode
 * auto-recovery. Complements lagMonitor.test.ts (which covers the pure
 * computeLevel / threshold / singleton behaviour).
 *
 * Tests cover:
 *  - Alerts fire only on effective-level transitions, not on every poll
 *  - Escalation is immediate; de-escalation requires sustained recovery
 *  - Hysteresis margin prevents flapping around a threshold
 *  - Sustained breach holds the degraded level
 *  - Rapid recovery still drains one level at a time (warn guard window kept)
 *  - Missing / corrupt current-ledger reads fail safe to critical
 *  - Alert payloads carry only operational fields (no secrets)
 *  - getLagStatus() escalates immediately but never auto-clears
 */

import {
  LagMonitor,
  LagAlertEvent,
  DEFAULT_HYSTERESIS_MARGIN,
  DEFAULT_RECOVERY_POLLS,
} from "../services/lagMonitor";
import { statusService } from "../services/statusService";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** warn=10, critical=50, margin=3, recoveryPolls=3 unless overridden. */
function makeMonitor(
  warn = 10,
  critical = 50,
  margin = 3,
  recoveryPolls = 3
): LagMonitor {
  return new LagMonitor(warn, critical, margin, recoveryPolls);
}

/** Set the mocked lag by adjusting the mock current ledger. */
function setLag(lag: number): void {
  statusService.updateLastIndexedLedger(100000);
  statusService.setMockCurrentLedger(100000 + lag);
}

function collectAlerts(m: LagMonitor): LagAlertEvent[] {
  const events: LagAlertEvent[] = [];
  m.onAlert((e) => events.push(e));
  return events;
}

beforeEach(() => {
  statusService.setMaintenanceMode(false);
  statusService.updateLastIndexedLedger(100000);
});

afterEach(() => {
  statusService.setMockCurrentLedger(null);
});

// ---------------------------------------------------------------------------
// Config / defaults
// ---------------------------------------------------------------------------

describe("LagMonitor hysteresis config", () => {
  it("exposes default hysteresis margin and recovery polls", () => {
    const m = makeMonitor();
    expect(DEFAULT_HYSTERESIS_MARGIN).toBe(3);
    expect(DEFAULT_RECOVERY_POLLS).toBe(3);
    expect(m.hysteresisMargin).toBe(3);
    expect(m.recoveryPolls).toBe(3);
  });

  it("reads hysteresis config from env vars", () => {
    process.env["LAG_HYSTERESIS_MARGIN"] = "5";
    process.env["LAG_RECOVERY_POLLS"] = "4";
    const m = new LagMonitor();
    expect(m.hysteresisMargin).toBe(5);
    expect(m.recoveryPolls).toBe(4);
    delete process.env["LAG_HYSTERESIS_MARGIN"];
    delete process.env["LAG_RECOVERY_POLLS"];
  });

  it("setHysteresis validates inputs", () => {
    const m = makeMonitor();
    expect(() => m.setHysteresis(-1, 3)).toThrow(RangeError);
    expect(() => m.setHysteresis(3, 0)).toThrow(RangeError);
    expect(() => m.setHysteresis(3, 1.5)).toThrow(RangeError);
    m.setHysteresis(4, 2);
    expect(m.hysteresisMargin).toBe(4);
    expect(m.recoveryPolls).toBe(2);
  });

  it("clamps invalid constructor values to safe defaults", () => {
    const m = new LagMonitor(10, 50, -5, 0);
    expect(m.hysteresisMargin).toBe(0);
    expect(m.recoveryPolls).toBe(DEFAULT_RECOVERY_POLLS);
  });
});

// ---------------------------------------------------------------------------
// Escalation (immediate)
// ---------------------------------------------------------------------------

describe("LagMonitor escalation", () => {
  it("escalates none→warn immediately on first breaching poll", async () => {
    const m = makeMonitor();
    const alerts = collectAlerts(m);

    setLag(15);
    const status = await m.poll();

    expect(status.level).toBe("warn");
    expect(status.isDegraded).toBe(true);
    expect(alerts).toHaveLength(1);
    expect(alerts[0]).toMatchObject({
      from: "none",
      to: "warn",
      direction: "escalation",
      lag: 15,
    });
  });

  it("escalates warn→critical immediately", async () => {
    const m = makeMonitor();
    const alerts = collectAlerts(m);

    setLag(15);
    await m.poll(); // none -> warn
    setLag(60);
    const status = await m.poll(); // warn -> critical

    expect(status.level).toBe("critical");
    expect(status.isCritical).toBe(true);
    expect(alerts.map((a) => a.to)).toEqual(["warn", "critical"]);
  });

  it("can jump none→critical in a single poll", async () => {
    const m = makeMonitor();
    const alerts = collectAlerts(m);

    setLag(80);
    const status = await m.poll();

    expect(status.level).toBe("critical");
    expect(alerts).toHaveLength(1);
    expect(alerts[0]).toMatchObject({ from: "none", to: "critical" });
  });
});

// ---------------------------------------------------------------------------
// Alerts fire only on transitions
// ---------------------------------------------------------------------------

describe("LagMonitor emits alerts only on transitions", () => {
  it("does not emit on repeated polls at the same level", async () => {
    const m = makeMonitor();
    const alerts = collectAlerts(m);

    setLag(20);
    await m.poll(); // none -> warn (1 alert)
    await m.poll(); // still warn
    await m.poll(); // still warn
    await m.poll(); // still warn

    expect(alerts).toHaveLength(1);
    expect(m.getAlertMetrics().escalations).toBe(1);
  });

  it("does not emit while healthy across many polls", async () => {
    const m = makeMonitor();
    const alerts = collectAlerts(m);

    setLag(2);
    await m.poll();
    await m.poll();
    await m.poll();

    expect(alerts).toHaveLength(0);
    expect(m.effectiveLevel).toBe("none");
  });
});

// ---------------------------------------------------------------------------
// De-escalation requires sustained recovery
// ---------------------------------------------------------------------------

describe("LagMonitor auto-recovery (sustained)", () => {
  it("does not clear warn until lag stays below recovery threshold for N polls", async () => {
    const m = makeMonitor(10, 50, 3, 3); // recovery threshold for warn = 10-3 = 7
    const alerts = collectAlerts(m);

    setLag(20);
    await m.poll(); // -> warn
    expect(m.effectiveLevel).toBe("warn");

    setLag(5); // below recovery threshold (7)
    await m.poll(); // streak 1 — still warn
    expect(m.effectiveLevel).toBe("warn");
    await m.poll(); // streak 2 — still warn
    expect(m.effectiveLevel).toBe("warn");

    const status = await m.poll(); // streak 3 — clears
    expect(status.level).toBe("none");
    expect(m.effectiveLevel).toBe("none");

    const recovery = alerts.filter((a) => a.direction === "recovery");
    expect(recovery).toHaveLength(1);
    expect(recovery[0]).toMatchObject({ from: "warn", to: "none" });
  });

  it("requires lag below the warn recovery threshold, not merely below warn", async () => {
    const m = makeMonitor(10, 50, 3, 2); // recovery threshold = 7
    const alerts = collectAlerts(m);

    setLag(20);
    await m.poll(); // -> warn

    // lag = 8 is below warn (10) but above recovery threshold (7): no progress
    setLag(8);
    await m.poll();
    await m.poll();
    expect(m.effectiveLevel).toBe("warn");
    expect(alerts.filter((a) => a.direction === "recovery")).toHaveLength(0);

    // drop to recovery range and dwell
    setLag(7);
    await m.poll(); // streak 1
    await m.poll(); // streak 2 -> clears
    expect(m.effectiveLevel).toBe("none");
  });

  it("steps critical→warn→none one level per recovery window", async () => {
    const m = makeMonitor(10, 50, 3, 2);
    const alerts = collectAlerts(m);

    setLag(80);
    await m.poll(); // -> critical

    // recovery threshold for critical = 50-3 = 47
    setLag(10);
    await m.poll(); // streak 1 (critical)
    await m.poll(); // streak 2 -> step down to warn
    expect(m.effectiveLevel).toBe("warn");

    // recovery threshold for warn = 10-3 = 7; lag 10 is NOT in range yet
    await m.poll();
    await m.poll();
    expect(m.effectiveLevel).toBe("warn");

    setLag(5);
    await m.poll(); // streak 1 (warn)
    await m.poll(); // streak 2 -> none
    expect(m.effectiveLevel).toBe("none");

    expect(alerts.map((a) => `${a.from}->${a.to}`)).toEqual([
      "none->critical",
      "critical->warn",
      "warn->none",
    ]);
  });
});

// ---------------------------------------------------------------------------
// Flapping around a threshold
// ---------------------------------------------------------------------------

describe("LagMonitor flapping resistance", () => {
  it("a single good poll does not clear a degraded state (anti-flap)", async () => {
    const m = makeMonitor(10, 50, 3, 3);
    const alerts = collectAlerts(m);

    setLag(20);
    await m.poll(); // -> warn

    // oscillate around the recovery threshold
    setLag(5); // streak 1
    await m.poll();
    setLag(20); // breach again -> reset streak, still warn (no new alert)
    await m.poll();
    setLag(5); // streak 1 again
    await m.poll();
    setLag(11); // above recovery -> reset
    await m.poll();

    expect(m.effectiveLevel).toBe("warn");
    // Only the original escalation alert; flapping produced no transitions.
    expect(alerts).toHaveLength(1);
    expect(alerts[0].direction).toBe("escalation");
  });

  it("resets recovery streak the moment lag bounces above recovery threshold", async () => {
    const m = makeMonitor(10, 50, 3, 3);
    setLag(20);
    await m.poll(); // -> warn
    setLag(5);
    await m.poll(); // streak 1
    await m.poll(); // streak 2
    expect(m.getAlertMetrics().consecutiveRecoveryPolls).toBe(2);

    setLag(15); // bounce above recovery
    await m.poll();
    expect(m.getAlertMetrics().consecutiveRecoveryPolls).toBe(0);
    expect(m.effectiveLevel).toBe("warn");
  });
});

// ---------------------------------------------------------------------------
// Sustained breach
// ---------------------------------------------------------------------------

describe("LagMonitor sustained breach", () => {
  it("holds critical across a long breach with no duplicate alerts", async () => {
    const m = makeMonitor();
    const alerts = collectAlerts(m);

    setLag(120);
    for (let i = 0; i < 10; i++) await m.poll();

    expect(m.effectiveLevel).toBe("critical");
    expect(alerts).toHaveLength(1);
    expect(alerts[0].to).toBe("critical");
  });
});

// ---------------------------------------------------------------------------
// Missing / corrupt current-ledger read
// ---------------------------------------------------------------------------

describe("LagMonitor missing current-ledger read", () => {
  it("fails safe to critical when statusService throws", async () => {
    const m = makeMonitor();
    const spy = jest
      .spyOn(statusService, "getStatus")
      .mockRejectedValueOnce(new Error("rpc unavailable"));

    const status = await m.poll();
    expect(status.level).toBe("critical");
    expect(status.isCritical).toBe(true);
    expect(status.lag).toBe(m.criticalThreshold);

    spy.mockRestore();
  });

  it("fails safe to critical on a negative / nonsensical lag", async () => {
    const m = makeMonitor();
    // current ledger behind last-indexed → negative lag
    statusService.updateLastIndexedLedger(100000);
    statusService.setMockCurrentLedger(99990);

    const status = await m.poll();
    expect(status.level).toBe("critical");
    expect(status.lag).toBe(m.criticalThreshold);
  });

  it("a failed read does not silently clear an existing degraded state", async () => {
    const m = makeMonitor();
    setLag(20);
    await m.poll(); // -> warn

    const spy = jest
      .spyOn(statusService, "getStatus")
      .mockRejectedValueOnce(new Error("rpc unavailable"));
    const status = await m.poll();
    // read failed → treated as critical, which is an escalation, never a clear
    expect(status.level).toBe("critical");
    spy.mockRestore();
  });
});

// ---------------------------------------------------------------------------
// getLagStatus contract (instantaneous, side-effect free)
// ---------------------------------------------------------------------------

describe("LagMonitor.getLagStatus contract", () => {
  it("reflects the current lag instantaneously (no poll required)", async () => {
    const m = makeMonitor();
    setLag(60);
    const status = await m.getLagStatus();
    expect(status.level).toBe("critical");
    expect(status.isCritical).toBe(true);
  });

  it("is side-effect free: never advances the state machine or emits alerts", async () => {
    const m = makeMonitor();
    const alerts = collectAlerts(m);
    setLag(60);
    await m.getLagStatus();
    await m.getLagStatus();
    await m.getLagStatus();
    expect(alerts).toHaveLength(0);
    expect(m.effectiveLevel).toBe("none");
  });

  it("tracks lag down again immediately (instantaneous, unlike effective level)", async () => {
    const m = makeMonitor();
    setLag(60);
    expect((await m.getLagStatus()).level).toBe("critical");
    setLag(0);
    expect((await m.getLagStatus()).level).toBe("none");
  });

  it("getEffectiveStatus reflects hysteresis without advancing it", async () => {
    const m = makeMonitor();
    setLag(60);
    await m.poll(); // effective -> critical
    setLag(0); // lag healthy again, but no sustained recovery yet
    const eff = await m.getEffectiveStatus();
    expect(eff.level).toBe("critical");
    expect(eff.lag).toBe(0);
    // getEffectiveStatus did not advance the machine
    expect(m.effectiveLevel).toBe("critical");
  });

  it("preserves the LagStatus shape", async () => {
    const m = makeMonitor();
    setLag(5);
    const status = await m.getLagStatus();
    expect(Object.keys(status).sort()).toEqual(
      [
        "checkedAt",
        "criticalThreshold",
        "isCritical",
        "isDegraded",
        "lag",
        "level",
        "warnThreshold",
      ].sort()
    );
    expect(status.checkedAt).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });
});

// ---------------------------------------------------------------------------
// Alert payload safety (no secrets)
// ---------------------------------------------------------------------------

describe("LagMonitor alert payload safety", () => {
  it("alert payload contains only operational fields", async () => {
    const m = makeMonitor();
    const alerts = collectAlerts(m);
    setLag(60);
    await m.poll();

    expect(alerts).toHaveLength(1);
    expect(Object.keys(alerts[0]).sort()).toEqual(
      [
        "at",
        "criticalThreshold",
        "direction",
        "from",
        "lag",
        "to",
        "warnThreshold",
      ].sort()
    );
    // No string field should resemble a secret/token/wallet/auth value.
    const serialized = JSON.stringify(alerts[0]);
    expect(serialized).not.toMatch(/secret|token|password|signature|apikey|authorization/i);
  });
});

// ---------------------------------------------------------------------------
// Metrics & subscription lifecycle
// ---------------------------------------------------------------------------

describe("LagMonitor metrics & subscriptions", () => {
  it("tracks escalation/recovery counts and transitionsTo", async () => {
    const m = makeMonitor(10, 50, 3, 1); // recover after a single good poll
    setLag(60);
    await m.poll(); // none->critical
    setLag(0);
    await m.poll(); // critical->warn
    await m.poll(); // warn->none

    const metrics = m.getAlertMetrics();
    expect(metrics.escalations).toBe(1);
    expect(metrics.recoveries).toBe(2);
    expect(metrics.transitionsTo.critical).toBe(1);
    expect(metrics.transitionsTo.warn).toBe(1);
    expect(metrics.transitionsTo.none).toBe(1);
    expect(metrics.currentLevel).toBe("none");
  });

  it("getAlertMetrics returns a defensive copy", async () => {
    const m = makeMonitor();
    const snap = m.getAlertMetrics();
    snap.escalations = 999;
    snap.transitionsTo.warn = 999;
    expect(m.getAlertMetrics().escalations).toBe(0);
    expect(m.getAlertMetrics().transitionsTo.warn).toBe(0);
  });

  it("unsubscribe stops further alert delivery", async () => {
    const m = makeMonitor();
    const events: LagAlertEvent[] = [];
    const unsub = m.onAlert((e) => events.push(e));

    setLag(20);
    await m.poll(); // -> warn (delivered)
    unsub();
    setLag(60);
    await m.poll(); // -> critical (not delivered)

    expect(events).toHaveLength(1);
    expect(events[0].to).toBe("warn");
  });

  it("a throwing listener never breaks the monitor", async () => {
    const m = makeMonitor();
    m.onAlert(() => {
      throw new Error("boom");
    });
    setLag(60);
    await expect(m.poll()).resolves.toMatchObject({ level: "critical" });
  });

  it("reset() clears state and counters", async () => {
    const m = makeMonitor();
    setLag(60);
    await m.poll();
    m.reset();
    expect(m.effectiveLevel).toBe("none");
    expect(m.getAlertMetrics().escalations).toBe(0);
    expect(m.getAlertMetrics().currentLevel).toBe("none");
  });
});

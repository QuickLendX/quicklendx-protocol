/**
 * Unit tests for LagMonitor service.
 *
 * Tests cover:
 *  - computeLevel() pure function across all threshold boundaries
 *  - getLagStatus() integration with statusService
 *  - isDegraded() / isCritical() convenience helpers
 *  - Threshold validation (setThresholds)
 *  - Singleton behaviour
 *  - Environment-variable-driven threshold initialisation
 */

import {
  LagMonitor,
  lagMonitor,
  DEFAULT_WARN_THRESHOLD,
  DEFAULT_CRITICAL_THRESHOLD,
} from "../services/lagMonitor";
import { statusService } from "../services/statusService";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeFreshMonitor(warn = 10, critical = 50): LagMonitor {
  return new LagMonitor(warn, critical);
}

// ---------------------------------------------------------------------------
// computeLevel – pure function
// ---------------------------------------------------------------------------

describe("LagMonitor.computeLevel", () => {
  const monitor = makeFreshMonitor(10, 50);

  it('returns "none" when lag is 0', () => {
    expect(monitor.computeLevel(0)).toBe("none");
  });

  it('returns "none" when lag is below warn threshold', () => {
    expect(monitor.computeLevel(9)).toBe("none");
  });

  it('returns "warn" exactly at warn threshold', () => {
    expect(monitor.computeLevel(10)).toBe("warn");
  });

  it('returns "warn" between warn and critical thresholds', () => {
    expect(monitor.computeLevel(25)).toBe("warn");
    expect(monitor.computeLevel(49)).toBe("warn");
  });

  it('returns "critical" exactly at critical threshold', () => {
    expect(monitor.computeLevel(50)).toBe("critical");
  });

  it('returns "critical" above critical threshold', () => {
    expect(monitor.computeLevel(100)).toBe("critical");
    expect(monitor.computeLevel(999)).toBe("critical");
  });

  it("respects custom thresholds", () => {
    const m = makeFreshMonitor(5, 20);
    expect(m.computeLevel(4)).toBe("none");
    expect(m.computeLevel(5)).toBe("warn");
    expect(m.computeLevel(19)).toBe("warn");
    expect(m.computeLevel(20)).toBe("critical");
  });
});

// ---------------------------------------------------------------------------
// setThresholds – validation
// ---------------------------------------------------------------------------

describe("LagMonitor.setThresholds", () => {
  it("updates thresholds when valid", () => {
    const m = makeFreshMonitor();
    m.setThresholds(5, 30);
    expect(m.warnThreshold).toBe(5);
    expect(m.criticalThreshold).toBe(30);
  });

  it("throws RangeError when warn >= critical", () => {
    const m = makeFreshMonitor();
    expect(() => m.setThresholds(50, 50)).toThrow(RangeError);
    expect(() => m.setThresholds(60, 50)).toThrow(RangeError);
  });

  it("throws RangeError when warn is zero", () => {
    const m = makeFreshMonitor();
    expect(() => m.setThresholds(0, 50)).toThrow(RangeError);
  });

  it("throws RangeError when critical is zero", () => {
    const m = makeFreshMonitor();
    expect(() => m.setThresholds(5, 0)).toThrow(RangeError);
  });

  it("throws RangeError when either threshold is negative", () => {
    const m = makeFreshMonitor();
    expect(() => m.setThresholds(-1, 50)).toThrow(RangeError);
    expect(() => m.setThresholds(5, -1)).toThrow(RangeError);
  });
});

// ---------------------------------------------------------------------------
// Default threshold constants
// ---------------------------------------------------------------------------

describe("Default threshold constants", () => {
  it("DEFAULT_WARN_THRESHOLD is 10", () => {
    expect(DEFAULT_WARN_THRESHOLD).toBe(10);
  });

  it("DEFAULT_CRITICAL_THRESHOLD is 50", () => {
    expect(DEFAULT_CRITICAL_THRESHOLD).toBe(50);
  });

  it("fresh LagMonitor uses defaults when no args given", () => {
    // Temporarily clear env vars to test pure defaults
    const origWarn = process.env["LAG_WARN_THRESHOLD"];
    const origCrit = process.env["LAG_CRITICAL_THRESHOLD"];
    delete process.env["LAG_WARN_THRESHOLD"];
    delete process.env["LAG_CRITICAL_THRESHOLD"];

    const m = new LagMonitor();
    expect(m.warnThreshold).toBe(DEFAULT_WARN_THRESHOLD);
    expect(m.criticalThreshold).toBe(DEFAULT_CRITICAL_THRESHOLD);

    process.env["LAG_WARN_THRESHOLD"] = origWarn;
    process.env["LAG_CRITICAL_THRESHOLD"] = origCrit;
  });

  it("reads thresholds from environment variables", () => {
    process.env["LAG_WARN_THRESHOLD"] = "15";
    process.env["LAG_CRITICAL_THRESHOLD"] = "75";

    const m = new LagMonitor();
    expect(m.warnThreshold).toBe(15);
    expect(m.criticalThreshold).toBe(75);

    delete process.env["LAG_WARN_THRESHOLD"];
    delete process.env["LAG_CRITICAL_THRESHOLD"];
  });

  it("falls back to defaults when env vars are non-numeric", () => {
    process.env["LAG_WARN_THRESHOLD"] = "not-a-number";
    process.env["LAG_CRITICAL_THRESHOLD"] = "also-bad";

    const m = new LagMonitor();
    expect(m.warnThreshold).toBe(DEFAULT_WARN_THRESHOLD);
    expect(m.criticalThreshold).toBe(DEFAULT_CRITICAL_THRESHOLD);

    delete process.env["LAG_WARN_THRESHOLD"];
    delete process.env["LAG_CRITICAL_THRESHOLD"];
  });
});

// ---------------------------------------------------------------------------
// getLagStatus – integration with statusService
// ---------------------------------------------------------------------------

describe("LagMonitor.getLagStatus", () => {
  beforeEach(() => {
    statusService.setMaintenanceMode(false);
    statusService.updateLastIndexedLedger(100000);
    statusService.setMockCurrentLedger(100005); // lag = 5
  });

  afterEach(() => {
    statusService.setMockCurrentLedger(null);
  });

  it("returns none level when lag is below warn threshold", async () => {
    const m = makeFreshMonitor(10, 50);
    const result = await m.getLagStatus();

    expect(result.lag).toBe(5);
    expect(result.level).toBe("none");
    expect(result.isDegraded).toBe(false);
    expect(result.isCritical).toBe(false);
    expect(result.warnThreshold).toBe(10);
    expect(result.criticalThreshold).toBe(50);
    expect(result.checkedAt).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });

  it("returns warn level when lag is at warn threshold", async () => {
    statusService.setMockCurrentLedger(100010); // lag = 10
    const m = makeFreshMonitor(10, 50);
    const result = await m.getLagStatus();

    expect(result.lag).toBe(10);
    expect(result.level).toBe("warn");
    expect(result.isDegraded).toBe(true);
    expect(result.isCritical).toBe(false);
  });

  it("returns warn level when lag is between thresholds", async () => {
    statusService.setMockCurrentLedger(100030); // lag = 30
    const m = makeFreshMonitor(10, 50);
    const result = await m.getLagStatus();

    expect(result.level).toBe("warn");
    expect(result.isDegraded).toBe(true);
    expect(result.isCritical).toBe(false);
  });

  it("returns critical level when lag is at critical threshold", async () => {
    statusService.setMockCurrentLedger(100050); // lag = 50
    const m = makeFreshMonitor(10, 50);
    const result = await m.getLagStatus();

    expect(result.lag).toBe(50);
    expect(result.level).toBe("critical");
    expect(result.isDegraded).toBe(true);
    expect(result.isCritical).toBe(true);
  });

  it("returns critical level when lag exceeds critical threshold", async () => {
    statusService.setMockCurrentLedger(100200); // lag = 200
    const m = makeFreshMonitor(10, 50);
    const result = await m.getLagStatus();

    expect(result.level).toBe("critical");
    expect(result.isCritical).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// isDegraded / isCritical convenience helpers
// ---------------------------------------------------------------------------

describe("LagMonitor.isDegraded / isCritical", () => {
  beforeEach(() => {
    statusService.updateLastIndexedLedger(100000);
  });

  afterEach(() => {
    statusService.setMockCurrentLedger(null);
  });

  it("isDegraded returns false when healthy", async () => {
    statusService.setMockCurrentLedger(100005);
    const m = makeFreshMonitor(10, 50);
    expect(await m.isDegraded()).toBe(false);
  });

  it("isDegraded returns true when warn", async () => {
    statusService.setMockCurrentLedger(100020);
    const m = makeFreshMonitor(10, 50);
    expect(await m.isDegraded()).toBe(true);
  });

  it("isDegraded returns true when critical", async () => {
    statusService.setMockCurrentLedger(100100);
    const m = makeFreshMonitor(10, 50);
    expect(await m.isDegraded()).toBe(true);
  });

  it("isCritical returns false when healthy", async () => {
    statusService.setMockCurrentLedger(100005);
    const m = makeFreshMonitor(10, 50);
    expect(await m.isCritical()).toBe(false);
  });

  it("isCritical returns false when only warn", async () => {
    statusService.setMockCurrentLedger(100020);
    const m = makeFreshMonitor(10, 50);
    expect(await m.isCritical()).toBe(false);
  });

  it("isCritical returns true when critical", async () => {
    statusService.setMockCurrentLedger(100100);
    const m = makeFreshMonitor(10, 50);
    expect(await m.isCritical()).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Singleton
// ---------------------------------------------------------------------------

describe("LagMonitor singleton", () => {
  it("getInstance returns the same instance", () => {
    const a = LagMonitor.getInstance();
    const b = LagMonitor.getInstance();
    expect(a).toBe(b);
  });

  it("exported lagMonitor is the singleton", () => {
    expect(lagMonitor).toBe(LagMonitor.getInstance());
  });
});

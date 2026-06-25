import { AlertRouter } from "../services/alertRouter";
import { Severity, AlertStatus } from "../types/reconciliation";
import { AlertTransport } from "../services/alerts/transports/AlertTransport";

// Mock transport for testing
class MockTransport implements AlertTransport {
  calls: Array<{ alertKey: string; severity: string; message: string }> = [];

  async send(alert: any): Promise<void> {
    this.calls.push({
      alertKey: alert.alertKey,
      severity: alert.severity,
      message: alert.message,
    });
  }

  reset(): void {
    this.calls = [];
  }
}

// Failing transport to test isolation
class FailingTransport implements AlertTransport {
  async send(_alert: any): Promise<void> {
    throw new Error("Transport failure");
  }
}

describe("AlertRouter", () => {
  let router: AlertRouter;
  let mockEmailTransport: MockTransport;
  let mockSlackTransport: MockTransport;
  let mockPagerDutyTransport: MockTransport;

  beforeEach(() => {
    AlertRouter.resetInstance();
    router = AlertRouter.getInstance(1000); // 1 second for testing
    mockEmailTransport = new MockTransport();
    mockSlackTransport = new MockTransport();
    mockPagerDutyTransport = new MockTransport();
  });

  describe("severity-based routing", () => {
    it("should route HIGH severity alerts to configured channels", async () => {
      // Manually set transports
      (router as any).transports.set("email", mockEmailTransport);
      (router as any).transports.set("slack", mockSlackTransport);
      (router as any).transports.set("pagerduty", mockPagerDutyTransport);

      const dispatched = await router.routeAlert(
        "test-high",
        Severity.HIGH,
        "Critical error"
      );

      expect(dispatched).toBe(true);
      // HIGH should route to pagerduty and slack (based on default config)
      expect(mockPagerDutyTransport.calls.length).toBeGreaterThan(0);
    });

    it("should route MEDIUM severity alerts to appropriate channels", async () => {
      (router as any).transports.set("slack", mockSlackTransport);
      (router as any).transports.set("email", mockEmailTransport);

      const dispatched = await router.routeAlert(
        "test-medium",
        Severity.MEDIUM,
        "Warning"
      );

      expect(dispatched).toBe(true);
    });

    it("should route LOW severity alerts to email", async () => {
      (router as any).transports.set("email", mockEmailTransport);

      const dispatched = await router.routeAlert(
        "test-low",
        Severity.LOW,
        "Info"
      );

      expect(dispatched).toBe(true);
    });
  });

  describe("deduplication within window", () => {
    it("should deduplicate alerts within the window", async () => {
      (router as any).transports.set("email", mockEmailTransport);

      // First alert should be dispatched
      const first = await router.routeAlert(
        "test-dedupe",
        Severity.LOW,
        "Message 1"
      );
      expect(first).toBe(true);
      expect(mockEmailTransport.calls.length).toBe(1);

      // Second alert with same key within window should be suppressed
      const second = await router.routeAlert(
        "test-dedupe",
        Severity.LOW,
        "Message 2"
      );
      expect(second).toBe(false);
      expect(mockEmailTransport.calls.length).toBe(1);
    });

    it("should allow alerts after deduplication window expires", async () => {
      (router as any).transports.set("email", mockEmailTransport);

      // First alert
      await router.routeAlert("test-window", Severity.LOW, "Message 1");
      expect(mockEmailTransport.calls.length).toBe(1);

      // Wait for window to expire (1.1 seconds)
      await new Promise((resolve) => setTimeout(resolve, 1100));

      // Second alert should be dispatched
      const second = await router.routeAlert(
        "test-window",
        Severity.LOW,
        "Message 2"
      );
      expect(second).toBe(true);
      expect(mockEmailTransport.calls.length).toBe(2);
    }, 5000);
  });

  describe("transport failure isolation", () => {
    it("should not block other transports on failure", async () => {
      const failingTransport = new FailingTransport();
      const successTransport = new MockTransport();

      (router as any).transports.set("failing", failingTransport);
      (router as any).transports.set("success", successTransport);

      // Override the getChannelsForSeverity to return both transports
      const originalGetChannels = (router as any).getChannelsForSeverity;
      (router as any).getChannelsForSeverity = () => ["failing", "success"];

      const dispatched = await router.routeAlert(
        "test-isolation",
        Severity.HIGH,
        "Test message"
      );

      expect(dispatched).toBe(true);
      expect(successTransport.calls.length).toBe(1);

      // Restore
      (router as any).getChannelsForSeverity = originalGetChannels;
    });
  });

  describe("alert acknowledgement", () => {
    it("should acknowledge an open alert", async () => {
      (router as any).transports.set("email", mockEmailTransport);

      await router.routeAlert("test-ack", Severity.LOW, "Message");

      router.acknowledgeAlert("test-ack");
      const alert = router.getAlert("test-ack");
      expect(alert?.status).toBe(AlertStatus.Acknowledged);
      expect(alert?.acknowledgedAt).toBeDefined();
    });

    it("should throw when acknowledging non-existent alert", () => {
      expect(() => router.acknowledgeAlert("non-existent")).toThrow(
        "Alert not found"
      );
    });

    it("should throw when acknowledging already acknowledged alert", async () => {
      (router as any).transports.set("email", mockEmailTransport);

      await router.routeAlert("test-ack2", Severity.LOW, "Message");
      router.acknowledgeAlert("test-ack2");

      expect(() => router.acknowledgeAlert("test-ack2")).toThrow(
        "already acknowledged"
      );
    });
  });

  describe("query helpers", () => {
    it("should return all alerts", async () => {
      (router as any).transports.set("email", mockEmailTransport);

      await router.routeAlert("alert-1", Severity.LOW, "Message 1");
      await router.routeAlert("alert-2", Severity.MEDIUM, "Message 2");

      // Wait for dedup window
      await new Promise((resolve) => setTimeout(resolve, 1100));

      await router.routeAlert("alert-3", Severity.HIGH, "Message 3");

      const alerts = router.getAllAlerts();
      expect(alerts.length).toBe(3);
    }, 5000);

    it("should check if alert is open", async () => {
      (router as any).transports.set("email", mockEmailTransport);

      await router.routeAlert("test-open", Severity.LOW, "Message");

      expect(router.hasOpenAlert("test-open")).toBe(true);

      router.acknowledgeAlert("test-open");
      expect(router.hasOpenAlert("test-open")).toBe(false);
    });
  });

  describe("cleanup and state management", () => {
    it("should clear all alerts and dedupe entries", async () => {
      (router as any).transports.set("email", mockEmailTransport);

      await router.routeAlert("test-clear-1", Severity.LOW, "Message");
      await router.routeAlert("test-clear-2", Severity.MEDIUM, "Message");

      expect(router.getAllAlerts().length).toBe(2);

      router.clearAlerts();

      expect(router.getAllAlerts().length).toBe(0);
      expect((router as any).dedupeWindows.size).toBe(0);
    });

    it("should clear expired dedupe entries", async () => {
      (router as any).transports.set("email", mockEmailTransport);

      await router.routeAlert("test-expire", Severity.LOW, "Message");
      expect((router as any).dedupeWindows.size).toBe(1);

      // Wait for expiration
      await new Promise((resolve) => setTimeout(resolve, 1100));

      router.clearExpiredDedupeEntries();
      expect((router as any).dedupeWindows.size).toBe(0);
    }, 5000);
  });
});

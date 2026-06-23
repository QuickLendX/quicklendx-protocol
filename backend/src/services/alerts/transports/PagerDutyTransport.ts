import { Alert } from "../../types/reconciliation";
import { AlertTransport } from "./AlertTransport";

export class PagerDutyTransport implements AlertTransport {
  private readonly integrationKey: string;

  constructor(integrationKey: string) {
    this.integrationKey = integrationKey;
  }

  async send(alert: Alert): Promise<void> {
    const payload = {
      routing_key: this.integrationKey,
      event_action: "trigger",
      dedup_key: alert.alertKey,
      payload: {
        summary: `[${alert.severity}] ${alert.message}`,
        severity: this.mapSeverity(alert.severity),
        source: "QuickLendX",
        timestamp: new Date(alert.createdAt).toISOString(),
      },
    };

    try {
      const response = await fetch("https://events.pagerduty.com/v2/enqueue", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });

      if (!response.ok) {
        throw new Error(`PagerDuty API error: ${response.statusText}`);
      }
    } catch (error) {
      console.error("Failed to send PagerDuty alert:", error);
      throw error;
    }
  }

  private mapSeverity(severity: string): "critical" | "error" | "warning" | "info" {
    switch (severity) {
      case "HIGH":
        return "critical";
      case "MEDIUM":
        return "error";
      case "LOW":
        return "warning";
      default:
        return "info";
    }
  }
}

import { Alert } from "../../../types/reconciliation";
import { AlertTransport } from "./AlertTransport";

export class SlackTransport implements AlertTransport {
  private readonly webhookUrl: string;

  constructor(webhookUrl: string) {
    this.webhookUrl = webhookUrl;
  }

  async send(alert: Alert): Promise<void> {
    const color = this.getColor(alert.severity);
    const payload = {
      attachments: [
        {
          color,
          title: `[${alert.severity}] QuickLendX Alert`,
          text: alert.message,
          fields: [
            {
              title: "Alert Key",
              value: alert.alertKey,
              short: true,
            },
            {
              title: "Severity",
              value: alert.severity,
              short: true,
            },
            {
              title: "Timestamp",
              value: new Date(alert.createdAt).toISOString(),
              short: true,
            },
          ],
          footer: "QuickLendX Alert Router",
        },
      ],
    };

    try {
      const response = await fetch(this.webhookUrl, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });

      if (!response.ok) {
        throw new Error(`Slack API error: ${response.statusText}`);
      }
    } catch (error) {
      console.error("Failed to send Slack alert:", error);
      throw error;
    }
  }

  private getColor(severity: string): string {
    switch (severity) {
      case "HIGH":
        return "#FF0000"; // Red
      case "MEDIUM":
        return "#FFA500"; // Orange
      case "LOW":
        return "#FFFF00"; // Yellow
      default:
        return "#808080"; // Gray
    }
  }
}

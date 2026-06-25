/**
 * alertRouter.ts
 *
 * Routes severity-classified alerts to appropriate notification channels based on config.
 *
 * Design decisions:
 *  - Deduplication: uses per-alert-key dedupe windows (default 15 minutes).
 *    Consecutive calls with the same key within the window are silently ignored.
 *  - Config-based routing: severity → channels mapping loaded from env (ALERT_ROUTES_JSON).
 *    Channels are pluggable transports (email, Slack, PagerDuty).
 *  - Transport isolation: failure in one transport does not block others.
 *  - Secrets redaction: sensitive URLs are logged with placeholders.
 */

import { Alert, AlertStatus, Severity } from "../types/reconciliation";
import { alertConfig } from "../config";
import { AlertTransport } from "./alerts/transports/AlertTransport";
import { EmailTransport } from "./alerts/transports/EmailTransport";
import { SlackTransport } from "./alerts/transports/SlackTransport";
import { PagerDutyTransport } from "./alerts/transports/PagerDutyTransport";

export type { Alert } from "../types/reconciliation";
export { Severity } from "../types/reconciliation";

// ---------------------------------------------------------------------------
// Alert deduplication window tracking
// ---------------------------------------------------------------------------

interface DedupeEntry {
  lastFiredAt: number; // epoch-ms
}

// ---------------------------------------------------------------------------
// AlertRouter (singleton)
// ---------------------------------------------------------------------------

export class AlertRouter {
  private static instance: AlertRouter;

  /** In-memory alert store keyed by alertKey. */
  private readonly alerts: Map<string, Alert> = new Map();

  /** Deduplication window tracking: alertKey → last fired timestamp. */
  private readonly dedupeWindows: Map<string, DedupeEntry> = new Map();

  /** Deduplication window duration in milliseconds. */
  private readonly dedupeWindowMs: number;

  /** Transports keyed by channel name. */
  private readonly transports: Map<string, AlertTransport> = new Map();

  private constructor(dedupeWindowMs: number = 15 * 60 * 1000) {
    this.dedupeWindowMs = dedupeWindowMs;
    this.initializeTransports();
  }

  private initializeTransports(): void {
    // Initialize email transport if recipients are configured
    if (alertConfig.emailRecipients.length > 0) {
      this.transports.set(
        "email",
        new EmailTransport(alertConfig.emailRecipients)
      );
    }

    // Initialize Slack transport if webhook URL is configured
    if (alertConfig.slackWebhookUrl) {
      this.transports.set("slack", new SlackTransport(alertConfig.slackWebhookUrl));
    }

    // Initialize PagerDuty transport if integration key is configured
    if (alertConfig.pagerdutyIntegrationKey) {
      this.transports.set(
        "pagerduty",
        new PagerDutyTransport(alertConfig.pagerdutyIntegrationKey)
      );
    }
  }

  // --------------------------------------------------------------------------
  // Singleton access
  // --------------------------------------------------------------------------

  public static getInstance(
    dedupeWindowMs?: number
  ): AlertRouter {
    if (!AlertRouter.instance) {
      AlertRouter.instance = new AlertRouter(
        dedupeWindowMs ?? alertConfig.dedupeWindowMs
      );
    }
    return AlertRouter.instance;
  }

  public static resetInstance(): void {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (AlertRouter as any).instance = undefined;
  }

  // --------------------------------------------------------------------------
  // Core routing with deduplication
  // --------------------------------------------------------------------------

  /**
   * Routes an alert based on severity → channels config, applying deduplication.
   *
   * Returns `true` if the alert was dispatched, `false` if suppressed by dedupe window.
   * Individual transport failures do not block other transports.
   */
  public async routeAlert(
    alertKey: string,
    severity: Severity,
    message: string
  ): Promise<boolean> {
    const now = Date.now();

    // Check deduplication window
    const dedupeEntry = this.dedupeWindows.get(alertKey);
    if (dedupeEntry && now - dedupeEntry.lastFiredAt < this.dedupeWindowMs) {
      // Alert is within deduplication window; suppress it
      return false;
    }

    // Create alert object
    const alert: Alert = {
      alertKey,
      severity,
      message,
      status: AlertStatus.Open,
      createdAt: now,
    };

    // Store alert and update dedupe window
    this.alerts.set(alertKey, alert);
    this.dedupeWindows.set(alertKey, { lastFiredAt: now });

    // Get channels for this severity from config
    const channels = this.getChannelsForSeverity(severity);

    // Dispatch to all configured channels, but don't fail if one fails
    const results = await Promise.allSettled(
      channels.map((channel) => this.sendToChannel(channel, alert))
    );

    // Log any failures but don't throw
    results.forEach((result) => {
      if (result.status === "rejected") {
        console.error("Alert dispatch failed:", result.reason);
      }
    });

    return true;
  }

  /**
   * Marks an alert as acknowledged.
   */
  public acknowledgeAlert(alertKey: string): void {
    const alert = this.alerts.get(alertKey);
    if (!alert) {
      throw new Error(`Alert not found: ${alertKey}`);
    }
    if (alert.status === AlertStatus.Acknowledged) {
      throw new Error(`Alert already acknowledged: ${alertKey}`);
    }
    alert.status = AlertStatus.Acknowledged;
    alert.acknowledgedAt = Date.now();
    this.dedupeWindows.delete(alertKey);
  }

  // --------------------------------------------------------------------------
  // Helpers
  // --------------------------------------------------------------------------

  private getChannelsForSeverity(severity: Severity): string[] {
    const route = alertConfig.routes.find((r) => r.severity === severity);
    return route?.channels || [];
  }

  private async sendToChannel(
    channel: string,
    alert: Alert
  ): Promise<void> {
    const transport = this.transports.get(channel);
    if (!transport) {
      throw new Error(`Transport not configured for channel: ${channel}`);
    }
    await transport.send(alert);
  }

  // --------------------------------------------------------------------------
  // Query helpers
  // --------------------------------------------------------------------------

  public getAlert(alertKey: string): Alert | undefined {
    return this.alerts.get(alertKey);
  }

  public hasOpenAlert(alertKey: string): boolean {
    const alert = this.alerts.get(alertKey);
    return alert !== undefined && alert.status === AlertStatus.Open;
  }

  public getAllAlerts(): Alert[] {
    return [...this.alerts.values()];
  }

  public clearAlerts(): void {
    this.alerts.clear();
    this.dedupeWindows.clear();
  }

  /**
   * Clears expired dedupe entries. Call periodically (e.g., every minute).
   */
  public clearExpiredDedupeEntries(): void {
    const now = Date.now();
    const expired: string[] = [];

    this.dedupeWindows.forEach((entry, key) => {
      if (now - entry.lastFiredAt > this.dedupeWindowMs) {
        expired.push(key);
      }
    });

    expired.forEach((key) => this.dedupeWindows.delete(key));
  }
}

export const alertRouter = AlertRouter.getInstance();

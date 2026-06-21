/**
 * alertRouter.ts
 *
 * Routes severity-classified alerts to the appropriate notification channels.
 *
 * Design decisions:
 *  - Deduplication: only one open (unacknowledged) alert per `alertKey` is
 *    stored.  Consecutive calls with the same key are silently ignored so the
 *    worker never spams downstream channels.
 *  - Channels: HIGH alerts are dispatched to the critical channel; MEDIUM and
 *    LOW go to the standard channel.  Both channels are pluggable via the
 *    `NotificationChannel` interface so real webhook/email/PagerDuty adapters
 *    can be injected without touching this module.
 *  - Acknowledgement: `acknowledgeAlert(alertKey)` marks an alert resolved.
 *    The backfillService checks this status before permitting a resume.
 */

import { Alert, AlertStatus, Severity } from "../types/reconciliation";

// ---------------------------------------------------------------------------
// Notification channel interface (pluggable adapter pattern)
// ---------------------------------------------------------------------------

export interface NotificationChannel {
  send(alert: Alert): Promise<void>;
}

/** No-op channel used as default in production until real adapters are wired. */
export class NoOpChannel implements NotificationChannel {
  async send(_alert: Alert): Promise<void> {
    // intentionally empty – replace with real adapter
  }
}

// ---------------------------------------------------------------------------
// AlertRouter (singleton)
// ---------------------------------------------------------------------------

export class AlertRouter {
  private static instance: AlertRouter;

  /** In-memory alert store keyed by alertKey.  Replace with DB in production. */
  private readonly alerts: Map<string, Alert> = new Map();

  private criticalChannel: NotificationChannel;
  private standardChannel: NotificationChannel;

  private constructor(
    criticalChannel: NotificationChannel = new NoOpChannel(),
    standardChannel: NotificationChannel = new NoOpChannel()
  ) {
    this.criticalChannel = criticalChannel;
    this.standardChannel = standardChannel;
  }

  // --------------------------------------------------------------------------
  // Singleton access
  // --------------------------------------------------------------------------

  public static getInstance(
    criticalChannel?: NotificationChannel,
    standardChannel?: NotificationChannel
  ): AlertRouter {
    if (!AlertRouter.instance) {
      AlertRouter.instance = new AlertRouter(criticalChannel, standardChannel);
    }
    return AlertRouter.instance;
  }

  /**
   * Replaces the singleton instance.  Used only in tests to obtain a fresh
   * router without carrying state across test cases.
   */
  public static resetInstance(): void {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (AlertRouter as any).instance = undefined;
  }

  // --------------------------------------------------------------------------
  // Channel configuration
  // --------------------------------------------------------------------------

  /** Override the critical-alert channel (e.g. for testing). */
  public setCriticalChannel(channel: NotificationChannel): void {
    this.criticalChannel = channel;
  }

  /** Override the standard-alert channel (e.g. for testing). */
  public setStandardChannel(channel: NotificationChannel): void {
    this.standardChannel = channel;
  }

  // --------------------------------------------------------------------------
  // Core routing
  // --------------------------------------------------------------------------

  /**
   * Routes an alert, applying deduplication.
   *
   * If an open alert with the same `alertKey` already exists it is NOT re-sent
   * to the notification channel.  Returns `true` when the alert was dispatched,
   * `false` when it was suppressed by deduplication.
   */
  public async routeAlert(
    alertKey: string,
    severity: Severity,
    message: string
  ): Promise<boolean> {
    const existing = this.alerts.get(alertKey);

    // Deduplication: suppress if an open alert already exists for this key
    if (existing && existing.status === AlertStatus.Open) {
      return false;
    }

    const alert: Alert = {
      alertKey,
      severity,
      message,
      status: AlertStatus.Open,
      createdAt: Date.now(),
    };

    this.alerts.set(alertKey, alert);

    // Dispatch to the correct channel
    if (severity === Severity.HIGH) {
      await this.criticalChannel.send(alert);
    } else {
      await this.standardChannel.send(alert);
    }

    return true;
  }

  // --------------------------------------------------------------------------
  // Acknowledgement
  // --------------------------------------------------------------------------

  /**
   * Marks an alert as acknowledged.
   * Throws if the alert does not exist or is already acknowledged.
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
  }

  // --------------------------------------------------------------------------
  // Query helpers
  // --------------------------------------------------------------------------

  /** Returns the alert for the given key, or undefined if not found. */
  public getAlert(alertKey: string): Alert | undefined {
    return this.alerts.get(alertKey);
  }

  /** Returns true when an open (unacknowledged) alert exists for this key. */
  public hasOpenAlert(alertKey: string): boolean {
    const alert = this.alerts.get(alertKey);
    return alert !== undefined && alert.status === AlertStatus.Open;
  }

  /** Returns all stored alerts (snapshot). */
  public getAllAlerts(): Alert[] {
    return [...this.alerts.values()];
  }

  /**
   * Clears all alerts.  Use only in tests.
   */
  public clearAlerts(): void {
    this.alerts.clear();
  }
}

export const alertRouter = AlertRouter.getInstance();

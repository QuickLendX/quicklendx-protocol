import nodemailer from 'nodemailer';
import { ulid } from 'ulid';
import { getDatabase, getPreparedStatement } from '../lib/database';
import {
  NotificationEvent,
  NotificationType,
  UserNotificationPreferences,
  NotificationTemplate,
} from '../types/contract';

// Map NotificationType enum values to the notify_* column names
const PREF_COLUMN: Record<NotificationType, string> = {
  [NotificationType.InvoiceFunded]: 'notify_invoice_funded',
  [NotificationType.PaymentReceived]: 'notify_payment_received',
  [NotificationType.DisputeOpened]: 'notify_dispute_opened',
  [NotificationType.DisputeResolved]: 'notify_dispute_resolved',
};

export class NotificationService {
  private static instance: NotificationService;
  private transporter: nodemailer.Transporter;

  private constructor() {
    this.transporter = nodemailer.createTransport({
      host: process.env.SMTP_HOST || 'smtp.gmail.com',
      port: parseInt(process.env.SMTP_PORT || '587'),
      secure: false,
      auth: {
        user: process.env.SMTP_USER,
        pass: process.env.SMTP_PASS,
      },
    });
  }

  public static getInstance(): NotificationService {
    if (!NotificationService.instance) {
      NotificationService.instance = new NotificationService();
    }
    return NotificationService.instance;
  }

  // ---------------------------------------------------------------------------
  // Idempotency helpers (durable, survives restarts)
  // ---------------------------------------------------------------------------

  /**
   * Returns true if a notification row already exists for (event_id, user_id)
   * with status 'sent'. A 'failed' row is retryable.
   */
  private isNotificationSent(eventId: string, userId: string): boolean {
    const row = getPreparedStatement(
      "SELECT status FROM notifications WHERE event_id = ? AND user_id = ? LIMIT 1"
    ).get(eventId, userId) as { status: string } | undefined;
    return row?.status === 'sent';
  }

  /**
   * Insert a 'pending' row (idempotent via INSERT OR IGNORE).
   * Returns the row id that was inserted or already existed.
   */
  private insertPending(eventId: string, userId: string, type: NotificationType): string {
    const id = ulid();
    const now = new Date().toISOString();
    getPreparedStatement(`
      INSERT OR IGNORE INTO notifications
        (id, event_id, user_id, notification_type, status, created_at, updated_at)
      VALUES (?, ?, ?, ?, 'pending', ?, ?)
    `).run(id, eventId, userId, type, now, now);

    // Return the actual id (may differ if row already existed)
    const row = getPreparedStatement(
      "SELECT id FROM notifications WHERE event_id = ? AND user_id = ?"
    ).get(eventId, userId) as { id: string };
    return row.id;
  }

  private markSent(rowId: string): void {
    getPreparedStatement(
      "UPDATE notifications SET status = 'sent', smtp_error = NULL, updated_at = ? WHERE id = ?"
    ).run(new Date().toISOString(), rowId);
  }

  private markFailed(rowId: string, error: string): void {
    // Truncate error to avoid storing full stack traces / PII
    const safeError = error.slice(0, 500);
    getPreparedStatement(
      "UPDATE notifications SET status = 'failed', smtp_error = ?, updated_at = ? WHERE id = ?"
    ).run(safeError, new Date().toISOString(), rowId);
  }

  // ---------------------------------------------------------------------------
  // Preferences (persisted, replaces mock)
  // ---------------------------------------------------------------------------

  private getUserPreferences(userId: string): UserNotificationPreferences | null {
    const row = getPreparedStatement(
      "SELECT * FROM user_notification_preferences WHERE user_id = ?"
    ).get(userId) as Record<string, any> | undefined;

    if (!row) return null;

    return {
      email_enabled: row.email_enabled === 1,
      email_address: row.email_address ?? undefined,
      notifications: {
        [NotificationType.InvoiceFunded]: row.notify_invoice_funded === 1,
        [NotificationType.PaymentReceived]: row.notify_payment_received === 1,
        [NotificationType.DisputeOpened]: row.notify_dispute_opened === 1,
        [NotificationType.DisputeResolved]: row.notify_dispute_resolved === 1,
      },
    };
  }

  // ---------------------------------------------------------------------------
  // Email template (unchanged logic, kept private)
  // ---------------------------------------------------------------------------

  private getEmailTemplate(event: NotificationEvent): NotificationTemplate {
    const baseUrl = process.env.FRONTEND_URL || 'https://quicklendx.com';

    switch (event.type) {
      case NotificationType.InvoiceFunded:
        return {
          subject: 'Your Invoice Has Been Funded - QuickLendX',
          html: `<p>Invoice ${event.invoice_id} funded for ${event.amount} XLM.</p><a href="${baseUrl}/invoices/${event.invoice_id}">View Invoice</a>`,
          text: `Invoice ${event.invoice_id} funded for ${event.amount} XLM.\n${baseUrl}/invoices/${event.invoice_id}`,
        };
      case NotificationType.PaymentReceived:
        return {
          subject: 'Payment Received - QuickLendX',
          html: `<p>Payment of ${event.amount} XLM received for invoice ${event.invoice_id}.</p><a href="${baseUrl}/invoices/${event.invoice_id}">View Invoice</a>`,
          text: `Payment of ${event.amount} XLM received for invoice ${event.invoice_id}.\n${baseUrl}/invoices/${event.invoice_id}`,
        };
      case NotificationType.DisputeOpened:
        return {
          subject: 'Dispute Opened - QuickLendX',
          html: `<p>A dispute has been opened for invoice ${event.invoice_id}.</p><a href="${baseUrl}/invoices/${event.invoice_id}">View Dispute</a>`,
          text: `A dispute has been opened for invoice ${event.invoice_id}.\n${baseUrl}/invoices/${event.invoice_id}`,
        };
      case NotificationType.DisputeResolved:
        return {
          subject: 'Dispute Resolved - QuickLendX',
          html: `<p>The dispute for invoice ${event.invoice_id} has been resolved.</p><a href="${baseUrl}/invoices/${event.invoice_id}">View Resolution</a>`,
          text: `The dispute for invoice ${event.invoice_id} has been resolved.\n${baseUrl}/invoices/${event.invoice_id}`,
        };
      default:
        return {
          subject: 'QuickLendX Notification',
          html: '<p>You have a new notification.</p>',
          text: 'You have a new notification.',
        };
    }
  }

  private async sendEmail(to: string, template: NotificationTemplate): Promise<void> {
    await this.transporter.sendMail({
      from: process.env.FROM_EMAIL || 'noreply@quicklendx.com',
      to,
      subject: template.subject,
      text: template.text,
      html: template.html,
    });
  }

  // ---------------------------------------------------------------------------
  // Public API
  // ---------------------------------------------------------------------------

  /**
   * Process a notification event with durable idempotency.
   *
   * Idempotency key: (event.id, event.user_id)
   * - If a 'sent' row exists → skip (already delivered).
   * - If a 'failed' row exists → retry.
   * - If no row exists → insert 'pending', attempt send, update to 'sent'/'failed'.
   */
  public async processNotification(event: NotificationEvent): Promise<void> {
    // Fast-path: already delivered
    if (this.isNotificationSent(event.id, event.user_id)) {
      // debug-level only — no PII
      return;
    }

    const rowId = this.insertPending(event.id, event.user_id, event.type);

    const preferences = this.getUserPreferences(event.user_id);
    if (!preferences || !preferences.email_enabled || !preferences.email_address) {
      // No preferences row or email disabled — treat as opted-out, mark sent to avoid retry spam
      this.markSent(rowId);
      return;
    }

    if (!preferences.notifications[event.type]) {
      // Notification type disabled for this user
      this.markSent(rowId);
      return;
    }

    const template = this.getEmailTemplate(event);

    try {
      await this.sendEmail(preferences.email_address, template);
      this.markSent(rowId);
    } catch (error: any) {
      this.markFailed(rowId, error?.message ?? String(error));
      throw error;
    }
  }

  /**
   * Upsert user notification preferences.
   */
  public updateUserPreferences(
    userId: string,
    preferences: Partial<UserNotificationPreferences>
  ): void {
    const db = getDatabase();
    const now = new Date().toISOString();

    const existing = getPreparedStatement(
      "SELECT user_id FROM user_notification_preferences WHERE user_id = ?"
    ).get(userId);

    if (!existing) {
      getPreparedStatement(`
        INSERT INTO user_notification_preferences
          (user_id, email_enabled, email_address,
           notify_invoice_funded, notify_payment_received,
           notify_dispute_opened, notify_dispute_resolved, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      `).run(
        userId,
        preferences.email_enabled !== false ? 1 : 0,
        preferences.email_address ?? null,
        preferences.notifications?.[NotificationType.InvoiceFunded] !== false ? 1 : 0,
        preferences.notifications?.[NotificationType.PaymentReceived] !== false ? 1 : 0,
        preferences.notifications?.[NotificationType.DisputeOpened] !== false ? 1 : 0,
        preferences.notifications?.[NotificationType.DisputeResolved] !== false ? 1 : 0,
        now,
      );
    } else {
      const sets: string[] = ['updated_at = ?'];
      const params: unknown[] = [now];

      if (preferences.email_enabled !== undefined) {
        sets.push('email_enabled = ?');
        params.push(preferences.email_enabled ? 1 : 0);
      }
      if (preferences.email_address !== undefined) {
        sets.push('email_address = ?');
        params.push(preferences.email_address);
      }
      if (preferences.notifications) {
        const n = preferences.notifications;
        if (n[NotificationType.InvoiceFunded] !== undefined) {
          sets.push('notify_invoice_funded = ?');
          params.push(n[NotificationType.InvoiceFunded] ? 1 : 0);
        }
        if (n[NotificationType.PaymentReceived] !== undefined) {
          sets.push('notify_payment_received = ?');
          params.push(n[NotificationType.PaymentReceived] ? 1 : 0);
        }
        if (n[NotificationType.DisputeOpened] !== undefined) {
          sets.push('notify_dispute_opened = ?');
          params.push(n[NotificationType.DisputeOpened] ? 1 : 0);
        }
        if (n[NotificationType.DisputeResolved] !== undefined) {
          sets.push('notify_dispute_resolved = ?');
          params.push(n[NotificationType.DisputeResolved] ? 1 : 0);
        }
      }

      params.push(userId);
      db.prepare(
        `UPDATE user_notification_preferences SET ${sets.join(', ')} WHERE user_id = ?`
      ).run(...params);
    }
  }

  /**
   * Retrieve persisted preferences for a user (returns null if not found).
   */
  public getUserPreferencesPublic(userId: string): UserNotificationPreferences | null {
    return this.getUserPreferences(userId);
  }

  /**
   * Close the SMTP transport during graceful shutdown so in-flight sends
   * complete and no new ones start.
   */
  public closeTransport(): void {
    this.transporter.close();
  }
}

export const notificationService = NotificationService.getInstance();

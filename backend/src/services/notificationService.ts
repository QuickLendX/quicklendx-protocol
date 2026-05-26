import nodemailer from 'nodemailer';
import { NotificationEvent, NotificationType, UserNotificationPreferences, NotificationTemplate } from '../types/contract';

export class NotificationService {
  private static instance: NotificationService;
  private transporter: nodemailer.Transporter;
  private sentNotifications: Set<string> = new Set(); // For idempotency in memory (in prod, use DB)

  private constructor() {
    this.transporter = nodemailer.createTransport({
      host: process.env.SMTP_HOST || 'smtp.gmail.com',
      port: parseInt(process.env.SMTP_PORT || '587'),
      secure: false, // true for 465, false for other ports
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

  // Check if notification was already sent (idempotency)
  private isNotificationSent(eventId: string): boolean {
    return this.sentNotifications.has(eventId);
  }

  // Mark notification as sent
  private markNotificationSent(eventId: string): void {
    this.sentNotifications.add(eventId);
  }

  // Get user preferences (mock implementation - in prod, fetch from DB)
  private async getUserPreferences(userId: string): Promise<UserNotificationPreferences | null> {
    // Mock preferences - in real implementation, query database
    return {
      email_enabled: true,
      email_address: process.env.DEFAULT_EMAIL || 'user@example.com',
      notifications: {
        [NotificationType.InvoiceFunded]: true,
        [NotificationType.PaymentReceived]: true,
        [NotificationType.DisputeOpened]: true,
        [NotificationType.DisputeResolved]: true,
      },
    };
  }

  // Get email template for notification type
  private getEmailTemplate(event: NotificationEvent): NotificationTemplate {
    const baseUrl = process.env.FRONTEND_URL || 'https://quicklendx.com';

    switch (event.type) {
      case NotificationType.InvoiceFunded:
        return {
          subject: 'Your Invoice Has Been Funded - QuickLendX',
          html: `
            <div style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto;">
              <h2 style="color: #2563eb;">Invoice Funded Successfully</h2>
              <p>Great news! Your invoice has been funded and is ready for fulfillment.</p>
              <p><strong>Invoice ID:</strong> ${event.invoice_id}</p>
              <p><strong>Amount:</strong> ${event.amount} XLM</p>
              <p><strong>Funded At:</strong> ${new Date(event.timestamp).toLocaleString()}</p>
              <div style="margin: 30px 0;">
                <a href="${baseUrl}/invoices/${event.invoice_id}" style="background-color: #2563eb; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px;">View Invoice</a>
              </div>
              <p style="color: #6b7280; font-size: 14px;">You can unsubscribe from these notifications at any time.</p>
            </div>
          `,
          text: `
Invoice Funded Successfully

Great news! Your invoice has been funded and is ready for fulfillment.

Invoice ID: ${event.invoice_id}
Amount: ${event.amount} XLM
Funded At: ${new Date(event.timestamp).toLocaleString()}

View Invoice: ${baseUrl}/invoices/${event.invoice_id}

You can unsubscribe from these notifications at any time.
          `,
        };

      case NotificationType.PaymentReceived:
        return {
          subject: 'Payment Received - QuickLendX',
          html: `
            <div style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto;">
              <h2 style="color: #059669;">Payment Received</h2>
              <p>A payment has been received for your invoice.</p>
              <p><strong>Invoice ID:</strong> ${event.invoice_id}</p>
              <p><strong>Amount:</strong> ${event.amount} XLM</p>
              <p><strong>Received At:</strong> ${new Date(event.timestamp).toLocaleString()}</p>
              <div style="margin: 30px 0;">
                <a href="${baseUrl}/invoices/${event.invoice_id}" style="background-color: #059669; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px;">View Invoice</a>
              </div>
              <p style="color: #6b7280; font-size: 14px;">You can unsubscribe from these notifications at any time.</p>
            </div>
          `,
          text: `
Payment Received

A payment has been received for your invoice.

Invoice ID: ${event.invoice_id}
Amount: ${event.amount} XLM
Received At: ${new Date(event.timestamp).toLocaleString()}

View Invoice: ${baseUrl}/invoices/${event.invoice_id}

You can unsubscribe from these notifications at any time.
          `,
        };

      case NotificationType.DisputeOpened:
        return {
          subject: 'Dispute Opened - QuickLendX',
          html: `
            <div style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto;">
              <h2 style="color: #dc2626;">Dispute Opened</h2>
              <p>A dispute has been opened for your invoice.</p>
              <p><strong>Invoice ID:</strong> ${event.invoice_id}</p>
              <p><strong>Opened At:</strong> ${new Date(event.timestamp).toLocaleString()}</p>
              <div style="margin: 30px 0;">
                <a href="${baseUrl}/invoices/${event.invoice_id}" style="background-color: #dc2626; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px;">View Dispute</a>
              </div>
              <p style="color: #6b7280; font-size: 14px;">You can unsubscribe from these notifications at any time.</p>
            </div>
          `,
          text: `
Dispute Opened

A dispute has been opened for your invoice.

Invoice ID: ${event.invoice_id}
Opened At: ${new Date(event.timestamp).toLocaleString()}

View Dispute: ${baseUrl}/invoices/${event.invoice_id}

You can unsubscribe from these notifications at any time.
          `,
        };

      case NotificationType.DisputeResolved:
        return {
          subject: 'Dispute Resolved - QuickLendX',
          html: `
            <div style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto;">
              <h2 style="color: #059669;">Dispute Resolved</h2>
              <p>The dispute for your invoice has been resolved.</p>
              <p><strong>Invoice ID:</strong> ${event.invoice_id}</p>
              <p><strong>Resolved At:</strong> ${new Date(event.timestamp).toLocaleString()}</p>
              <div style="margin: 30px 0;">
                <a href="${baseUrl}/invoices/${event.invoice_id}" style="background-color: #059669; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px;">View Resolution</a>
              </div>
              <p style="color: #6b7280; font-size: 14px;">You can unsubscribe from these notifications at any time.</p>
            </div>
          `,
          text: `
Dispute Resolved

The dispute for your invoice has been resolved.

Invoice ID: ${event.invoice_id}
Resolved At: ${new Date(event.timestamp).toLocaleString()}

View Resolution: ${baseUrl}/invoices/${event.invoice_id}

You can unsubscribe from these notifications at any time.
          `,
        };

      default:
        return {
          subject: 'QuickLendX Notification',
          html: '<p>You have a new notification.</p>',
          text: 'You have a new notification.',
        };
    }
  }

  // Send email notification
  private async sendEmail(to: string, template: NotificationTemplate): Promise<void> {
    try {
      await this.transporter.sendMail({
        from: process.env.FROM_EMAIL || 'noreply@quicklendx.com',
        to,
        subject: template.subject,
        text: template.text,
        html: template.html,
      });
    } catch (error) {
      console.error('Failed to send email:', error);
      throw error;
    }
  }

  // Process a notification event
  public async processNotification(event: NotificationEvent): Promise<void> {
    // Check idempotency
    if (this.isNotificationSent(event.id)) {
      console.log(`Notification ${event.id} already sent, skipping`);
      return;
    }

    try {
      // Get user preferences
      const preferences = await this.getUserPreferences(event.user_id);
      if (!preferences || !preferences.email_enabled || !preferences.email_address) {
        console.log(`User ${event.user_id} has disabled email notifications or no email set`);
        return;
      }

      // Check if this notification type is enabled
      if (!preferences.notifications[event.type]) {
        console.log(`User ${event.user_id} has disabled ${event.type} notifications`);
        return;
      }

      // Get email template
      const template = this.getEmailTemplate(event);

      // Send email
      await this.sendEmail(preferences.email_address, template);

      // Mark as sent for idempotency
      this.markNotificationSent(event.id);

      console.log(`Notification ${event.id} sent successfully to ${preferences.email_address}`);
    } catch (error) {
      console.error(`Failed to process notification ${event.id}:`, error);
      throw error;
    }
  }

  // Update user preferences (mock implementation)
  public async updateUserPreferences(userId: string, preferences: Partial<UserNotificationPreferences>): Promise<void> {
    // In real implementation, save to database
    console.log(`Updated preferences for user ${userId}:`, preferences);
  }

  // Get user preferences
  public async getUserPreferencesPublic(userId: string): Promise<UserNotificationPreferences | null> {
    return this.getUserPreferences(userId);
  }
}

export const notificationService = NotificationService.getInstance();
import { Alert } from "../../types/reconciliation";
import { AlertTransport } from "./AlertTransport";
import nodemailer from "nodemailer";

export class EmailTransport implements AlertTransport {
  private readonly transporter: nodemailer.Transporter;
  private readonly recipients: string[];

  constructor(recipients: string[]) {
    this.recipients = recipients;
    this.transporter = nodemailer.createTransport({
      host: process.env.SMTP_HOST || "smtp.gmail.com",
      port: parseInt(process.env.SMTP_PORT || "587"),
      secure: false,
      auth: {
        user: process.env.SMTP_USER,
        pass: process.env.SMTP_PASS,
      },
    });
  }

  async send(alert: Alert): Promise<void> {
    const html = `
      <h2>[${alert.severity}] QuickLendX Alert</h2>
      <p><strong>Message:</strong> ${this.escapeHtml(alert.message)}</p>
      <p><strong>Alert Key:</strong> ${this.escapeHtml(alert.alertKey)}</p>
      <p><strong>Severity:</strong> ${alert.severity}</p>
      <p><strong>Timestamp:</strong> ${new Date(alert.createdAt).toISOString()}</p>
    `;

    try {
      await Promise.all(
        this.recipients.map((recipient) =>
          this.transporter.sendMail({
            to: recipient,
            subject: `[${alert.severity}] QuickLendX Alert: ${alert.message.substring(0, 50)}`,
            text: `[${alert.severity}] ${alert.message}\n\nAlert Key: ${alert.alertKey}\nTimestamp: ${new Date(alert.createdAt).toISOString()}`,
            html,
          })
        )
      );
    } catch (error) {
      console.error("Failed to send email alert:", error);
      throw error;
    }
  }

  private escapeHtml(text: string): string {
    const map: Record<string, string> = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#039;",
    };
    return text.replace(/[&<>"']/g, (m) => map[m]);
  }
}

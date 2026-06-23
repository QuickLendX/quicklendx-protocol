import { Alert } from "../../types/reconciliation";
import { NotificationService } from "../notificationService";

export class EmailTransport {
  private readonly notificationService: NotificationService;
  private readonly recipients: string[];

  constructor(recipients: string[]) {
    this.notificationService = NotificationService.getInstance();
    this.recipients = recipients;
  }

  public async send(alert: Alert): Promise<void> {
    const message = {
      id: `alert-${alert.alertKey}-${Date.now()}`,
      user_id: "system-alert",
      type: alert.severity === "HIGH" ? "InvoiceFunded" : "PaymentReceived",
      invoice_id: alert.alertKey,
      amount: "0",
    } as any;

    await Promise.all(
      this.recipients.map(async (recipient) => {
        await this.notificationService.sendEmail(recipient, {
          subject: `QuickLendX alert: ${alert.severity}`,
          text: `[${alert.severity}] ${alert.message}`,
          html: `<p>[${alert.severity}] ${alert.message}</p>`,
        } as any),
      }),
    );
  }
}

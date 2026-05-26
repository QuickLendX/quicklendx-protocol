import { notificationService } from './notificationService';
import { NotificationEvent, NotificationType } from '../types/contract';

export class EventProcessor {
  private static instance: EventProcessor;

  private constructor() {}

  public static getInstance(): EventProcessor {
    if (!EventProcessor.instance) {
      EventProcessor.instance = new EventProcessor();
    }
    return EventProcessor.instance;
  }

  // Process invoice settled event (funded)
  public async processInvoiceSettled(
    eventId: string,
    invoiceId: string,
    business: string,
    investor: string,
    amount: string,
    timestamp: number
  ): Promise<void> {
    // Notify business that invoice is funded
    const businessEvent: NotificationEvent = {
      id: `${eventId}_business`,
      type: NotificationType.InvoiceFunded,
      user_id: business,
      invoice_id: invoiceId,
      amount,
      timestamp,
    };

    await notificationService.processNotification(businessEvent);

    // Could also notify investor, but for now focusing on business notifications
  }

  // Process payment recorded event
  public async processPaymentRecorded(
    eventId: string,
    invoiceId: string,
    payer: string,
    amount: string,
    timestamp: number
  ): Promise<void> {
    // Notify business that payment was received
    const businessEvent: NotificationEvent = {
      id: `${eventId}_business`,
      type: NotificationType.PaymentReceived,
      user_id: payer, // Assuming payer is the business in this context
      invoice_id: invoiceId,
      amount,
      timestamp,
    };

    await notificationService.processNotification(businessEvent);
  }

  // Process dispute created event
  public async processDisputeCreated(
    eventId: string,
    invoiceId: string,
    initiator: string,
    timestamp: number
  ): Promise<void> {
    // Notify relevant parties about dispute
    const disputeEvent: NotificationEvent = {
      id: `${eventId}_dispute`,
      type: NotificationType.DisputeOpened,
      user_id: initiator,
      invoice_id: invoiceId,
      timestamp,
    };

    await notificationService.processNotification(disputeEvent);
  }

  // Process dispute resolved event
  public async processDisputeResolved(
    eventId: string,
    invoiceId: string,
    resolvedBy: string,
    timestamp: number
  ): Promise<void> {
    // Notify relevant parties about resolution
    const resolutionEvent: NotificationEvent = {
      id: `${eventId}_resolution`,
      type: NotificationType.DisputeResolved,
      user_id: resolvedBy,
      invoice_id: invoiceId,
      timestamp,
    };

    await notificationService.processNotification(resolutionEvent);
  }

  // Generic event processor that can be called from indexer
  public async processEvent(event: any): Promise<void> {
    const eventId = event.id || `${event.type}_${event.timestamp}`;

    switch (event.type) {
      case 'InvoiceSettled':
        await this.processInvoiceSettled(
          eventId,
          event.invoice_id,
          event.business,
          event.investor,
          event.amount || event.investor_return,
          event.timestamp
        );
        break;

      case 'PaymentRecorded':
        await this.processPaymentRecorded(
          eventId,
          event.invoice_id,
          event.payer,
          event.amount,
          event.timestamp
        );
        break;

      case 'DisputeCreated':
        await this.processDisputeCreated(
          eventId,
          event.invoice_id,
          event.initiator,
          event.timestamp
        );
        break;

      case 'DisputeResolved':
        await this.processDisputeResolved(
          eventId,
          event.invoice_id,
          event.resolved_by || event.admin,
          event.timestamp
        );
        break;

      default:
        console.log(`Unhandled event type: ${event.type}`);
    }
  }
}

export const eventProcessor = EventProcessor.getInstance();
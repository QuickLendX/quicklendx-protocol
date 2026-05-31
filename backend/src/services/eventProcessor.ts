import { notificationService } from './notificationService';
import { NotificationEvent, NotificationType } from '../types/contract';
import { settlementOrchestrator } from './settlementOrchestrator';

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

    // Create a pending settlement to track the debt lifecycle
    settlementOrchestrator.createPending({
      invoice_id: invoiceId,
      amount,
      payer: business,
      recipient: investor,
      timestamp,
      event_id: eventId,
    });
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
      user_id: payer,
      invoice_id: invoiceId,
      amount,
      timestamp,
    };

    await notificationService.processNotification(businessEvent);

    // Advance the settlement lifecycle: Pending -> Processing -> Paid
    settlementOrchestrator.startProcessing(invoiceId, `${eventId}_processing`);
    settlementOrchestrator.completeProcessing(invoiceId, `${eventId}_complete`);
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

    // Accept both legacy (flat) and new (payload-wrapped) event shapes
    const get = (field: string) =>
      event.payload && event.payload[field] !== undefined
        ? event.payload[field]
        : event[field];

    switch (event.type) {
      case 'InvoiceSettled':
        await this.processInvoiceSettled(
          eventId,
          get('invoice_id'),
          get('business'),
          get('investor'),
          get('amount') || get('investor_return'),
          event.timestamp
        );
        break;

      case 'PaymentRecorded':
        await this.processPaymentRecorded(
          eventId,
          get('invoice_id'),
          get('payer'),
          get('amount'),
          event.timestamp
        );
        break;

      case 'DisputeCreated':
        await this.processDisputeCreated(
          eventId,
          get('invoice_id'),
          get('initiator'),
          event.timestamp
        );
        break;

      case 'DisputeResolved':
        await this.processDisputeResolved(
          eventId,
          get('invoice_id'),
          get('resolved_by') || get('admin'),
          event.timestamp
        );
        break;

      default:
        // Unknown event types are treated as no-ops for backwards compatibility
        // with indexers that may post events we don't actively process.
        console.warn(`Ignoring unknown event type: ${event.type}`);
        return;
    }
  }
}

export const eventProcessor = EventProcessor.getInstance();

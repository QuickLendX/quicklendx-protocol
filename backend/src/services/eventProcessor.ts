import { notificationService } from './notificationService';
import { NotificationEvent, NotificationType } from '../types/contract';
import { getCorrelationId, withCorrelationId } from '../lib/requestContext';

export class EventProcessor {
  private static instance: EventProcessor;
  private processedEvents: Set<string>;

  private eventLog: Array<{ seq: number; id: string; type: string; timestamp: number; correlationId?: string }> = [];
  private sequence: number = 0;

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
    const correlationId = getCorrelationId();
    const correlationPrefix = correlationId ? `[${correlationId}] ` : "";
    const processedKey = `InvoiceSettled:${eventId}`;

    if (this.processedEvents.has(processedKey)) {
      console.log(`${correlationPrefix}EventProcessor: Skipping already processed InvoiceSettled event ${eventId}`);
      return;
    }

    this.processedEvents.add(processedKey);

    try {
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
      console.log(`${correlationPrefix}EventProcessor: Processed InvoiceSettled event ${eventId}`);
    } catch (error) {
      this.processedEvents.delete(processedKey);
      throw error;
    }
  }

  // Process payment recorded event
  public async processPaymentRecorded(
    eventId: string,
    invoiceId: string,
    payer: string,
    amount: string,
    timestamp: number
  ): Promise<void> {
    const correlationId = getCorrelationId();
    const correlationPrefix = correlationId ? `[${correlationId}] ` : "";
    const processedKey = `PaymentRecorded:${eventId}`;

    if (this.processedEvents.has(processedKey)) {
      console.log(`${correlationPrefix}EventProcessor: Skipping already processed PaymentRecorded event ${eventId}`);
      return;
    }

    this.processedEvents.add(processedKey);

    try {
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

      console.log(`${correlationPrefix}EventProcessor: Processed PaymentRecorded event ${eventId}`);
    } catch (error) {
      this.processedEvents.delete(processedKey);
      throw error;
    }
  }

  // Process dispute created event
  public async processDisputeCreated(
    eventId: string,
    invoiceId: string,
    initiator: string,
    timestamp: number
  ): Promise<void> {
    const correlationId = getCorrelationId();
    const correlationPrefix = correlationId ? `[${correlationId}] ` : "";
    const processedKey = `DisputeCreated:${eventId}`;

    if (this.processedEvents.has(processedKey)) {
      console.log(`${correlationPrefix}EventProcessor: Skipping already processed DisputeCreated event ${eventId}`);
      return;
    }

    this.processedEvents.add(processedKey);

    try {
      // Notify relevant parties about dispute
      const disputeEvent: NotificationEvent = {
        id: `${eventId}_dispute`,
        type: NotificationType.DisputeOpened,
        user_id: initiator,
        invoice_id: invoiceId,
        timestamp,
      };

      await notificationService.processNotification(disputeEvent);

      console.log(`${correlationPrefix}EventProcessor: Processed DisputeCreated event ${eventId}`);
    } catch (error) {
      this.processedEvents.delete(processedKey);
      throw error;
    }
  }

  // Process dispute resolved event
  public async processDisputeResolved(
    eventId: string,
    invoiceId: string,
    resolvedBy: string,
    timestamp: number
  ): Promise<void> {
    const correlationId = getCorrelationId();
    const correlationPrefix = correlationId ? `[${correlationId}] ` : "";
    const processedKey = `DisputeResolved:${eventId}`;

    if (this.processedEvents.has(processedKey)) {
      console.log(`${correlationPrefix}EventProcessor: Skipping already processed DisputeResolved event ${eventId}`);
      return;
    }

    this.processedEvents.add(processedKey);

    try {
      // Notify relevant parties about resolution
      const resolutionEvent: NotificationEvent = {
        id: `${eventId}_resolution`,
        type: NotificationType.DisputeResolved,
        user_id: resolvedBy,
        invoice_id: invoiceId,
        timestamp,
      };

      await notificationService.processNotification(resolutionEvent);

      console.log(`${correlationPrefix}EventProcessor: Processed DisputeResolved event ${eventId}`);
    } catch (error) {
      this.processedEvents.delete(processedKey);
      throw error;
    }
  }

  // Generic event processor that can be called from indexer
  public async processEvent(event: any): Promise<void> {
    const seq = this.sequence++;
    const eventId = event.id || `${event.type}_${event.timestamp}_${seq}`;
    const correlationId = getCorrelationId();

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
        throw new Error(`Unknown event type: ${event.type}`);
    }

    this.eventLog.push({ seq, id: eventId, type: event.type, timestamp: event.timestamp, correlationId: correlationId || undefined });
  }

  public async listEvents(cursor?: string, limit: number = 50): Promise<{ events: Array<{ seq: number; id: string; type: string; timestamp: number; correlationId?: string }>; nextCursor: string | null }> {
    const maxLimit = 100;
    const pageLimit = Math.min(Math.max(limit, 1), maxLimit);

    let afterSeq = -1;
    if (cursor) {
      const decoded = Buffer.from(cursor, 'base64').toString('utf8');
      const parsed = parseInt(decoded, 10);
      if (isNaN(parsed) || parsed < 0) {
        throw new Error('Invalid cursor');
      }
      afterSeq = parsed;
    }

    const filtered = this.eventLog.filter(event => event.seq > afterSeq).sort((a, b) => a.seq - b.seq);
    const events = filtered.slice(0, pageLimit);
    const lastEvent = events[events.length - 1];
    const nextCursor = lastEvent && filtered.length > pageLimit ? Buffer.from(lastEvent.seq.toString()).toString('base64') : null;

    return { events, nextCursor };
  }
}

export const eventProcessor = EventProcessor.getInstance();
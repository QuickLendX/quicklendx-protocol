import { notificationService } from './notificationService';
import { NotificationEvent, NotificationType } from '../types/contract';
import { getCorrelationId, withCorrelationId } from '../lib/requestContext';

export class EventProcessor {
  private static instance: EventProcessor;
  private processedEvents = new Set<string>();

  private eventLog: Array<{ seq: number; id: string; type: string; timestamp: number; correlationId?: string }> = [];
  private sequence: number = 0;
  private maxLogEntries: number = 10000;

  public constructor(maxLogEntries: number = 10000) {
    this.maxLogEntries = maxLogEntries;
  }

  public static getInstance(): EventProcessor {
    if (!EventProcessor.instance) {
      EventProcessor.instance = new EventProcessor();
    }
    return EventProcessor.instance;
  }

  public setMaxLogEntries(max: number): void {
    this.maxLogEntries = Math.max(1, max);
    if (this.eventLog.length > this.maxLogEntries) {
      this.eventLog.splice(0, this.eventLog.length - this.maxLogEntries);
    }
  }

  public getMaxLogEntries(): number {
    return this.maxLogEntries;
  }

  public getEventLogLength(): number {
    return this.eventLog.length;
  }

  public reset(): void {
    this.processedEvents.clear();
    this.eventLog = [];
    this.sequence = 0;
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

    const payload = event.payload || {};
    const invoiceId = payload.invoice_id || event.invoice_id;
    const business = payload.business || event.business;
    const investor = payload.investor || event.investor;
    const amount = payload.amount || event.amount || payload.investor_return || event.investor_return;
    const payer = payload.payer || event.payer;
    const initiator = payload.initiator || event.initiator;
    const resolvedBy = payload.resolved_by || event.resolved_by || payload.admin || event.admin;

    switch (event.type) {
      case 'InvoiceSettled':
        await this.processInvoiceSettled(
          eventId,
          invoiceId,
          business,
          investor,
          amount,
          event.timestamp
        );
        break;

      case 'PaymentRecorded':
        await this.processPaymentRecorded(
          eventId,
          invoiceId,
          payer,
          amount,
          event.timestamp
        );
        break;

      case 'DisputeCreated':
        await this.processDisputeCreated(
          eventId,
          invoiceId,
          initiator,
          event.timestamp
        );
        break;

      case 'DisputeResolved':
        await this.processDisputeResolved(
          eventId,
          invoiceId,
          resolvedBy,
          event.timestamp
        );
        break;

      case 'InvoiceCreated':
      case 'BidPlaced':
        // Legacy/indexer events accepted by the HTTP ingestion endpoint
        break;

      default: {
        const correlationPrefix = correlationId ? `[${correlationId}] ` : "";
        // Unknown event types must fail loudly instead of being silently
        // dropped, so callers can surface the error without losing the event.
        throw new Error(`${correlationPrefix}Unknown event type: ${event.type}`);
      }
    }

    this.eventLog.push({ seq, id: eventId, type: event.type, timestamp: event.timestamp, correlationId: correlationId || undefined });
    if (this.eventLog.length > this.maxLogEntries) {
      this.eventLog.splice(0, this.eventLog.length - this.maxLogEntries);
    }
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

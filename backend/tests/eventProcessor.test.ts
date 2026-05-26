import { describe, expect, it, jest, beforeEach } from "@jest/globals";
import { EventProcessor } from "../src/services/eventProcessor";
import { NotificationType } from "../src/types/contract";

// Mock the notification service
jest.mock('../src/services/notificationService', () => ({
  notificationService: {
    processNotification: jest.fn(),
  },
}));

import { notificationService } from '../src/services/notificationService';

describe("EventProcessor", () => {
  let eventProcessor: EventProcessor;
  let mockNotificationService: any;

  beforeEach(() => {
    // Clear singleton instance
    (EventProcessor as any).instance = null;
    eventProcessor = EventProcessor.getInstance();
    mockNotificationService = notificationService;
    jest.clearAllMocks();
  });

  describe("processInvoiceSettled", () => {
    it("should process invoice settled event correctly", async () => {
      await eventProcessor.processInvoiceSettled(
        "event123",
        "inv123",
        "business456",
        "investor789",
        "1000",
        1234567890
      );

      expect(mockNotificationService.processNotification).toHaveBeenCalledWith({
        id: "event123_business",
        type: NotificationType.InvoiceFunded,
        user_id: "business456",
        invoice_id: "inv123",
        amount: "1000",
        timestamp: 1234567890,
      });
    });
  });

  describe("processPaymentRecorded", () => {
    it("should process payment recorded event correctly", async () => {
      await eventProcessor.processPaymentRecorded(
        "event456",
        "inv123",
        "payer789",
        "500",
        1234567891
      );

      expect(mockNotificationService.processNotification).toHaveBeenCalledWith({
        id: "event456_business",
        type: NotificationType.PaymentReceived,
        user_id: "payer789",
        invoice_id: "inv123",
        amount: "500",
        timestamp: 1234567891,
      });
    });
  });

  describe("processDisputeCreated", () => {
    it("should process dispute created event correctly", async () => {
      await eventProcessor.processDisputeCreated(
        "event789",
        "inv123",
        "initiator456",
        1234567892
      );

      expect(mockNotificationService.processNotification).toHaveBeenCalledWith({
        id: "event789_dispute",
        type: NotificationType.DisputeOpened,
        user_id: "initiator456",
        invoice_id: "inv123",
        timestamp: 1234567892,
      });
    });
  });

  describe("processDisputeResolved", () => {
    it("should process dispute resolved event correctly", async () => {
      await eventProcessor.processDisputeResolved(
        "event101",
        "inv123",
        "resolver456",
        1234567893
      );

      expect(mockNotificationService.processNotification).toHaveBeenCalledWith({
        id: "event101_resolution",
        type: NotificationType.DisputeResolved,
        user_id: "resolver456",
        invoice_id: "inv123",
        timestamp: 1234567893,
      });
    });
  });

  describe("processEvent", () => {
    it("should handle InvoiceSettled event", async () => {
      const event = {
        type: "InvoiceSettled",
        id: "settled123",
        invoice_id: "inv123",
        business: "business456",
        investor: "investor789",
        amount: "1000",
        timestamp: 1234567890,
      };

      await eventProcessor.processEvent(event);

      expect(mockNotificationService.processNotification).toHaveBeenCalledWith({
        id: "settled123_business",
        type: NotificationType.InvoiceFunded,
        user_id: "business456",
        invoice_id: "inv123",
        amount: "1000",
        timestamp: 1234567890,
      });
    });

    it("should handle PaymentRecorded event", async () => {
      const event = {
        type: "PaymentRecorded",
        id: "payment123",
        invoice_id: "inv123",
        payer: "payer789",
        amount: "500",
        timestamp: 1234567891,
      };

      await eventProcessor.processEvent(event);

      expect(mockNotificationService.processNotification).toHaveBeenCalledWith({
        id: "payment123_business",
        type: NotificationType.PaymentReceived,
        user_id: "payer789",
        invoice_id: "inv123",
        amount: "500",
        timestamp: 1234567891,
      });
    });

    it("should handle DisputeCreated event", async () => {
      const event = {
        type: "DisputeCreated",
        id: "dispute123",
        invoice_id: "inv123",
        initiator: "initiator456",
        timestamp: 1234567892,
      };

      await eventProcessor.processEvent(event);

      expect(mockNotificationService.processNotification).toHaveBeenCalledWith({
        id: "dispute123_dispute",
        type: NotificationType.DisputeOpened,
        user_id: "initiator456",
        invoice_id: "inv123",
        timestamp: 1234567892,
      });
    });

    it("should handle DisputeResolved event", async () => {
      const event = {
        type: "DisputeResolved",
        id: "resolved123",
        invoice_id: "inv123",
        resolved_by: "resolver456",
        timestamp: 1234567893,
      };

      await eventProcessor.processEvent(event);

      expect(mockNotificationService.processNotification).toHaveBeenCalledWith({
        id: "resolved123_resolution",
        type: NotificationType.DisputeResolved,
        user_id: "resolver456",
        invoice_id: "inv123",
        timestamp: 1234567893,
      });
    });

    it("should ignore unknown event types", async () => {
      const event = {
        type: "UnknownEvent",
        id: "unknown123",
        timestamp: 1234567894,
      };

      await eventProcessor.processEvent(event);

      expect(mockNotificationService.processNotification).not.toHaveBeenCalled();
    });
  });
});
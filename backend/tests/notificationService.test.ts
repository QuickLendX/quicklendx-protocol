import { describe, expect, it, jest, beforeEach, afterEach } from "@jest/globals";
import { NotificationService } from "../src/services/notificationService";
import { NotificationEvent, NotificationType } from "../src/types/contract";

// Mock nodemailer
jest.mock('nodemailer', () => ({
  createTransporter: jest.fn(() => ({
    sendMail: jest.fn(),
  })),
}));

describe("NotificationService", () => {
  let notificationService: NotificationService;
  let mockTransporter: any;

  beforeEach(() => {
    // Clear singleton instance
    (NotificationService as any).instance = null;
    notificationService = NotificationService.getInstance();

    // Get the mocked transporter
    mockTransporter = (notificationService as any).transporter;
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe("Idempotency", () => {
    it("should not send duplicate notifications", async () => {
      const event: NotificationEvent = {
        id: "test-event-1",
        type: NotificationType.InvoiceFunded,
        user_id: "user123",
        invoice_id: "inv123",
        amount: "100",
        timestamp: Date.now(),
      };

      mockTransporter.sendMail.mockResolvedValue({});

      // First send should succeed
      await notificationService.processNotification(event);
      expect(mockTransporter.sendMail).toHaveBeenCalledTimes(1);

      // Second send should be skipped
      await notificationService.processNotification(event);
      expect(mockTransporter.sendMail).toHaveBeenCalledTimes(1);
    });
  });

  describe("Template Rendering", () => {
    it("should render invoice funded template correctly", async () => {
      const event: NotificationEvent = {
        id: "test-event-2",
        type: NotificationType.InvoiceFunded,
        user_id: "user123",
        invoice_id: "inv123",
        amount: "100",
        timestamp: Date.now(),
      };

      mockTransporter.sendMail.mockResolvedValue({});

      await notificationService.processNotification(event);

      expect(mockTransporter.sendMail).toHaveBeenCalledWith(
        expect.objectContaining({
          subject: "Your Invoice Has Been Funded - QuickLendX",
          html: expect.stringContaining("Invoice Funded Successfully"),
          text: expect.stringContaining("Invoice Funded Successfully"),
        })
      );
    });

    it("should render payment received template correctly", async () => {
      const event: NotificationEvent = {
        id: "test-event-3",
        type: NotificationType.PaymentReceived,
        user_id: "user123",
        invoice_id: "inv123",
        amount: "50",
        timestamp: Date.now(),
      };

      mockTransporter.sendMail.mockResolvedValue({});

      await notificationService.processNotification(event);

      expect(mockTransporter.sendMail).toHaveBeenCalledWith(
        expect.objectContaining({
          subject: "Payment Received - QuickLendX",
          html: expect.stringContaining("Payment Received"),
          text: expect.stringContaining("Payment Received"),
        })
      );
    });

    it("should render dispute opened template correctly", async () => {
      const event: NotificationEvent = {
        id: "test-event-4",
        type: NotificationType.DisputeOpened,
        user_id: "user123",
        invoice_id: "inv123",
        timestamp: Date.now(),
      };

      mockTransporter.sendMail.mockResolvedValue({});

      await notificationService.processNotification(event);

      expect(mockTransporter.sendMail).toHaveBeenCalledWith(
        expect.objectContaining({
          subject: "Dispute Opened - QuickLendX",
          html: expect.stringContaining("Dispute Opened"),
          text: expect.stringContaining("Dispute Opened"),
        })
      );
    });

    it("should render dispute resolved template correctly", async () => {
      const event: NotificationEvent = {
        id: "test-event-5",
        type: NotificationType.DisputeResolved,
        user_id: "user123",
        invoice_id: "inv123",
        timestamp: Date.now(),
      };

      mockTransporter.sendMail.mockResolvedValue({});

      await notificationService.processNotification(event);

      expect(mockTransporter.sendMail).toHaveBeenCalledWith(
        expect.objectContaining({
          subject: "Dispute Resolved - QuickLendX",
          html: expect.stringContaining("Dispute Resolved"),
          text: expect.stringContaining("Dispute Resolved"),
        })
      );
    });
  });

  describe("User Preferences", () => {
    it("should skip notification if email is disabled", async () => {
      // Mock getUserPreferences to return disabled email
      const originalGetPrefs = (notificationService as any).getUserPreferences;
      (notificationService as any).getUserPreferences = jest.fn().mockResolvedValue({
        email_enabled: false,
        email_address: "user@example.com",
        notifications: {
          [NotificationType.InvoiceFunded]: true,
          [NotificationType.PaymentReceived]: true,
          [NotificationType.DisputeOpened]: true,
          [NotificationType.DisputeResolved]: true,
        },
      });

      const event: NotificationEvent = {
        id: "test-event-6",
        type: NotificationType.InvoiceFunded,
        user_id: "user123",
        timestamp: Date.now(),
      };

      await notificationService.processNotification(event);

      expect(mockTransporter.sendMail).not.toHaveBeenCalled();

      // Restore original method
      (notificationService as any).getUserPreferences = originalGetPrefs;
    });

    it("should skip notification if specific type is disabled", async () => {
      // Mock getUserPreferences to return disabled for invoice funded
      const originalGetPrefs = (notificationService as any).getUserPreferences;
      (notificationService as any).getUserPreferences = jest.fn().mockResolvedValue({
        email_enabled: true,
        email_address: "user@example.com",
        notifications: {
          [NotificationType.InvoiceFunded]: false,
          [NotificationType.PaymentReceived]: true,
          [NotificationType.DisputeOpened]: true,
          [NotificationType.DisputeResolved]: true,
        },
      });

      const event: NotificationEvent = {
        id: "test-event-7",
        type: NotificationType.InvoiceFunded,
        user_id: "user123",
        timestamp: Date.now(),
      };

      await notificationService.processNotification(event);

      expect(mockTransporter.sendMail).not.toHaveBeenCalled();

      // Restore original method
      (notificationService as any).getUserPreferences = originalGetPrefs;
    });

    it("should skip notification if no email address", async () => {
      // Mock getUserPreferences to return no email
      const originalGetPrefs = (notificationService as any).getUserPreferences;
      (notificationService as any).getUserPreferences = jest.fn().mockResolvedValue({
        email_enabled: true,
        email_address: undefined,
        notifications: {
          [NotificationType.InvoiceFunded]: true,
          [NotificationType.PaymentReceived]: true,
          [NotificationType.DisputeOpened]: true,
          [NotificationType.DisputeResolved]: true,
        },
      });

      const event: NotificationEvent = {
        id: "test-event-8",
        type: NotificationType.InvoiceFunded,
        user_id: "user123",
        timestamp: Date.now(),
      };

      await notificationService.processNotification(event);

      expect(mockTransporter.sendMail).not.toHaveBeenCalled();

      // Restore original method
      (notificationService as any).getUserPreferences = originalGetPrefs;
    });
  });

  describe("Email Sending", () => {
    it("should handle email sending errors gracefully", async () => {
      const event: NotificationEvent = {
        id: "test-event-9",
        type: NotificationType.InvoiceFunded,
        user_id: "user123",
        timestamp: Date.now(),
      };

      mockTransporter.sendMail.mockRejectedValue(new Error("SMTP error"));

      await expect(notificationService.processNotification(event)).rejects.toThrow("SMTP error");
    });

    it("should send email with correct recipient", async () => {
      const event: NotificationEvent = {
        id: "test-event-10",
        type: NotificationType.InvoiceFunded,
        user_id: "user123",
        timestamp: Date.now(),
      };

      mockTransporter.sendMail.mockResolvedValue({});

      await notificationService.processNotification(event);

      expect(mockTransporter.sendMail).toHaveBeenCalledWith(
        expect.objectContaining({
          to: "user@example.com", // from mock
        })
      );
    });
  });
});
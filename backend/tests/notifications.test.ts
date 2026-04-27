import { describe, expect, it, jest, beforeEach } from "@jest/globals";
import request from "supertest";
import app from "../src/app";

// Mock the notification service
jest.mock('../src/services/notificationService', () => ({
  notificationService: {
    getUserPreferencesPublic: jest.fn(),
    updateUserPreferences: jest.fn(),
  },
}));

import { notificationService } from '../src/services/notificationService';

describe("Notification Routes", () => {
  let mockNotificationService: any;

  beforeEach(() => {
    mockNotificationService = notificationService;
    jest.clearAllMocks();
  });

  describe("GET /api/v1/notifications/preferences/:userId", () => {
    it("should return user preferences", async () => {
      const mockPreferences = {
        email_enabled: true,
        email_address: "user@example.com",
        notifications: {
          invoice_funded: true,
          payment_received: true,
          dispute_opened: true,
          dispute_resolved: true,
        },
      };

      mockNotificationService.getUserPreferencesPublic.mockResolvedValue(mockPreferences);

      const res = await request(app)
        .get("/api/v1/notifications/preferences/user123")
        .expect(200);

      expect(res.body).toEqual(mockPreferences);
      expect(mockNotificationService.getUserPreferencesPublic).toHaveBeenCalledWith("user123");
    });

    it("should return 404 if preferences not found", async () => {
      mockNotificationService.getUserPreferencesPublic.mockResolvedValue(null);

      const res = await request(app)
        .get("/api/v1/notifications/preferences/user123")
        .expect(404);

      expect(res.body).toEqual({ error: "User preferences not found" });
    });
  });

  describe("PUT /api/v1/notifications/preferences/:userId", () => {
    it("should update user preferences", async () => {
      const updates = {
        email_enabled: false,
        notifications: {
          invoice_funded: false,
        },
      };

      mockNotificationService.updateUserPreferences.mockResolvedValue(undefined);

      const res = await request(app)
        .put("/api/v1/notifications/preferences/user123")
        .send(updates)
        .expect(200);

      expect(res.body).toEqual({ success: true });
      expect(mockNotificationService.updateUserPreferences).toHaveBeenCalledWith("user123", updates);
    });

    it("should validate request data", async () => {
      const invalidUpdates = {
        email_address: "invalid-email",
      };

      const res = await request(app)
        .put("/api/v1/notifications/preferences/user123")
        .send(invalidUpdates)
        .expect(400);

      expect(res.body.error).toBe("Invalid request data");
    });
  });

  describe("POST /api/v1/notifications/unsubscribe/:userId", () => {
    it("should unsubscribe user from emails", async () => {
      mockNotificationService.updateUserPreferences.mockResolvedValue(undefined);

      const res = await request(app)
        .post("/api/v1/notifications/unsubscribe/user123")
        .expect(200);

      expect(res.body).toEqual({
        success: true,
        message: "Successfully unsubscribed from email notifications"
      });
      expect(mockNotificationService.updateUserPreferences).toHaveBeenCalledWith("user123", {
        email_enabled: false,
      });
    });
  });
});
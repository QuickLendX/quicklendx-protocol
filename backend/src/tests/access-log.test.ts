/**
 * Access Logging Middleware Tests
 * 
 * Comprehensive tests for:
 * - Access logging functionality
 * - Middleware integration
 * - Log filtering and statistics
 */

import { Request, Response, NextFunction } from "express";
import {
  logAccess,
  getAccessLogs,
  getRedactedAccessLogs,
  clearAccessLogs,
  getAccessLogStats,
  accessLogMiddleware,
  kycAccessLogMiddleware,
  AccessLogEntry
} from "../middleware/access-log";

describe("Access Logging Middleware", () => {
  beforeEach(() => {
    clearAccessLogs();
  });

  describe("Basic Logging", () => {
    it("should log access events", () => {
      logAccess({
        action: "read",
        resource: "kyc",
        userId: "user_123",
        ipAddress: "192.168.1.1",
        fields: ["tax_id", "customer_name"],
        sensitiveFields: ["tax_id"],
        piiFields: ["customer_name"],
        status: "success"
      });

      const logs = getAccessLogs();
      expect(logs.length).toBe(1);
      expect(logs[0].action).toBe("read");
      expect(logs[0].resource).toBe("kyc");
    });

    it("should log with resource ID", () => {
      logAccess({
        action: "read",
        resource: "invoice",
        resourceId: "inv_456",
        userId: "user_123",
        fields: [],
        sensitiveFields: [],
        piiFields: [],
        status: "success"
      });

      const logs = getAccessLogs();
      expect(logs[0].resourceId).toBe("inv_456");
    });

    it("should log failed access", () => {
      logAccess({
        action: "read",
        resource: "kyc",
        userId: "user_123",
        fields: [],
        sensitiveFields: [],
        piiFields: [],
        status: "failure",
        error: "HTTP 403"
      });

      const logs = getAccessLogs();
      expect(logs[0].status).toBe("failure");
      expect(logs[0].error).toBe("HTTP 403");
    });

    it("should include timestamp in log entry", () => {
      logAccess({
        action: "write",
        resource: "kyc",
        fields: [],
        sensitiveFields: [],
        piiFields: [],
        status: "success"
      });

      const logs = getAccessLogs();
      expect(logs[0].timestamp).toBeDefined();
      expect(new Date(logs[0].timestamp).getTime()).toBeGreaterThan(0);
    });
  });

  describe("Log Filtering", () => {
    beforeEach(() => {
      // Add multiple log entries
      logAccess({ action: "read", resource: "kyc", userId: "user_1", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
      logAccess({ action: "write", resource: "kyc", userId: "user_1", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
      logAccess({ action: "read", resource: "invoice", userId: "user_2", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
      logAccess({ action: "update", resource: "kyc", userId: "user_1", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
      logAccess({ action: "delete", resource: "settlement", userId: "user_3", fields: [], sensitiveFields: [], piiFields: [], status: "failure" });
    });

    it("should filter by userId", () => {
      const logs = getAccessLogs({ userId: "user_1" });
      expect(logs.length).toBe(3);
      logs.forEach(log => expect(log.userId).toBe("user_1"));
    });

    it("should filter by resource", () => {
      const logs = getAccessLogs({ resource: "kyc" });
      expect(logs.length).toBe(3);
      logs.forEach(log => expect(log.resource).toBe("kyc"));
    });

    it("should filter by action", () => {
      const logs = getAccessLogs({ action: "read" });
      expect(logs.length).toBe(2);
      logs.forEach(log => expect(log.action).toBe("read"));
    });

    it("should filter by date range", () => {
      const startDate = new Date(Date.now() - 1000);
      const endDate = new Date(Date.now() + 1000);
      
      const logs = getAccessLogs({ startDate, endDate });
      expect(logs.length).toBe(5);
    });

    it("should combine multiple filters", () => {
      const logs = getAccessLogs({ userId: "user_1", resource: "kyc" });
      expect(logs.length).toBe(3);
    });
  });

  describe("Log Statistics", () => {
    it("should return correct total count", () => {
      logAccess({ action: "read", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
      logAccess({ action: "write", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
      logAccess({ action: "read", resource: "invoice", fields: [], sensitiveFields: [], piiFields: [], status: "success" });

      const stats = getAccessLogStats();
      expect(stats.total).toBe(3);
    });

    it("should count by action", () => {
      logAccess({ action: "read", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
      logAccess({ action: "read", resource: "invoice", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
      logAccess({ action: "write", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });

      const stats = getAccessLogStats();
      expect(stats.byAction.read).toBe(2);
      expect(stats.byAction.write).toBe(1);
    });

    it("should count by resource", () => {
      logAccess({ action: "read", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
      logAccess({ action: "read", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
      logAccess({ action: "read", resource: "invoice", fields: [], sensitiveFields: [], piiFields: [], status: "success" });

      const stats = getAccessLogStats();
      expect(stats.byResource.kyc).toBe(2);
      expect(stats.byResource.invoice).toBe(1);
    });

    it("should count by status", () => {
      logAccess({ action: "read", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
      logAccess({ action: "read", resource: "invoice", fields: [], sensitiveFields: [], piiFields: [], status: "failure" });

      const stats = getAccessLogStats();
      expect(stats.byStatus.success).toBe(1);
      expect(stats.byStatus.failure).toBe(1);
    });
  });

  describe("Redacted Logs", () => {
    it("should redact PII in logs", () => {
      logAccess({
        action: "read",
        resource: "kyc",
        userId: "user_123",
        ipAddress: "192.168.1.1",
        fields: ["tax_id", "customer_name"],
        sensitiveFields: ["tax_id"],
        piiFields: ["customer_name"],
        status: "success"
      });

      const redactedLogs = getRedactedAccessLogs();
      expect(redactedLogs.length).toBe(1);
      // IP address should be redacted since it's in PII fields
      // "192.168.1.1" -> first 2 = "19", last 2 = ".1" -> "19****.1"
      expect(redactedLogs[0].ipAddress).toBe("19****.1");
    });
  });

  describe("Middleware Integration", () => {
    it("should create access log middleware", () => {
      const middleware = accessLogMiddleware("kyc", "read");
      expect(typeof middleware).toBe("function");
    });

    it("should create KYC-specific middleware", () => {
      const middleware = kycAccessLogMiddleware("read");
      expect(typeof middleware).toBe("function");
    });
  });

  describe("Log Management", () => {
    it("should clear all logs", () => {
      logAccess({ action: "read", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
      logAccess({ action: "write", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });

      clearAccessLogs();

      const logs = getAccessLogs();
      expect(logs.length).toBe(0);
    });

    it("should handle empty filters", () => {
      logAccess({ action: "read", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });

      const logs = getAccessLogs({});
      expect(logs.length).toBe(1);
    });
  });

  describe("Security Validation", () => {
    it("should log all sensitive field access", () => {
      logAccess({
        action: "read",
        resource: "invoice",
        fields: ["tax_id", "customer_name", "amount"],
        sensitiveFields: ["tax_id"],
        piiFields: ["customer_name"],
        status: "success"
      });

      const logs = getAccessLogs();
      expect(logs[0].fields).toContain("tax_id");
      expect(logs[0].fields).toContain("customer_name");
      expect(logs[0].fields).toContain("amount");
      expect(logs[0].sensitiveFields).toContain("tax_id");
      expect(logs[0].piiFields).toContain("customer_name");
    });

    it("should track user identification for audit", () => {
      logAccess({
        action: "read",
        resource: "kyc",
        userId: "user_abc123",
        ipAddress: "10.0.0.1",
        fields: [],
        sensitiveFields: [],
        piiFields: [],
        status: "success"
      });

      const logs = getAccessLogs();
      expect(logs[0].userId).toBe("user_abc123");
      expect(logs[0].ipAddress).toBe("10.0.0.1");
    });
  });
});
/**
 * Access Logging Middleware for Sensitive Data
 * 
 * Logs every read access to sensitive data fields including KYC data.
 * This is critical for compliance and security auditing.
 * 
 * Security assumptions:
 * - Logs and backups are sensitive surfaces
 * - All access to sensitive data must be logged
 * - Logs should not contain PII (use hashing where needed)
 */

import { Request, Response, NextFunction } from "express";
import { redactPii, hashForLog, isPiiField, isSensitiveField } from "../services/kycService";
import { getCorrelationId, sanitizeCorrelationId } from "../lib/requestContext";

// Log entry interface
export interface AccessLogEntry {
  correlationId?: string;
  timestamp: string;
  action: "read" | "write" | "update" | "delete";
  resource: string;
  resourceId?: string;
  userId?: string;
  ipAddress?: string;
  userAgent?: string;
  fields: string[];
  sensitiveFields: string[];
  piiFields: string[];
  status: "success" | "failure";
  error?: string;
}

// In-memory access log storage (in production, use a proper logging service)
const accessLogs: AccessLogEntry[] = [];
const MAX_LOGS = 10000;
const installedLoggers = new WeakMap<Response, Set<string>>();

// Diagnostics are fixed codes: caught errors may contain KYC values or tokens.
// A failed secondary console sink must never replace an application error.
function reportFailure(code: string): void {
  try {
    console.error(`[${code}]`);
  } catch {
    // The in-memory audit record remains available when console output fails.
  }
}

/**
 * Log an access event to sensitive data
 */
export function logAccess(entry: Omit<AccessLogEntry, "timestamp" | "correlationId">): void {
  const correlationId = sanitizeCorrelationId(getCorrelationId());
  const logEntry: AccessLogEntry = {
    ...entry,
    correlationId: correlationId ?? undefined,
    timestamp: new Date().toISOString()
  };

  accessLogs.push(logEntry);

  // Trim old logs to prevent memory issues
  if (accessLogs.length > MAX_LOGS) {
    accessLogs.shift();
  }

  // In production, this would send to a logging service (e.g., Winston, ELK stack)
  const correlationPrefix = correlationId ? `[${correlationId}] ` : "";
  // Identity hints/IPs stay in the protected audit store, never in stdout.
  try {
    console.log(`${correlationPrefix}[ACCESS] ${logEntry.action.toUpperCase()} ${logEntry.resource} - ${logEntry.status}`);
  } catch {
    reportFailure("ACCESS_LOG_CONSOLE_FAILED");
  }
}

function stringHeader(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

/**
 * Get client IP address from request
 */
function getClientIp(req: Request): string {
  return stringHeader(req.headers["x-forwarded-for"]) ||
         stringHeader(req.headers["x-real-ip"]) ||
         req.ip || 
         "unknown";
}

/**
 * Extract user identifier from request
 * In production, this would come from authentication middleware
 */
function getUserId(req: Request): string | undefined {
  // Authenticated actors take precedence over caller-supplied audit hints.
  // Logging itself never grants a permission or authenticates the request.
  const actor = (req as Request & { actor?: unknown }).actor;
  if (typeof actor === "string" && actor.length > 0) return actor;
  // Check for authenticated user
  const userId = stringHeader(req.headers["x-user-id"]);
  if (userId) return userId;
  
  // Check for API key or other auth token
  const authorization = stringHeader(req.headers["authorization"]);
  if (authorization) return hashForLog(authorization);
  
  return undefined;
}

/**
 * Identify sensitive and PII fields from request body/query
 */
function identifySensitiveFields(data: unknown): {
  allSensitive: string[];
  piiFields: string[];
} {
  const allSensitive: string[] = [];
  const piiFields: string[] = [];

  // Inspect names only. Reading values/spreading records can execute getters
  // and needlessly expose plaintext to the audit pipeline.
  const keys = data !== null && typeof data === "object" ? Object.keys(data) : [];
  for (const key of keys) {
    if (isSensitiveField(key)) {
      allSensitive.push(key);
    }
    if (isPiiField(key)) {
      piiFields.push(key);
    }
  }

  return { allSensitive, piiFields };
}

/**
 * Middleware to log access to sensitive endpoints
 */
export function accessLogMiddleware(
  resource: string,
  action: "read" | "write" | "update" | "delete"
) {
  if (!["read", "write", "update", "delete"].includes(action)) {
    throw new TypeError("Invalid access log action");
  }
  return (req: Request, res: Response, next: NextFunction) => {
    const key = JSON.stringify([resource, action]);
    const installed = installedLoggers.get(res) ?? new Set<string>();
    if (installed.has(key)) {
      next();
      return;
    }
    installed.add(key);
    installedLoggers.set(res, installed);
    // Capture original json method
    const originalJson = res.json.bind(res);

    // Override json to capture response data
    res.json = function(body: any) {
      let entry: Omit<AccessLogEntry, "timestamp" | "correlationId">;
      try {
        const queryFields = identifySensitiveFields(req.query);
        const bodyFields = identifySensitiveFields(req.body);
        const responseFields = identifySensitiveFields(body);
        entry = {
          action, resource,
          resourceId: typeof req.params.id === "string" ? req.params.id : undefined,
          userId: getUserId(req),
          ipAddress: getClientIp(req),
          userAgent: stringHeader(req.headers["user-agent"]),
          fields: [...new Set([...queryFields.allSensitive, ...bodyFields.allSensitive, ...responseFields.allSensitive])],
          sensitiveFields: responseFields.allSensitive,
          piiFields: responseFields.piiFields,
          status: res.statusCode >= 200 && res.statusCode < 400 ? "success" : "failure",
          error: res.statusCode >= 400 ? `HTTP ${res.statusCode}` : undefined,
        };
      } catch {
        entry = {
          action, resource, fields: [], sensitiveFields: [], piiFields: [],
          status: "failure", error: "ACCESS_LOG_METADATA_FAILED",
        };
        reportFailure("ACCESS_LOG_METADATA_FAILED");
      }

      // One record per actual json attempt (including retries). Installation
      // is idempotent, but distinct response attempts must remain auditable.
      // Writer failures keep their exact error identity and never look like
      // successful access; audit failures cannot suppress/replace the writer.
      try {
        return originalJson(body);
      } catch (error) {
        entry.status = "failure";
        entry.error = "RESPONSE_WRITE_FAILED";
        reportFailure("RESPONSE_WRITE_FAILED");
        throw error;
      } finally {
        if (entry.error !== "ACCESS_LOG_METADATA_FAILED" && entry.error !== "RESPONSE_WRITE_FAILED") {
          // The response writer may resolve a different status than the one
          // present when the attempt began. Audit its final synchronous state.
          entry.status = res.statusCode >= 200 && res.statusCode < 400 ? "success" : "failure";
          entry.error = res.statusCode >= 400 ? `HTTP ${res.statusCode}` : undefined;
        }
        try {
          logAccess(entry);
        } catch {
          reportFailure("ACCESS_LOG_WRITE_FAILED");
        }
      }
    };

    next();
  };
}

/**
 * Middleware specifically for KYC data access
 */
export function kycAccessLogMiddleware(
  action: "read" | "write" | "update" | "delete"
) {
  return accessLogMiddleware("kyc", action);
}

/**
 * Get access logs with optional filtering
 */
export function getAccessLogs(filters?: {
  userId?: string;
  resource?: string;
  action?: string;
  startDate?: Date;
  endDate?: Date;
}): AccessLogEntry[] {
  let logs = [...accessLogs];

  if (filters) {
    if (filters.userId) {
      logs = logs.filter(log => log.userId === filters.userId);
    }
    if (filters.resource) {
      logs = logs.filter(log => log.resource === filters.resource);
    }
    if (filters.action) {
      logs = logs.filter(log => log.action === filters.action);
    }
    if (filters.startDate) {
      logs = logs.filter(log => new Date(log.timestamp) >= filters.startDate!);
    }
    if (filters.endDate) {
      logs = logs.filter(log => new Date(log.timestamp) <= filters.endDate!);
    }
  }

  return logs;
}

/**
 * Get redacted access logs for safe export
 */
export function getRedactedAccessLogs(filters?: {
  userId?: string;
  resource?: string;
  action?: string;
  startDate?: Date;
  endDate?: Date;
}): any[] {
  const logs = getAccessLogs(filters);
  
  return logs.map(log => redactPii(log as unknown as Record<string, unknown>));
}

/**
 * Clear all access logs (for testing)
 */
export function clearAccessLogs(): void {
  accessLogs.length = 0;
}

/**
 * Get access log statistics
 */
export function getAccessLogStats(): {
  total: number;
  byAction: Record<string, number>;
  byResource: Record<string, number>;
  byStatus: Record<string, number>;
} {
  const stats = {
    total: accessLogs.length,
    byAction: {} as Record<string, number>,
    byResource: {} as Record<string, number>,
    byStatus: {} as Record<string, number>
  };

  for (const log of accessLogs) {
    stats.byAction[log.action] = (stats.byAction[log.action] || 0) + 1;
    stats.byResource[log.resource] = (stats.byResource[log.resource] || 0) + 1;
    stats.byStatus[log.status] = (stats.byStatus[log.status] || 0) + 1;
  }

  return stats;
}

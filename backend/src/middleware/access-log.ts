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

// Log entry interface
export interface AccessLogEntry {
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

/**
 * Log an access event to sensitive data
 */
export function logAccess(entry: Omit<AccessLogEntry, "timestamp">): void {
  const logEntry: AccessLogEntry = {
    ...entry,
    timestamp: new Date().toISOString()
  };

  accessLogs.push(logEntry);

  // Trim old logs to prevent memory issues
  if (accessLogs.length > MAX_LOGS) {
    accessLogs.shift();
  }

  // In production, this would send to a logging service (e.g., Winston, ELK stack)
  console.log(`[ACCESS] ${logEntry.action.toUpperCase()} ${logEntry.resource} - User: ${logEntry.userId || "anonymous"} - IP: ${logEntry.ipAddress || "unknown"}`);
}

/**
 * Get client IP address from request
 */
function getClientIp(req: Request): string {
  return req.headers["x-forwarded-for"] as string || 
         req.headers["x-real-ip"] as string || 
         req.ip || 
         "unknown";
}

/**
 * Extract user identifier from request
 * In production, this would come from authentication middleware
 */
function getUserId(req: Request): string | undefined {
  // Check for authenticated user
  if (req.headers["x-user-id"]) {
    return req.headers["x-user-id"] as string;
  }
  
  // Check for API key or other auth token
  if (req.headers["authorization"]) {
    return hashForLog(req.headers["authorization"] as string);
  }
  
  return undefined;
}

/**
 * Identify sensitive and PII fields from request body/query
 */
function identifySensitiveFields(data: Record<string, any>): {
  allSensitive: string[];
  piiFields: string[];
} {
  const allSensitive: string[] = [];
  const piiFields: string[] = [];

  for (const key of Object.keys(data)) {
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
  return (req: Request, res: Response, next: NextFunction) => {
    // Capture original json method
    const originalJson = res.json.bind(res);

    // Override json to capture response data
    res.json = function(body: any) {
      // Identify fields in request
      const requestData = { ...req.query, ...req.body };
      const { allSensitive, piiFields } = identifySensitiveFields(requestData);

      // Identify fields in response
      let responseSensitive: string[] = [];
      let responsePii: string[] = [];
      
      if (body && typeof body === "object") {
        const responseFields = identifySensitiveFields(body);
        responseSensitive = responseFields.allSensitive;
        responsePii = responseFields.piiFields;
      }

      // Log the access
      logAccess({
        action,
        resource,
        resourceId: typeof req.params.id === "string" ? req.params.id : undefined,
        userId: getUserId(req),
        ipAddress: getClientIp(req),
        userAgent: req.headers["user-agent"],
        fields: [...allSensitive, ...responseSensitive],
        sensitiveFields: responseSensitive,
        piiFields: responsePii,
        status: res.statusCode >= 200 && res.statusCode < 400 ? "success" : "failure",
        error: res.statusCode >= 400 ? `HTTP ${res.statusCode}` : undefined
      });

      return originalJson(body);
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
  
  return logs.map(log => redactPii(log));
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
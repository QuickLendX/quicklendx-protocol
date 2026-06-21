import { db, DbAuditLog } from '../db/database';
import crypto from 'crypto';

export type AuditEventType = 'created' | 'used' | 'rotated' | 'revoked';

export interface AuditLogEntry {
  event_type: AuditEventType;
  key_id: string;
  actor: string;
  ip_address?: string | null;
  endpoint?: string | null;
  metadata?: Record<string, any> | null;
}

/**
 * Audit logging service for API key events
 * Logs are written asynchronously to avoid blocking request processing
 */
export class AuditLogService {
  /**
   * Log an API key event
   */
  async logEvent(entry: AuditLogEntry): Promise<void> {
    // Use setImmediate to make this truly async and non-blocking
    setImmediate(() => {
      try {
        const log: DbAuditLog = {
          id: crypto.randomUUID(),
          event_type: entry.event_type,
          key_id: entry.key_id,
          actor: entry.actor,
          timestamp: new Date().toISOString(),
          ip_address: entry.ip_address || null,
          endpoint: entry.endpoint || null,
          metadata: entry.metadata ? JSON.stringify(entry.metadata) : null,
        };

        db.createAuditLog(log);
      } catch (error) {
        // Log errors but don't throw - audit logging should never break the main flow
        console.error('[AuditLog] Failed to write audit log:', error);
      }
    });
  }

  /**
   * Log API key creation
   */
  async logCreated(keyId: string, actor: string, ipAddress?: string): Promise<void> {
    await this.logEvent({
      event_type: 'created',
      key_id: keyId,
      actor,
      ip_address: ipAddress,
    });
  }

  /**
   * Log API key usage
   */
  async logUsed(keyId: string, actor: string, endpoint: string, ipAddress?: string): Promise<void> {
    await this.logEvent({
      event_type: 'used',
      key_id: keyId,
      actor,
      endpoint,
      ip_address: ipAddress,
    });
  }

  /**
   * Log API key rotation
   */
  async logRotated(
    oldKeyId: string,
    newKeyId: string,
    actor: string,
    ipAddress?: string
  ): Promise<void> {
    await this.logEvent({
      event_type: 'rotated',
      key_id: oldKeyId,
      actor,
      ip_address: ipAddress,
      metadata: { new_key_id: newKeyId },
    });
  }

  /**
   * Log API key revocation
   */
  async logRevoked(keyId: string, actor: string, ipAddress?: string): Promise<void> {
    await this.logEvent({
      event_type: 'revoked',
      key_id: keyId,
      actor,
      ip_address: ipAddress,
    });
  }

  /**
   * Get audit logs for a specific key
   */
  getLogsForKey(keyId: string): DbAuditLog[] {
    return db.getAuditLogs({ key_id: keyId });
  }

  /**
   * Get audit logs by event type
   */
  getLogsByEventType(eventType: AuditEventType): DbAuditLog[] {
    return db.getAuditLogs({ event_type: eventType });
  }

  /**
   * Get all audit logs
   */
  getAllLogs(): DbAuditLog[] {
    return db.getAuditLogs();
  }
}

// Singleton instance
export const auditLogService = new AuditLogService();

import { AdminRole } from "../types/rbac";

export type AuditOutcome = "allowed" | "denied" | "performed";

type AuditRole = AdminRole | "anonymous";

export interface AuditLogEntry {
  timestamp: string;
  action: string;
  outcome: AuditOutcome;
  role: AuditRole;
  method: string;
  path: string;
  ip: string;
  reason?: string;
  metadata?: Record<string, unknown>;
}

interface AuditAuthorizationEvent {
  action: string;
  outcome: Extract<AuditOutcome, "allowed" | "denied">;
  role: AuditRole;
  method: string;
  path: string;
  ip: string;
  reason?: string;
}

interface AuditAdminActionEvent {
  action: string;
  role: AdminRole;
  method: string;
  path: string;
  ip: string;
  metadata?: Record<string, unknown>;
}

class AuditLogService {
  private readonly maxEntries = 250;
  private entries: AuditLogEntry[] = [];

  public recordAuthorization(event: AuditAuthorizationEvent): void {
    this.push({
      timestamp: new Date().toISOString(),
      action: event.action,
      outcome: event.outcome,
      role: event.role,
      method: event.method,
      path: event.path,
      ip: event.ip,
      reason: event.reason,
    });
  }

  public recordAdminAction(event: AuditAdminActionEvent): void {
    this.push({
      timestamp: new Date().toISOString(),
      action: event.action,
      outcome: "performed",
      role: event.role,
      method: event.method,
      path: event.path,
      ip: event.ip,
      metadata: event.metadata,
    });
  }

  public listEntries(limit = 50): AuditLogEntry[] {
    const safeLimit = Math.min(Math.max(Math.trunc(limit), 1), 100);
    return this.entries.slice(-safeLimit).reverse();
  }

  public clear(): void {
    this.entries = [];
  }

  private push(entry: AuditLogEntry): void {
    this.entries.push(entry);
    if (this.entries.length > this.maxEntries) {
      this.entries = this.entries.slice(-this.maxEntries);
    }
  }
}

export const auditLogService = new AuditLogService();

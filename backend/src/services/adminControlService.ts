import { AdminRole } from "../types/rbac";

export interface AdminAuditEntry {
  id: string;
  action: string;
  actor: AdminRole;
  timestamp: string;
  metadata: Record<string, unknown>;
}

export interface BackfillJob {
  id: string;
  scope: string;
  status: "queued";
  requestedAt: string;
  requestedBy: AdminRole;
}

export interface DangerousConfig {
  allowEmergencyConfigChanges: boolean;
  maintenanceWindowMinutes: number;
  updatedAt: string | null;
  updatedBy: AdminRole | null;
}

const DEFAULT_DANGEROUS_CONFIG: DangerousConfig = {
  allowEmergencyConfigChanges: false,
  maintenanceWindowMinutes: 30,
  updatedAt: null,
  updatedBy: null,
};

class AdminControlService {
  private readonly maxJobs = 50;
  private backfillJobs: BackfillJob[] = [];
  private dangerousConfig: DangerousConfig = { ...DEFAULT_DANGEROUS_CONFIG };
  private auditLog: AdminAuditEntry[] = [];
  private readonly maxAuditEntries = 1000;

  private recordAudit(
    action: string,
    actor: AdminRole,
    metadata: Record<string, unknown> = {}
  ): AdminAuditEntry {
    const entry: AdminAuditEntry = {
      id: `audit_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
      action,
      actor,
      timestamp: new Date().toISOString(),
      metadata,
    };
    this.auditLog.push(entry);
    if (this.auditLog.length > this.maxAuditEntries) {
      this.auditLog = this.auditLog.slice(-this.maxAuditEntries);
    }
    return entry;
  }

  public getAuditLog(): AdminAuditEntry[] {
    return [...this.auditLog].reverse();
  }

  public queueBackfill(requestedBy: AdminRole, scope: string): BackfillJob {
    const job: BackfillJob = {
      id: `backfill_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
      scope,
      status: "queued",
      requestedAt: new Date().toISOString(),
      requestedBy,
    };

    this.backfillJobs.push(job);
    if (this.backfillJobs.length > this.maxJobs) {
      this.backfillJobs = this.backfillJobs.slice(-this.maxJobs);
    }

    this.recordAudit("queue_backfill", requestedBy, { jobId: job.id, scope });
    return job;
  }

  public listBackfillJobs(): BackfillJob[] {
    return [...this.backfillJobs].reverse();
  }

  public getDangerousConfig(): DangerousConfig {
    return { ...this.dangerousConfig };
  }

  public updateDangerousConfig(
    requestedBy: AdminRole,
    config: Pick<
      DangerousConfig,
      "allowEmergencyConfigChanges" | "maintenanceWindowMinutes"
    >,
  ): DangerousConfig {
    const prev = { ...this.dangerousConfig };
    this.dangerousConfig = {
      ...config,
      updatedAt: new Date().toISOString(),
      updatedBy: requestedBy,
    };
    this.recordAudit("update_dangerous_config", requestedBy, {
      previous: prev,
      updated: config,
    });
    return this.getDangerousConfig();
  }

  public reset(): void {
    this.backfillJobs = [];
    this.dangerousConfig = { ...DEFAULT_DANGEROUS_CONFIG };
    this.auditLog = [];
  }
}

export const adminControlService = new AdminControlService();

import { AdminRole } from "../types/rbac";

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
    this.dangerousConfig = {
      ...config,
      updatedAt: new Date().toISOString(),
      updatedBy: requestedBy,
    };

    return this.getDangerousConfig();
  }

  public reset(): void {
    this.backfillJobs = [];
    this.dangerousConfig = { ...DEFAULT_DANGEROUS_CONFIG };
  }
}

export const adminControlService = new AdminControlService();

import { z } from "zod";

export const BackfillStartRequestSchema = z.object({
  startLedger: z.number().int().nonnegative(),
  endLedger: z.number().int().nonnegative(),
  dryRun: z.boolean().optional().default(false),
  concurrency: z.number().int().positive().optional().default(1),
  idempotencyKey: z.string().min(1).max(128).optional(),
});

export const BackfillActionSchema = z.object({
  runId: z.string().min(1),
});

export const BackfillRunStatusSchema = z.enum([
  "running",
  "paused",
  "completed",
  "failed",
]);

export const BackfillAuditEventTypeSchema = z.enum([
  "preview",
  "started",
  "paused",
  "resumed",
  "completed",
  "failed",
  "idempotent_reuse",
]);

export type BackfillStartRequest = z.infer<typeof BackfillStartRequestSchema>;
export type BackfillActionRequest = z.infer<typeof BackfillActionSchema>;
export type BackfillRunStatus = z.infer<typeof BackfillRunStatusSchema>;
export type BackfillAuditEventType = z.infer<typeof BackfillAuditEventTypeSchema>;

export interface BackfillRun {
  id: string;
  startLedger: number;
  endLedger: number;
  dryRun: boolean;
  concurrency: number;
  status: BackfillRunStatus;
  processedLedgers: number;
  cursorLedger: number;
  actor: string;
  createdAt: string;
  updatedAt: string;
  completedAt?: string;
  error?: string;
  idempotencyKey?: string;
}

export interface BackfillPreview {
  range: {
    startLedger: number;
    endLedger: number;
    totalLedgers: number;
  };
  estimatedAffectedRecords: number;
  concurrency: number;
}

export interface BackfillAuditEntry {
  runId: string;
  timestamp: string;
  eventType: BackfillAuditEventType;
  actor: string;
  metadata: Record<string, unknown>;
}

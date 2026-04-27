import { z } from "zod";

export const RawEventSchema = z.object({
  id: z.string().min(1),
  ledger: z.number().int().nonnegative(),
  txHash: z.string().min(1),
  type: z.string().min(1),
  payload: z.record(z.unknown()),
  timestamp: z.number().int().nonnegative(),
  complianceHold: z.boolean().default(false),
  indexedAt: z.string().datetime(), // ISO 8601 timestamp
});

export const ReplayStartRequestSchema = z.object({
  fromLedger: z.number().int().nonnegative(),
  toLedger: z.number().int().nonnegative(),
  dryRun: z.boolean().optional().default(false),
  batchSize: z.number().int().positive().optional().default(100),
  forceRebuild: z.boolean().optional().default(false), // Clear existing derived tables
  idempotencyKey: z.string().min(1).max(128).optional(),
});

export const ReplayActionSchema = z.object({
  runId: z.string().min(1),
});

export const ReplayRunStatusSchema = z.enum([
  "pending",
  "running", 
  "paused",
  "completed",
  "failed",
]);

export const ReplayAuditEventTypeSchema = z.enum([
  "started",
  "paused",
  "resumed", 
  "completed",
  "failed",
  "validation_failed",
  "idempotent_reuse",
]);

export type RawEvent = z.infer<typeof RawEventSchema>;
export type ReplayStartRequest = z.infer<typeof ReplayStartRequestSchema>;
export type ReplayActionRequest = z.infer<typeof ReplayActionSchema>;
export type ReplayRunStatus = z.infer<typeof ReplayRunStatusSchema>;
export type ReplayAuditEventType = z.infer<typeof ReplayAuditEventTypeSchema>;

export interface ReplayRun {
  id: string;
  fromLedger: number;
  toLedger: number;
  dryRun: boolean;
  batchSize: number;
  forceRebuild: boolean;
  status: ReplayRunStatus;
  processedEvents: number;
  cursorLedger: number;
  actor: string;
  createdAt: string;
  updatedAt: string;
  completedAt?: string;
  error?: string;
  idempotencyKey?: string;
  validationErrors?: string[];
}

export interface ReplayPreview {
  range: {
    fromLedger: number;
    toLedger: number;
    totalLedgers: number;
  };
  estimatedEvents: number;
  batchSize: number;
  forceRebuild: boolean;
}

export interface ReplayAuditEntry {
  runId: string;
  timestamp: string;
  eventType: ReplayAuditEventType;
  actor: string;
  metadata: Record<string, unknown>;
}

export interface ReplayStats {
  totalEvents: number;
  processedEvents: number;
  failedEvents: number;
  skippedEvents: number;
  currentLedger: number;
  estimatedCompletion?: string;
}

// Raw event store interface
export interface RawEventStore {
  // Store raw events from blockchain
  storeEvents(events: RawEvent[]): Promise<void>;
  
  // Retrieve events in ledger range for replay
  getEventsByLedgerRange(fromLedger: number, toLedger: number, limit?: number): Promise<RawEvent[]>;
  
  // Get event count for range
  getEventCount(fromLedger: number, toLedger: number): Promise<number>;
  
  // Get min/max ledger for bounds checking
  getLedgerBounds(): Promise<{ min: number | null; max: number | null }>;
  
  // Delete events by ledger range (for cleanup/migration)
  deleteEventsByLedgerRange(fromLedger: number, toLedger: number): Promise<number>;
  
  // Get replay cursor (last processed ledger)
  getReplayCursor(): Promise<number | null>;
  
  // Set replay cursor
  setReplayCursor(ledger: number): Promise<void>;
}

// Derived table store interface
export interface DerivedTableStore {
  // Clear all derived tables (for force rebuild)
  clearDerivedTables(): Promise<void>;
  
  // Get current state hash for verification
  getStateHash(): Promise<string>;
  
  // Begin transaction for batch processing
  beginTransaction(): Promise<void>;
  
  // Commit transaction
  commitTransaction(): Promise<void>;
  
  // Rollback transaction
  rollbackTransaction(): Promise<void>;
}

// Security validation interface
export interface EventValidator {
  validateEvent(event: RawEvent): Promise<string[]>; // Returns array of validation errors
  sanitizeEvent(event: RawEvent): Promise<RawEvent>; // Returns sanitized event
}

import { AdminRole } from "../types/rbac";

export type AuditOutcome = "allowed" | "denied" | "performed";

type AuditRole = AdminRole | "anonymous";

export interface AuditLogEntry {
  id: string;
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

export interface AuditLogPage {
  entries: AuditLogEntry[];
  nextCursor: string | null;
  hasMore: boolean;
}

export class InvalidCursorError extends Error {
  constructor(message = "Invalid cursor") {
    super(message);
    this.name = "InvalidCursorError";
  }
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
  outcome?: Extract<AuditOutcome, "denied" | "performed">;
  reason?: string;
  metadata?: Record<string, unknown>;
}

class AuditLogService {
  private readonly maxEntries = 250;
  private entries: AuditLogEntry[] = [];
  private seq = 0;

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
      outcome: event.outcome ?? "performed",
      role: event.role,
      method: event.method,
      path: event.path,
      ip: event.ip,
      reason: event.reason,
      metadata: event.metadata,
    });
  }

  public listEntries(limit = 50): AuditLogEntry[] {
    return this.listPage(undefined, limit).entries;
  }

  public listPage(cursor?: string | null, limit = 50): AuditLogPage {
    const safeLimit = this.normalizeLimit(limit);
    const toIndex = this.resolveToIndex(cursor);
    const fromIndex = Math.max(0, toIndex - safeLimit);
    const entries = this.entries.slice(fromIndex, toIndex).reverse();
    const hasMore = fromIndex > 0;
    const nextCursor = hasMore ? this.encodeCursor(this.entries[fromIndex].id) : null;
    return { entries, nextCursor, hasMore };
  }

  public clear(): void {
    this.entries = [];
  }

  private push(entry: Omit<AuditLogEntry, "id">): void {
    const fullEntry: AuditLogEntry = { id: this.nextId(), ...entry };
    this.entries.push(fullEntry);
    if (this.entries.length > this.maxEntries) {
      this.entries = this.entries.slice(-this.maxEntries);
    }
  }

  private nextId(): string {
    this.seq += 1;
    return `${Date.now().toString(36)}-${this.seq.toString(36)}`;
  }

  private normalizeLimit(limit: number): number {
    return Math.min(Math.max(Math.trunc(limit), 1), 100);
  }

  private resolveToIndex(cursor?: string | null): number {
    if (cursor === undefined || cursor === null || cursor === "") {
      return this.entries.length;
    }
    const id = this.decodeCursor(cursor);
    const index = this.entries.findIndex(entry => entry.id === id);
    if (index === -1) {
      throw new InvalidCursorError("Cursor refers to an entry that no longer exists");
    }
    return index;
  }

  private encodeCursor(id: string): string {
    return Buffer.from(JSON.stringify({ v: 1, id }), "utf8").toString("base64");
  }

  private decodeCursor(cursor: string): string {
    let parsed: unknown;
    try {
      parsed = JSON.parse(Buffer.from(cursor, "base64").toString("utf8"));
    } catch {
      throw new InvalidCursorError("Malformed cursor");
    }
    if (typeof parsed === "object" && parsed !== null && typeof (parsed as { id?: unknown }).id === "string") {
      return (parsed as { id: string }).id;
    }
    throw new InvalidCursorError("Malformed cursor");
  }
}

export const auditLogService = new AuditLogService();
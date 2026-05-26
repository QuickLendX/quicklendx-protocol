import * as fs from "fs";
import * as path from "path";
import { ulid } from "ulid";
import {
  AuditEntry,
  AuditEntrySchema,
  AuditQuerySchema,
  AuditQuery,
  AuditQueryResponse,
  AuditOperation,
} from "../types/audit";

const MAX_LINE_BYTES = 10 * 1024;

function getAuditDir(): string {
  return process.env.AUDIT_DIR || "audit_logs";
}

class AuditService {
  private static instance: AuditService;

  private constructor() {
    this.ensureAuditDir();
  }

  public static getInstance(): AuditService {
    if (!AuditService.instance) {
      AuditService.instance = new AuditService();
    }
    return AuditService.instance;
  }

  public static resetInstance(): void {
    AuditService.instance = undefined as unknown as AuditService;
  }

  private ensureAuditDir(): void {
    const dir = getAuditDir();
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
  }

  private logFilePath(date: string): string {
    return path.join(getAuditDir(), `audit-${date}.jsonl`);
  }

  private todayDate(): string {
    return new Date().toISOString().slice(0, 10);
  }

  private generateId(): string {
    return ulid();
  }

  append(entry: Omit<AuditEntry, "id" | "timestamp">): AuditEntry {
    const full: AuditEntry = {
      ...entry,
      id: this.generateId(),
      timestamp: new Date().toISOString(),
    };

    const validated = AuditEntrySchema.parse(full);
    const line = JSON.stringify(validated);

    if (line.length > MAX_LINE_BYTES) {
      throw new Error(
        `Audit entry exceeds maximum size of ${MAX_LINE_BYTES} bytes`
      );
    }

    const filePath = this.logFilePath(validated.timestamp.slice(0, 10));
    fs.appendFileSync(filePath, line + "\n", "utf8");

    return validated;
  }

  query(rawParams: {
    actor?: string;
    operation?: string;
    from?: string;
    to?: string;
    limit?: string | number;
    offset?: string | number;
  }): AuditQueryResponse {
    const parsed = AuditQuerySchema.parse(rawParams) as AuditQuery;
    return this.queryWithSchema(parsed);
  }

  private queryWithSchema(params: AuditQuery): AuditQueryResponse {
    const dates = this.getDateRange(params.from, params.to);
    const allEntries: AuditEntry[] = [];

    for (const date of dates) {
      const filePath = this.logFilePath(date);
      if (!fs.existsSync(filePath)) continue;

      const lines = fs.readFileSync(filePath, "utf8").split("\n");
      for (const line of lines) {
        if (!line.trim()) continue;
        try {
          allEntries.push(AuditEntrySchema.parse(JSON.parse(line)));
        } catch {
          continue;
        }
      }
    }

    const filtered = allEntries.filter((e) => {
      if (params.actor && e.actor !== params.actor) return false;
      if (params.operation && e.operation !== params.operation) return false;
      if (params.from && e.timestamp < params.from) return false;
      if (params.to && e.timestamp > params.to) return false;
      return true;
    });

    filtered.sort(
      (a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()
    );

    const total = filtered.length;
    const page = filtered.slice(params.offset, params.offset + params.limit);

    return {
      entries: page,
      total,
      limit: params.limit,
      offset: params.offset,
      hasMore: params.offset + params.limit < total,
    };
  }

  private getDateRange(from?: string, to?: string): string[] {
    const today = new Date().toISOString().slice(0, 10);
    const endDate = to ? to.slice(0, 10) : today;
    const startDate = from ? from.slice(0, 10) : endDate;

    if (startDate > endDate) return [];

    const dates: string[] = [];
    let cur = startDate;
    while (cur <= endDate) {
      dates.push(cur);
      const d = new Date(cur);
      d.setDate(d.getDate() + 1);
      cur = d.toISOString().slice(0, 10);
    }
    return dates;
  }

  getEntriesForTest(): AuditEntry[] {
    const today = this.todayDate();
    const filePath = this.logFilePath(today);
    if (!fs.existsSync(filePath)) return [];

    return fs
      .readFileSync(filePath, "utf8")
      .split("\n")
      .filter((l) => l.trim())
      .map((l) => AuditEntrySchema.parse(JSON.parse(l)));
  }

  clearAll(): void {
    const dir = getAuditDir();
    if (!fs.existsSync(dir)) return;
    for (const file of fs.readdirSync(dir)) {
      if (file.startsWith("audit-") && file.endsWith(".jsonl")) {
        fs.unlinkSync(path.join(dir, file));
      }
    }
  }

  setAuditDir(dir: string): void {
    (process.env as Record<string, string>).AUDIT_DIR = dir;
    this.ensureAuditDir();
  }
}

export { AuditService };
export const auditService = {
  append: (...args: Parameters<AuditService["append"]>) =>
    AuditService.getInstance().append(...args),
  query: (...args: Parameters<AuditService["query"]>) =>
    AuditService.getInstance().query(...args),
  getEntriesForTest: () => AuditService.getInstance().getEntriesForTest(),
  clearAll: () => AuditService.getInstance().clearAll(),
  setAuditDir: (...args: Parameters<AuditService["setAuditDir"]>) =>
    AuditService.getInstance().setAuditDir(...args),
};
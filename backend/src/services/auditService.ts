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
  AUDIT_CHAIN_GENESIS_HASH,
  computeEntryHash,
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

  private getLastEntry(filePath: string): AuditEntry | null {
    if (!fs.existsSync(filePath)) {
      return null;
    }
    const content = fs.readFileSync(filePath, "utf8").trim();
    const lines = content.split("\n");
    if (lines.length === 0 || !lines[lines.length - 1]) {
      return null;
    }
    try {
      return AuditEntrySchema.parse(JSON.parse(lines[lines.length - 1]));
    } catch {
      return null;
    }
  }

  append(entry: Omit<AuditEntry, "id" | "timestamp" | "prevHash" | "entryHash">): AuditEntry {
    const timestamp = new Date().toISOString();
    const filePath = this.logFilePath(timestamp.slice(0, 10));
    const lastEntry = this.getLastEntry(filePath);
    const prevHash = lastEntry ? lastEntry.entryHash : AUDIT_CHAIN_GENESIS_HASH;

    const full: AuditEntry = {
      ...entry,
      id: this.generateId(),
      timestamp,
      prevHash,
    };

    const entryHash = computeEntryHash(full);
    const validated = AuditEntrySchema.parse({ ...full, entryHash });
    const line = JSON.stringify(validated);

    if (line.length > MAX_LINE_BYTES) {
      throw new Error(
        `Audit entry exceeds maximum size of ${MAX_LINE_BYTES} bytes`
      );
    }

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

  verifyChain(date: string): { ok: boolean; brokenAt?: number } {
    const filePath = this.logFilePath(date);
    if (!fs.existsSync(filePath)) {
      return { ok: true }; // An empty or non-existent log is valid.
    }

    const lines = fs.readFileSync(filePath, "utf8").split("\n").filter(line => line.trim());
    let expectedPrevHash = AUDIT_CHAIN_GENESIS_HASH;

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      const lineNumber = i + 1;
      let entry: AuditEntry;

      try {
        entry = AuditEntrySchema.parse(JSON.parse(line));
      } catch (e) {
        return { ok: false, brokenAt: lineNumber }; // Malformed JSON or schema violation
      }

      if (entry.prevHash !== expectedPrevHash) {
        return { ok: false, brokenAt: lineNumber }; // Chain is broken
      }

      const actualEntryHash = computeEntryHash(entry);
      if (actualEntryHash !== entry.entryHash) {
        return { ok: false, brokenAt: lineNumber }; // Entry content was tampered with
      }

      expectedPrevHash = entry.entryHash;
    }

    return { ok: true };
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

  getAllEntries(): AuditEntry[] {
    const dir = getAuditDir();
    if (!fs.existsSync(dir)) return [];

    const entries: AuditEntry[] = [];
    const files = fs
      .readdirSync(dir)
      .filter((file) => file.startsWith("audit-") && file.endsWith(".jsonl"))
      .sort();

    for (const file of files) {
      const filePath = path.join(dir, file);
      const lines = fs.readFileSync(filePath, "utf8").split("\n");
      for (const line of lines) {
        if (!line.trim()) continue;
        try {
          entries.push(AuditEntrySchema.parse(JSON.parse(line)));
        } catch {
          continue;
        }
      }
    }

    return entries.sort(
      (a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime()
    );
  }

  replaceEntries(entries: AuditEntry[]): void {
    const dir = getAuditDir();
    const parentDir = path.dirname(dir);
    const tempDir = path.join(parentDir, `${path.basename(dir)}.tmp-${Date.now()}`);
    const backupDir = path.join(parentDir, `${path.basename(dir)}.bak-${Date.now()}`);
    const grouped = new Map<string, string[]>();

    fs.mkdirSync(tempDir, { recursive: true });

    for (const entry of entries) {
      const validated = AuditEntrySchema.parse(entry);
      const date = validated.timestamp.slice(0, 10);
      const existing = grouped.get(date) ?? [];
      existing.push(JSON.stringify(validated));
      grouped.set(date, existing);
    }

    for (const [date, lines] of grouped.entries()) {
      fs.writeFileSync(
        path.join(tempDir, `audit-${date}.jsonl`),
        `${lines.join("\n")}\n`,
        "utf8"
      );
    }

    if (fs.existsSync(dir)) {
      fs.renameSync(dir, backupDir);
    }

    try {
      fs.renameSync(tempDir, dir);
      if (fs.existsSync(backupDir)) {
        fs.rmSync(backupDir, { recursive: true, force: true });
      }
    } catch (error) {
      if (fs.existsSync(dir)) {
        fs.rmSync(dir, { recursive: true, force: true });
      }
      if (fs.existsSync(backupDir)) {
        fs.renameSync(backupDir, dir);
      }
      if (fs.existsSync(tempDir)) {
        fs.rmSync(tempDir, { recursive: true, force: true });
      }
      throw error;
    }
  }
}

export { AuditService };
export const auditService = {
  append: (...args: Parameters<AuditService["append"]>) =>
    AuditService.getInstance().append(...args),
  query: (...args: Parameters<AuditService["query"]>) =>
    AuditService.getInstance().query(...args),
  verifyChain: (...args: Parameters<AuditService["verifyChain"]>) =>
    AuditService.getInstance().verifyChain(...args),
  getEntriesForTest: () => AuditService.getInstance().getEntriesForTest(),
  clearAll: () => AuditService.getInstance().clearAll(),
  setAuditDir: (...args: Parameters<AuditService["setAuditDir"]>) =>
    AuditService.getInstance().setAuditDir(...args),
  getAllEntries: () => AuditService.getInstance().getAllEntries(),
  replaceEntries: (...args: Parameters<AuditService["replaceEntries"]>) =>
    AuditService.getInstance().replaceEntries(...args),
};

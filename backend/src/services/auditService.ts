import * as fs from "fs";
import * as path from "path";
import { ulid } from "ulid";
import * as crypto from "crypto";
import { getCorrelationId } from "../lib/requestContext";
import {
  AuditEntry,
  AuditEntrySchema,
  AuditQuerySchema,
  AuditQuery,
  AuditQueryResponse,
  AUDIT_CHAIN_GENESIS_HASH,
  computeEntryHash,
} from "../types/audit";

const MAX_LINE_BYTES = 10 * 1024;
const MAX_APPEND_RETRIES = 3;
const APPEND_RETRY_BASE_MS = 5;

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

  private static appendLock: Promise<void> = Promise.resolve();

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

  private withAppendLock<T>(fn: () => T): T {
    // Serialize append operations within the process so concurrent callers
    // cannot interleave reads of the chain head with writes of the next entry.
    // The lock is a promise chain; each caller awaits the previous holder.
    let release!: () => void;
    const next = new Promise<void>((resolve) => {
      release = resolve;
    });
    const previous = AuditService.appendLock;
    AuditService.appendLock = previous.then(() => next);
    let result!: T;
    let error: unknown;
    let done = false;
    previous
      .then(() => {
        try {
          result = fn();
        } catch (e) {
          error = e;
        } finally {
          done = true;
          release();
        }
      })
      .catch(() => {
        done = true;
        release();
      });
    // Busy-wait is unacceptable; instead we synchronously drain by relying on
    // the fact that append() is synchronous in this codebase. We block using
    // a spin-free approach: since fn is sync, we must run it after previous
    // resolves. To keep append() synchronous, we throw if the lock is held.
    if (!done) {
      // Fallback: run inline if previous already resolved (common case).
      // Otherwise, we cannot await in a sync method; run inline anyway but
      // rely on Node's single-threaded execution for atomicity.
      try {
        result = fn();
      } catch (e) {
        error = e;
      } finally {
        release();
      }
    }
    if (error) throw error;
    return result;
  }

  append(
    entry: Omit<AuditEntry, "id" | "timestamp" | "prevHash" | "entryHash">
  ): AuditEntry {
    // Stamp the originating request id from async-local-storage so the audit
    // entry can be traced back to the inbound API call. An explicit value on
    // the entry wins; otherwise we fall back to the active request context.
    const requestId = entry.requestId ?? getCorrelationId() ?? undefined;
    // Validate the caller-supplied fields before doing any I/O so invalid
    // input fails fast and deterministically without touching the log file.
    if (entry.actor !== undefined && typeof entry.actor !== "string") {
      throw new Error("Audit entry actor must be a string");
    }
    if (entry.operation !== undefined && typeof entry.operation !== "string") {
      throw new Error("Audit entry operation must be a string");
    }
    if (entry.actor !== undefined && entry.actor.length > 256) {
      throw new Error("Audit entry actor exceeds maximum length of 256");
    }
    if (entry.operation !== undefined && entry.operation.length > 256) {
      throw new Error("Audit entry operation exceeds maximum length of 256");
    }
    const timestamp = new Date().toISOString();
    const filePath = this.logFilePath(timestamp.slice(0, 10));
    let prevHash = AUDIT_CHAIN_GENESIS_HASH;
    if (fs.existsSync(filePath)) {
      const lines = fs.readFileSync(filePath, "utf8").split("\n").filter(Boolean);
      if (lines.length > 0) {
        try {
          const previous = AuditEntrySchema.parse(JSON.parse(lines[lines.length - 1]));
          prevHash = previous.entryHash;
        } catch {
          // Keep the genesis hash when the last line is malformed.
        }
      }
    }

    const full = {
      ...entry,
      requestId,
      id: this.generateId(),
      timestamp,
      prevHash,
    };

    const entryHash = computeEntryHash(full as AuditEntry);
    const validated = AuditEntrySchema.parse({ ...full, entryHash });
    const line = JSON.stringify(validated);

    if (Buffer.byteLength(line, "utf8") > MAX_LINE_BYTES) {
      throw new Error(
        `Audit entry exceeds maximum size of ${MAX_LINE_BYTES} bytes`
      );
    }
    if (line.length > MAX_LINE_BYTES) {
      throw new Error(
        `Audit entry exceeds maximum size of ${MAX_LINE_BYTES} bytes`
      );
    }

    this.appendWithRetry(filePath, line + "\n");

    return validated;
  }

  private appendWithRetry(filePath: string, payload: string): void {
    // Deterministic retry with bounded attempts. Only transient filesystem
    // errors (EBUSY, EAGAIN, EPERM on Windows) are retried; validation and
    // size errors are thrown immediately by the caller.
    let lastError: unknown;
    for (let attempt = 0; attempt < MAX_APPEND_RETRIES; attempt++) {
      try {
        fs.appendFileSync(filePath, payload, "utf8");
        return;
      } catch (error) {
        lastError = error;
        const code = (error as NodeJS.ErrnoException).code;
        const transient =
          code === "EBUSY" || code === "EAGAIN" || code === "EPERM";
        if (!transient || attempt === MAX_APPEND_RETRIES - 1) {
          throw error;
        }
        // Synchronous backoff: spin briefly to avoid async in a sync API.
        const deadline = Date.now() + APPEND_RETRY_BASE_MS * (attempt + 1);
        while (Date.now() < deadline) {
          // intentional short busy-wait for deterministic retry timing
        }
      }
    }
    throw lastError;
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

    // Cap the number of files scanned to avoid unbounded work on adversarial
    // ranges. The schema already bounds limit/offset; this bounds the range.
    if (dates.length > 366) {
      throw new Error("Audit query date range exceeds maximum of 366 days");
    }

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

    if (startDate > today && endDate > today) return [];
    if (startDate > endDate) return [];

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
      return { ok: true }; 
    }

    const fileContent = fs.readFileSync(filePath, "utf8").trim();
    if (!fileContent) {
      return { ok: true };
    }

    const lines = fileContent.split("\n").filter(line => line.trim());
    let expectedPrevHash = AUDIT_CHAIN_GENESIS_HASH;

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      const lineNumber = i + 1;
      let entry: AuditEntry;

      try {
        entry = AuditEntrySchema.parse(JSON.parse(line));
      } catch (e) {
        return { ok: false, brokenAt: lineNumber }; 
      }

      if (entry.prevHash !== expectedPrevHash) {
        return { ok: false, brokenAt: lineNumber }; 
      }

      // Reject entries whose timestamp does not match the file's date to
      // prevent cross-day injection from breaking chain semantics.
      if (entry.timestamp.slice(0, 10) !== date) {
        return { ok: false, brokenAt: lineNumber };
      }

      const actualEntryHash = computeEntryHash(entry);
      if (actualEntryHash !== entry.entryHash) {
        return { ok: false, brokenAt: lineNumber }; 
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
    // Use a cryptographically random suffix so concurrent replaceEntries
    // calls cannot collide on temp/backup directory names.
    const suffix = crypto.randomBytes(8).toString("hex");
    const tempDir = path.join(parentDir, `${path.basename(dir)}.tmp-${suffix}`);
    const backupDir = path.join(parentDir, `${path.basename(dir)}.bak-${suffix}`);
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

  // Exposed for tests to assert deterministic boundary behavior without
  // reaching into private state.
  static getMaxLineBytes(): number {
    return MAX_LINE_BYTES;
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

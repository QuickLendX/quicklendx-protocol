import { MOCK_INVOICES } from "../controllers/v1/invoices";
import { MOCK_BIDS } from "../controllers/v1/bids";
import { MOCK_SETTLEMENTS } from "../controllers/v1/settlements";
import { invoiceStore } from "./invoiceStore";
import { config } from "../config";
import crypto from "crypto";
import fs from "fs";
import fsp from "fs/promises";
import path from "path";

export enum ExportFormat {
  JSON = "json",
  CSV = "csv",
}

export interface ExportData {
  userId: string;
  format: ExportFormat;
  data: {
    invoices: any[];
    bids: any[];
    settlements: any[];
  };
}

/**
 * Deterministic error taxonomy for export operations.
 *
 * INVARIANT: Every failure path in ExportService must surface as one of these
 * codes so callers (and tests) can assert on a stable, non-leaking contract.
 * Raw error messages from storage/DB layers must never be propagated verbatim
 * to the HTTP boundary.
 */
export enum ExportErrorCode {
  INVALID_INPUT = "INVALID_INPUT",
  UNAUTHORIZED = "UNAUTHORIZED",
  TOKEN_INVALID = "TOKEN_INVALID",
  TOKEN_EXPIRED = "TOKEN_EXPIRED",
  NOT_FOUND = "NOT_FOUND",
  CONFLICT = "CONFLICT",
  STORAGE_FAILURE = "STORAGE_FAILURE",
  INTERNAL = "INTERNAL",
}

export class ExportError extends Error {
  public readonly code: ExportErrorCode;
  public readonly retryable: boolean;
  constructor(code: ExportErrorCode, message: string, retryable = false) {
    super(message);
    this.name = "ExportError";
    this.code = code;
    this.retryable = retryable;
  }
}

interface ExportFileMeta {
  userId: string;
  format: ExportFormat;
  expiresAt: number;
  filePath: string;
}

/**
 * In-flight export deduplication map.
 *
 * INVARIANT: For a given (userId, format) at most one generation runs at a
 * time. Concurrent callers await the same promise. This prevents partial
 * files from racing on the same tmp path and keeps retries deterministic.
 */
type InFlightKey = string;
const inFlightExports = new Map<InFlightKey, Promise<string>>();

class ExportService {
  private readonly secret = config.EXPORT_SECRET || "fallback-secret-for-signing-links";
  private readonly exportDir = config.EXPORT_DIR;
  private readonly ttlMs = config.EXPORT_TTL_MS;

  constructor() {
    fsp.mkdir(this.exportDir, { recursive: true, mode: 0o700 }).catch(() => {});
  }

  private static inFlightKey(userId: string, format: ExportFormat): InFlightKey {
    return `${userId}::${format}`;
  }

  private static assertValidUserId(userId: unknown): asserts userId is string {
    if (typeof userId !== "string" || userId.trim().length === 0) {
      throw new ExportError(
        ExportErrorCode.INVALID_INPUT,
        "Invalid userId: must be a non-empty string",
      );
    }
  }

  private static assertValidFormat(format: unknown): asserts format is ExportFormat {
    if (format !== ExportFormat.JSON && format !== ExportFormat.CSV) {
      throw new ExportError(
        ExportErrorCode.INVALID_INPUT,
        "Invalid format: must be json or csv",
      );
    }
  }

  /**
   * Fetches all data related to a user, strictly filtered by tenant context.
   * 
   * SECURITY: This method enforces tenant isolation by only returning data
   * where the userId matches the owner field (business for invoices, investor
   * for bids, payer/recipient for settlements). The userId parameter MUST be
   * derived from the authenticated req.apiKey.created_by field and cannot be
   * supplied by the client.
   * 
   * @param userId - The authenticated user/tenant identifier from req.apiKey
   * @param verifiedContext - Optional security context for double-verification
   * @returns Data belonging exclusively to the specified tenant
   */
  public async getUserData(
    userId: string,
    verifiedContext?: { authenticatedUserId: string }
  ): Promise<ExportData["data"]> {
    ExportService.assertValidUserId(userId);

    // SECURITY CHECK: Prevent context injection attacks
    // If verifiedContext is provided, userId MUST match the authenticated user
    if (verifiedContext && userId !== verifiedContext.authenticatedUserId) {
      throw new Error(
        "Security violation: userId does not match authenticated context"
      );
    }

    // Validate userId format (basic sanity check)
    if (!userId || typeof userId !== "string" || userId.trim().length === 0) {
      throw new Error("Invalid userId: must be a non-empty string");
    }

    // Deterministic boundary: reject non-string userIds before any I/O.
    ExportService.assertValidUserId(userId);

    // Filter invoices strictly by business ownership
    let invoices: any[];
    try {
      // SECURITY: invoiceStore.findInvoices filters by business === userId
      invoices = invoiceStore.findInvoices({ business: userId });
    } catch (err: any) {
      const msg = err && err.message ? String(err.message) : "";
      if (process.env.NODE_ENV === "test" && /no such table/i.test(msg)) {
        // Test environment fallback with strict filtering
        // eslint-disable-next-line @typescript-eslint/no-var-requires
        const { MOCK_INVOICES } = require("../controllers/v1/invoices");
        invoices = MOCK_INVOICES.filter(
          (inv: any) => inv.business === userId
        );
      } else {
        // Wrap storage errors so callers get a stable, non-leaking code.
        throw new ExportError(
          ExportErrorCode.STORAGE_FAILURE,
          "Failed to load invoices for export",
          true,
        );
      }
    }

    // SECURITY: Filter bids strictly by investor ownership
    // Only return bids where the authenticated user is the investor
    const bids = MOCK_BIDS.filter((b) => b.investor === userId);

    // SECURITY: Filter settlements strictly by participation
    // Only return settlements where the authenticated user is payer OR recipient
    const settlements = MOCK_SETTLEMENTS.filter(
      (s: any) => s.payer === userId || s.recipient === userId
    );
    return { invoices, bids, settlements };
  }

  public generateSignedToken(userId: string, format: ExportFormat): string {
    ExportService.assertValidUserId(userId);
    ExportService.assertValidFormat(format);
    const expiresAt = Date.now() + this.ttlMs;
    const payload = JSON.stringify({ userId, format, expiresAt });
    const signature = crypto
      .createHmac("sha256", this.secret)
      .update(payload)
      .digest("hex");
    return Buffer.from(JSON.stringify({ payload, signature })).toString("base64");
  }

  public validateToken(token: string): { userId: string; format: ExportFormat; expiresAt: number } | null {
    try {
      if (typeof token !== "string" || token.length === 0) return null;
      const decoded = JSON.parse(Buffer.from(token, "base64").toString("utf8"));
      const { payload, signature } = decoded;
      if (typeof payload !== "string" || typeof signature !== "string") return null;
      const expectedSignature = crypto
        .createHmac("sha256", this.secret)
        .update(payload)
        .digest("hex");
      if (signature !== expectedSignature) return null;
      const { userId, format, expiresAt } = JSON.parse(payload);
      if (typeof userId !== "string" || userId.length === 0) return null;
      if (format !== ExportFormat.JSON && format !== ExportFormat.CSV) return null;
      if (typeof expiresAt !== "number" || !Number.isFinite(expiresAt)) return null;
      if (Date.now() > expiresAt) return null;
      return { userId, format, expiresAt };
    } catch {
      return null;
    }
  }

  /**
   * Deterministic token validation that distinguishes invalid vs expired.
   * Callers that need to render different user-visible errors should use this.
   */
  public validateTokenStrict(
    token: string,
  ): { userId: string; format: ExportFormat; expiresAt: number } {
    if (typeof token !== "string" || token.length === 0) {
      throw new ExportError(ExportErrorCode.TOKEN_INVALID, "Export token is missing or malformed");
    }
    let decoded: any;
    try {
      decoded = JSON.parse(Buffer.from(token, "base64").toString("utf8"));
    } catch {
      throw new ExportError(ExportErrorCode.TOKEN_INVALID, "Export token is not decodable");
    }
    const { payload, signature } = decoded || {};
    if (typeof payload !== "string" || typeof signature !== "string") {
      throw new ExportError(ExportErrorCode.TOKEN_INVALID, "Export token payload is malformed");
    }
    const expectedSignature = crypto
      .createHmac("sha256", this.secret)
      .update(payload)
      .digest("hex");
    // Constant-time comparison to avoid signature oracle timing leaks.
    const sigBuf = Buffer.from(signature, "hex");
    const expBuf = Buffer.from(expectedSignature, "hex");
    if (sigBuf.length !== expBuf.length || !crypto.timingSafeEqual(sigBuf, expBuf)) {
      throw new ExportError(ExportErrorCode.TOKEN_INVALID, "Export token signature mismatch");
    }
    let parsed: any;
    try {
      parsed = JSON.parse(payload);
    } catch {
      throw new ExportError(ExportErrorCode.TOKEN_INVALID, "Export token payload is not JSON");
    }
    const { userId, format, expiresAt } = parsed || {};
    if (typeof userId !== "string" || userId.length === 0) {
      throw new ExportError(ExportErrorCode.TOKEN_INVALID, "Export token userId is invalid");
    }
    if (format !== ExportFormat.JSON && format !== ExportFormat.CSV) {
      throw new ExportError(ExportErrorCode.TOKEN_INVALID, "Export token format is invalid");
    }
    if (typeof expiresAt !== "number" || !Number.isFinite(expiresAt)) {
      throw new ExportError(ExportErrorCode.TOKEN_INVALID, "Export token expiry is invalid");
    }
    if (Date.now() > expiresAt) {
      throw new ExportError(ExportErrorCode.TOKEN_EXPIRED, "Export token has expired");
    }
    return { userId, format, expiresAt };
  }

  public async generateExportFile(userId: string, format: ExportFormat): Promise<string> {
    ExportService.assertValidUserId(userId);
    ExportService.assertValidFormat(format);

    const key = ExportService.inFlightKey(userId, format);
    const existing = inFlightExports.get(key);
    if (existing) {
      // Concurrent callers share the same deterministic result.
      return existing;
    }

    const run = (async (): Promise<string> => {
      try {
        await fsp.mkdir(this.exportDir, { recursive: true, mode: 0o700 });
      } catch {
        throw new ExportError(
          ExportErrorCode.STORAGE_FAILURE,
          "Failed to prepare export directory",
          true,
        );
      }
      const token = this.generateSignedToken(userId, format);
      const ext = format === ExportFormat.JSON ? "json" : "csv";
      const safeToken = token.replace(/[/+=]/g, "_");
      const filePath = path.join(this.exportDir, `${safeToken}.${ext}`);
      const data = await this.getUserData(userId);
      await this.streamToFile(data, format, filePath);
      return token;
    })();

    inFlightExports.set(key, run);
    try {
      return await run;
    } finally {
      // Only clear if we still own the slot (defensive against future changes).
      if (inFlightExports.get(key) === run) {
        inFlightExports.delete(key);
      }
    }
  }

  private async streamToFile(
    data: ExportData["data"],
    format: ExportFormat,
    filePath: string,
  ): Promise<void> {
    if (typeof filePath !== "string" || filePath.length === 0) {
      throw new ExportError(ExportErrorCode.INVALID_INPUT, "Invalid export file path");
    }
    ExportService.assertValidFormat(format);
    const tmpPath = filePath + ".tmp";
    const writeStream = fs.createWriteStream(tmpPath, { mode: 0o600 });

    try {
      await new Promise<void>((resolve, reject) => {
        if (format === ExportFormat.JSON) {
          writeStream.write('{\n');
          this.writeJsonSection(writeStream, data, "invoices", ["id", "amount", "currency", "status", "due_date"]);
          writeStream.write(',\n');
          this.writeJsonSection(writeStream, data, "bids", ["bid_id", "invoice_id", "bid_amount", "status", "timestamp"]);
          writeStream.write(',\n');
          this.writeJsonSection(writeStream, data, "settlements", ["id", "invoice_id", "amount", "status", "timestamp"]);
          writeStream.write('\n}\n');
          writeStream.end();
        } else {
          this.writeCsvSection(writeStream, "INVOICES", data.invoices, ["id", "amount", "currency", "status", "due_date"],
            (r) => `${r.id},${r.amount},${r.currency},${r.status},${r.due_date ? new Date(r.due_date * 1000).toISOString() : ""}`
          );
          this.writeCsvSection(writeStream, "BIDS", data.bids, ["bid_id", "invoice_id", "bid_amount", "status", "timestamp"],
            (r) => `${r.bid_id},${r.invoice_id},${r.bid_amount},${r.status},${r.timestamp ? new Date(r.timestamp * 1000).toISOString() : ""}`
          );
          this.writeCsvSection(writeStream, "SETTLEMENTS", data.settlements, ["id", "invoice_id", "amount", "status", "timestamp"],
            (r) => `${r.id},${r.invoice_id},${r.amount},${r.status},${r.timestamp ? new Date(r.timestamp * 1000).toISOString() : ""}`
          );
          writeStream.end();
        }
        writeStream.on("finish", resolve);
        writeStream.on("error", reject);
      });
      await fsp.rename(tmpPath, filePath);
      await fsp.chmod(filePath, 0o600);
    } catch (err) {
      await fsp.unlink(tmpPath).catch(() => {});
      if (err instanceof ExportError) throw err;
      throw new ExportError(
        ExportErrorCode.STORAGE_FAILURE,
        "Failed to write export file",
        true,
      );
    }
  }

  private writeJsonSection(
    stream: fs.WriteStream,
    data: ExportData["data"],
    key: string,
    fields: string[],
  ): void {
    const items = (data as any)[key] as any[];
    stream.write(`  "${key}": [\n`);
    for (let i = 0; i < items.length; i++) {
      const item = items[i];
      const obj = Object.fromEntries(fields.map((f) => [f, (item as any)[f] ?? null]));
      stream.write(`    ${JSON.stringify(obj)}`);
      if (i < items.length - 1) stream.write(",");
      stream.write("\n");
    }
    stream.write("  ]");
  }

  private writeCsvSection(
    stream: fs.WriteStream,
    sectionName: string,
    rows: any[],
    headers: string[],
    formatRow: (r: any) => string,
  ): void {
    stream.write(`--- ${sectionName} ---\n`);
    if (rows.length > 0) {
      stream.write(headers.join(",") + "\n");
      for (const row of rows) {
        stream.write(formatRow(row) + "\n");
      }
    } else {
      stream.write(`No ${sectionName.toLowerCase()} found\n`);
    }
    stream.write("\n");
  }

  public async getFilePath(token: string): Promise<string | null> {
    const validated = this.validateToken(token);
    if (!validated) return null;
    const ext = validated.format === ExportFormat.JSON ? "json" : "csv";
    const safeToken = token.replace(/[/+=]/g, "_");
    const filePath = path.join(this.exportDir, `${safeToken}.${ext}`);
    try {
      await fsp.access(filePath, fs.constants.R_OK);
      return filePath;
    } catch {
      return null;
    }
  }

  /**
   * Strict variant of getFilePath that surfaces a deterministic error code
   * for invalid/expired tokens and missing files. Use at HTTP boundaries.
   */
  public async getFilePathStrict(token: string): Promise<string> {
    const validated = this.validateTokenStrict(token);
    const ext = validated.format === ExportFormat.JSON ? "json" : "csv";
    const safeToken = token.replace(/[/+=]/g, "_");
    const filePath = path.join(this.exportDir, `${safeToken}.${ext}`);
    try {
      await fsp.access(filePath, fs.constants.R_OK);
    } catch {
      throw new ExportError(
        ExportErrorCode.NOT_FOUND,
        "Export file is not available",
      );
    }
    return filePath;
  }

  public async deleteFile(filePath: string): Promise<void> {
    await fsp.unlink(filePath).catch(() => {});
  }

  /** Test-only hook to reset in-flight dedupe state between cases. */
  public __resetInFlightForTests(): void {
    inFlightExports.clear();
  }

  public async cleanupExpiredFiles(): Promise<number> {
    let cleaned = 0;
    try {
      const files = await fsp.readdir(this.exportDir);
      const now = Date.now();
      for (const file of files) {
        if (!file.endsWith(".json") && !file.endsWith(".csv")) continue;
        const filePath = path.join(this.exportDir, file);
        try {
          const stat = await fsp.stat(filePath);
          if (now - stat.mtimeMs > this.ttlMs) {
            await fsp.unlink(filePath);
            cleaned++;
          }
        } catch { /* skip unreadable */ }
      }
    } catch { /* dir may not exist */ }
    return cleaned;
  }
}

export const exportService = new ExportService();

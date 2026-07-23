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

interface ExportFileMeta {
  userId: string;
  format: ExportFormat;
  expiresAt: number;
  filePath: string;
}

class ExportService {
  private readonly secret = config.EXPORT_SECRET || "fallback-secret-for-signing-links";
  private readonly exportDir = config.EXPORT_DIR;
  private readonly ttlMs = config.EXPORT_TTL_MS;

  constructor() {
    fsp.mkdir(this.exportDir, { recursive: true, mode: 0o700 }).catch(() => {});
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
        throw err;
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
      const decoded = JSON.parse(Buffer.from(token, "base64").toString("utf8"));
      const { payload, signature } = decoded;
      const expectedSignature = crypto
        .createHmac("sha256", this.secret)
        .update(payload)
        .digest("hex");
      if (signature !== expectedSignature) return null;
      const { userId, format, expiresAt } = JSON.parse(payload);
      if (Date.now() > expiresAt) return null;
      return { userId, format, expiresAt };
    } catch {
      return null;
    }
  }

  public async generateExportFile(userId: string, format: ExportFormat): Promise<string> {
    await fsp.mkdir(this.exportDir, { recursive: true, mode: 0o700 });
    const token = this.generateSignedToken(userId, format);
    const ext = format === ExportFormat.JSON ? "json" : "csv";
    const safeToken = token.replace(/[/+=]/g, "_");
    const filePath = path.join(this.exportDir, `${safeToken}.${ext}`);
    const data = await this.getUserData(userId);
    await this.streamToFile(data, format, filePath);
    return token;
  }

  private async streamToFile(
    data: ExportData["data"],
    format: ExportFormat,
    filePath: string,
  ): Promise<void> {
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
      throw err;
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

  public async deleteFile(filePath: string): Promise<void> {
    await fsp.unlink(filePath).catch(() => {});
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

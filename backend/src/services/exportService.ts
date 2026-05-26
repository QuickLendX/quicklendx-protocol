import { MOCK_INVOICES } from "../controllers/v1/invoices";
import { MOCK_BIDS } from "../controllers/v1/bids";
import { MOCK_SETTLEMENTS } from "../controllers/v1/settlements";
import { config } from "../config";
import crypto from "crypto";

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

class ExportService {
  private readonly secret = config.EXPORT_SECRET || "fallback-secret-for-signing-links";

  /**
   * Fetches all data related to a user.
   */
  public async getUserData(userId: string): Promise<ExportData["data"]> {
    // Filter mock data for the user
    // A user can be a business (invoices) or an investor (bids) or a payer/recipient (settlements)
    const invoices = MOCK_INVOICES.filter((i) => i.business === userId);
    const bids = MOCK_BIDS.filter((b) => b.investor === userId);
    const settlements = MOCK_SETTLEMENTS.filter(
      (s) => s.payer === userId || s.recipient === userId
    );

    return { invoices, bids, settlements };
  }

  /**
   * Generates a signed token for a download link.
   * Token includes userId, format, and an expiration timestamp.
   */
  public generateSignedToken(userId: string, format: ExportFormat): string {
    const expiresAt = Date.now() + 3600 * 1000; // 1 hour expiration
    const payload = JSON.stringify({ userId, format, expiresAt });
    
    const signature = crypto
      .createHmac("sha256", this.secret)
      .update(payload)
      .digest("hex");

    return Buffer.from(JSON.stringify({ payload, signature })).toString("base64");
  }

  /**
   * Validates a signed token and returns the payload if valid.
   */
  public validateToken(token: string): { userId: string; format: ExportFormat } | null {
    try {
      const decoded = JSON.parse(Buffer.from(token, "base64").toString("utf8"));
      const { payload, signature } = decoded;

      const expectedSignature = crypto
        .createHmac("sha256", this.secret)
        .update(payload)
        .digest("hex");

      if (signature !== expectedSignature) {
        return null;
      }

      const { userId, format, expiresAt } = JSON.parse(payload);
      if (Date.now() > expiresAt) {
        return null;
      }

      return { userId, format };
    } catch (error) {
      return null;
    }
  }

  /**
   * Converts data to the requested format.
   */
  public formatData(data: ExportData["data"], format: ExportFormat): string {
    if (format === ExportFormat.JSON) {
      return JSON.stringify(data, null, 2);
    }

    // Simple CSV generation
    let csv = "";
    
    // Invoices section
    csv += "--- INVOICES ---\n";
    if (data.invoices.length > 0) {
      csv += "ID,Amount,Currency,Status,Due Date\n";
      data.invoices.forEach((i) => {
        csv += `${i.id},${i.amount},${i.currency},${i.status},${new Date(i.due_date * 1000).toISOString()}\n`;
      });
    } else {
      csv += "No invoices found\n";
    }

    csv += "\n--- BIDS ---\n";
    if (data.bids.length > 0) {
      csv += "Bid ID,Invoice ID,Amount,Status,Timestamp\n";
      data.bids.forEach((b) => {
        csv += `${b.bid_id},${b.invoice_id},${b.bid_amount},${b.status},${new Date(b.timestamp * 1000).toISOString()}\n`;
      });
    } else {
      csv += "No bids found\n";
    }

    csv += "\n--- SETTLEMENTS ---\n";
    if (data.settlements.length > 0) {
      csv += "ID,Invoice ID,Amount,Status,Timestamp\n";
      data.settlements.forEach((s) => {
        csv += `${s.id},${s.invoice_id},${s.amount},${s.status},${new Date(s.timestamp * 1000).toISOString()}\n`;
      });
    } else {
      csv += "No settlements found\n";
    }

    return csv;
  }
}

export const exportService = new ExportService();

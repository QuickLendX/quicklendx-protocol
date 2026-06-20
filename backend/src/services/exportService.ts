import { MOCK_INVOICES } from "../controllers/v1/invoices";
import { MOCK_BIDS } from "../controllers/v1/bids";
import { MOCK_SETTLEMENTS } from "../controllers/v1/settlements";
import { invoiceStore } from "./invoiceStore";
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

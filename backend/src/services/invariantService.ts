import { z } from "zod";
import { Invoice } from "../types/contract";
import { Bid } from "../types/contract";
import { Settlement } from "../types/contract";
import { Dispute } from "../types/contract";
import { MOCK_INVOICES } from "../controllers/v1/invoices";
import { MOCK_BIDS } from "../controllers/v1/bids";
import { MOCK_SETTLEMENTS } from "../controllers/v1/settlements";
import { MOCK_DISPUTES } from "../controllers/v1/disputes";

const MAX_SAMPLE_IDS = 5;

export const InvariantCounterSchema = z.object({
  count: z.number().int().min(0),
  sampleIds: z.array(z.string()),
});

export const InvariantReportSchema = z.object({
  orphanBids: InvariantCounterSchema,
  orphanSettlements: InvariantCounterSchema,
  orphanDisputes: InvariantCounterSchema,
  mismatchSettlements: InvariantCounterSchema,
  timestamp: z.string().datetime(),
});

export type InvariantCounter = z.infer<typeof InvariantCounterSchema>;
export type InvariantReport = z.infer<typeof InvariantReportSchema>;

interface HasInvoiceId {
  invoice_id: string;
}

function scanOrphans<T extends HasInvoiceId>(
  items: T[],
  validInvoiceIds: Set<string>
): InvariantCounter {
  const orphans = items.filter(
    (item: T) => !validInvoiceIds.has(item.invoice_id)
  );
  return {
    count: orphans.length,
    sampleIds: orphans.slice(0, MAX_SAMPLE_IDS).map((o: T) => o.invoice_id),
  };
}

function scanMismatchSettlements(): InvariantCounter {
  const validBidInvoiceIds = new Set<string>(
    MOCK_BIDS.map((b: Bid) => b.invoice_id)
  );
  const mismatches = MOCK_SETTLEMENTS.filter(
    (s: Settlement) => !validBidInvoiceIds.has(s.invoice_id)
  );
  return {
    count: mismatches.length,
    sampleIds: mismatches
      .slice(0, MAX_SAMPLE_IDS)
      .map((s: Settlement) => s.id),
  };
}

export function getInvariantCounters(): InvariantReport {
  const validInvoiceIds = new Set<string>(
    MOCK_INVOICES.map((i: Invoice) => i.id)
  );

  return {
    orphanBids: scanOrphans<HasInvoiceId>(MOCK_BIDS as HasInvoiceId[], validInvoiceIds),
    orphanSettlements: scanOrphans<HasInvoiceId>(MOCK_SETTLEMENTS as HasInvoiceId[], validInvoiceIds),
    orphanDisputes: scanOrphans<HasInvoiceId>(MOCK_DISPUTES as HasInvoiceId[], validInvoiceIds),
    mismatchSettlements: scanMismatchSettlements(),
    timestamp: new Date().toISOString(),
  };
}
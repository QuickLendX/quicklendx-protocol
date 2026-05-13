// @ts-nocheck
/**
 * Deterministic dataset generator for performance tests.
 * Uses a fixed seed so results are reproducible across runs.
 */
import {
  Invoice,
  InvoiceStatus,
  InvoiceCategory,
  Bid,
  BidStatus,
  Settlement,
  SettlementStatus,
} from "../../types/contract";

const BASE_TS = 1_700_000_000; // fixed epoch, not Date.now()

function padHex(n: number): string {
  return n.toString(16).padStart(64, "0");
}

export function seedInvoices(count = 100): Invoice[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `0x${padHex(i + 1)}`,
    business: `G${String(i % 10).padStart(55, "A")}`,
    amount: String((i + 1) * 1_000_000),
    currency: "CBGHS...ABC",
    due_date: BASE_TS + 86400 * 30,
    status: Object.values(InvoiceStatus)[i % 6] as InvoiceStatus,
    description: `Invoice #${i + 1}`,
    category: Object.values(InvoiceCategory)[i % 7] as InvoiceCategory,
    tags: ["perf-test"],
    metadata: {
      customer_name: `Customer ${i + 1}`,
      customer_address: `${i + 1} Test St`,
      tax_id: `TX-${i + 1}`,
      line_items: [
        {
          description: "Service",
          quantity: "1",
          unit_price: String((i + 1) * 1_000_000),
          total: String((i + 1) * 1_000_000),
        },
      ],
      notes: "",
    },
    created_at: BASE_TS,
    updated_at: BASE_TS,
  }));
}

export function seedBids(invoices: Invoice[], bidsPerInvoice = 5): Bid[] {
  const bids: Bid[] = [];
  for (const inv of invoices) {
    for (let j = 0; j < bidsPerInvoice; j++) {
      bids.push({
        bid_id: `0x${padHex(bids.length + 1)}`,
        invoice_id: inv.id,
        investor: `GA${String(j).padStart(54, "B")}`,
        bid_amount: String(Number(inv.amount) * 0.95),
        expected_return: String(Number(inv.amount) * 0.05),
        timestamp: BASE_TS + j * 60,
        status: BidStatus.Placed,
        expiration_timestamp: BASE_TS + 86400,
      });
    }
  }
  return bids;
}

export function seedSettlements(invoices: Invoice[]): Settlement[] {
  return invoices.slice(0, 50).map((inv, i) => ({
    id: `0xsettle${padHex(i + 1)}`,
    invoice_id: inv.id,
    amount: inv.amount,
    payer: `GA${String(i).padStart(54, "P")}`,
    recipient: `GB${String(i).padStart(54, "R")}`,
    timestamp: BASE_TS + 3600,
    status: SettlementStatus.Paid,
  }));
}

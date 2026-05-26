// @ts-nocheck
import { Invoice, InvoiceStatus, InvoiceCategory } from "../types/contract";

export class MockDataProviders {
  // Canonical on-chain state
  static getOnChainInvoices(): Invoice[] {
    return [
      {
        id: "invoice_1",
        business: "BUS_1",
        amount: "1000",
        currency: "USDC",
        due_date: 1713974400,
        status: InvoiceStatus.Verified,
        description: "Valid Invoice",
        category: InvoiceCategory.Technology,
        tags: [],
        metadata: { customer_name: "C1", customer_address: "A1", tax_id: "T1", line_items: [], notes: "" },
        created_at: 1713888000,
        updated_at: 1713888000,
      },
      {
        id: "invoice_2",
        business: "BUS_2",
        amount: "2000",
        currency: "USDC",
        due_date: 1713974400,
        status: InvoiceStatus.Funded,
        description: "Funded Invoice",
        category: InvoiceCategory.Services,
        tags: [],
        metadata: { customer_name: "C2", customer_address: "A2", tax_id: "T2", line_items: [], notes: "" },
        created_at: 1713888000,
        updated_at: 1713888000,
      },
      {
        id: "invoice_3",
        business: "BUS_3",
        amount: "3000",
        currency: "USDC",
        due_date: 1713974400,
        status: InvoiceStatus.Paid,
        description: "Paid Invoice",
        category: InvoiceCategory.Consulting,
        tags: [],
        metadata: { customer_name: "C3", customer_address: "A3", tax_id: "T3", line_items: [], notes: "" },
        created_at: 1713888000,
        updated_at: 1713888000,
      },
    ];
  }

  // Simulated indexed state with drift
  static getIndexedInvoices(): Invoice[] {
    return [
      {
        id: "invoice_1",
        business: "BUS_1",
        amount: "1000",
        currency: "USDC",
        due_date: 1713974400,
        status: InvoiceStatus.Pending, // DRIFT: Status mismatch (On-chain is Verified)
        description: "Valid Invoice",
        category: InvoiceCategory.Technology,
        tags: [],
        metadata: { customer_name: "C1", customer_address: "A1", tax_id: "T1", line_items: [], notes: "" },
        created_at: 1713888000,
        updated_at: 1713888000,
      },
      {
        id: "invoice_3",
        business: "BUS_3",
        amount: "3000",
        currency: "USDC",
        due_date: 1713974400,
        status: InvoiceStatus.Paid, // NO DRIFT
        description: "Paid Invoice",
        category: InvoiceCategory.Consulting,
        tags: [],
        metadata: { customer_name: "C3", customer_address: "A3", tax_id: "T3", line_items: [], notes: "" },
        created_at: 1713888000,
        updated_at: 1713888000,
      },
      // DRIFT: invoice_2 is missing from indexer
    ];
  }
}

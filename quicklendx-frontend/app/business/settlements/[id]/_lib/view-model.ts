import { Settlement, Invoice, SettlementStatus } from "./data";

export interface ReceiptViewModel {
  id: string;
  settlementDate: string;
  rawTimestamp: number;
  status: SettlementStatus;
  statusLabel: string;
  settledAmount: string;
  payer: string;
  recipient: string;
  invoice: {
    id: string;
    amount: string;
    currency: string;
    dueDate: string;
    rawDueDate: number;
    category?: string;
    description: string;
    business: string;
  };
}

export interface JsonExportReceipt {
  receipt: {
    id: string;
    settlement_date: string;
    status: SettlementStatus;
    invoice: {
      id: string;
      amount: string;
      due_date: string;
      description: string;
    };
  };
}

export function formatTimestamp(timestamp: number): string {
  // If timestamp is in seconds (< 10_000_000_000), convert to ms
  const ms = timestamp < 10_000_000_000 ? timestamp * 1000 : timestamp;
  const date = new Date(ms);
  return date.toISOString();
}

export function getStatusLabel(status: SettlementStatus): string {
  switch (status) {
    case "Paid":
      return "✓ Settled";
    case "Pending":
      return "⏳ Pending";
    case "Defaulted":
      return "⚠ Defaulted";
  }
}

export function toReceiptViewModel(
  settlement: Settlement,
  invoice: Invoice
): ReceiptViewModel {
  return {
    id: settlement.id,
    settlementDate: formatTimestamp(settlement.timestamp),
    rawTimestamp: settlement.timestamp,
    status: settlement.status,
    statusLabel: getStatusLabel(settlement.status),
    settledAmount: settlement.amount,
    payer: settlement.payer,
    recipient: settlement.recipient,
    invoice: {
      id: invoice.id,
      amount: invoice.amount,
      currency: invoice.currency,
      dueDate: formatTimestamp(invoice.due_date),
      rawDueDate: invoice.due_date,
      category: invoice.category,
      description: invoice.description,
      business: invoice.business,
    },
  };
}

export function toJsonExport(vm: ReceiptViewModel): JsonExportReceipt {
  return {
    receipt: {
      id: vm.id,
      settlement_date: vm.settlementDate,
      status: vm.status,
      invoice: {
        id: vm.invoice.id,
        amount: vm.invoice.amount,
        due_date: vm.invoice.dueDate,
        description: vm.invoice.description,
      },
    },
  };
}

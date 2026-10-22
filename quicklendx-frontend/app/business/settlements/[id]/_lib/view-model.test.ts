import { toReceiptViewModel, toJsonExport } from "./view-model";
import { Settlement, Invoice } from "./data";

describe("view-model", () => {
  const fixtureSettlement: Settlement = {
    id: "0xsettle123",
    invoice_id: "0xinv8201",
    amount: "50000000000",
    payer: "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ",
    recipient: "GBSXVD727UNXJZ7ZM4VCXBTK3UMPXR7O6LLXS7XVOECGDYH3XFNV7C5K",
    timestamp: 1777390800,
    status: "Paid",
  };

  const fixtureInvoice: Invoice = {
    id: "0xinv8201",
    business: "GBSXVD727UNXJZ7ZM4VCXBTK3UMPXR7O6LLXS7XVOECGDYH3XFNV7C5K",
    amount: "50000000000",
    currency: "USDC",
    due_date: 1778860800,
    status: "Paid",
    description: "Professional Services, April 2026",
    category: "Services",
    created_at: 1776000000,
    updated_at: 1777390800,
  };

  it("maps settlement and invoice to view model with exact fields and statusLabel", () => {
    const vm = toReceiptViewModel(fixtureSettlement, fixtureInvoice);

    expect(vm.id).toBe("0xsettle123");
    expect(vm.status).toBe("Paid");
    expect(vm.statusLabel).toBe("✓ Settled");
    expect(vm.settledAmount).toBe("50000000000");
    expect(vm.invoice.amount).toBe("50000000000");
    expect(vm.invoice.id).toBe("0xinv8201");
    expect(vm.invoice.description).toBe("Professional Services, April 2026");

    const vmKeys = Object.keys(vm);
    expect(vmKeys).not.toContain("debtor");
    expect(vmKeys).not.toContain("accountLast4");
    expect(vmKeys).not.toContain("investorCount");
    expect(vmKeys).not.toContain("fees");

    const invoiceKeys = Object.keys(vm.invoice);
    expect(invoiceKeys).not.toContain("debtor");
  });

  it("maps Pending and Defaulted status labels correctly", () => {
    const pendingVm = toReceiptViewModel(
      { ...fixtureSettlement, status: "Pending" },
      fixtureInvoice
    );
    expect(pendingVm.statusLabel).toBe("⏳ Pending");

    const defaultedVm = toReceiptViewModel(
      { ...fixtureSettlement, status: "Defaulted" },
      fixtureInvoice
    );
    expect(defaultedVm.statusLabel).toBe("⚠ Defaulted");
  });

  it("toJsonExport produces clean JSON schema with no debtor, account_last4 or fees", () => {
    const vm = toReceiptViewModel(fixtureSettlement, fixtureInvoice);
    const json = toJsonExport(vm);

    expect(json.receipt.id).toBe("0xsettle123");
    expect(json.receipt.status).toBe("Paid");
    expect(json.receipt.invoice.id).toBe("0xinv8201");
    expect(json.receipt.invoice.amount).toBe("50000000000");

    const receiptKeys = Object.keys(json.receipt);
    expect(receiptKeys).not.toContain("debtor");
    expect(receiptKeys).not.toContain("account_last4");
    expect(receiptKeys).not.toContain("fees");
    expect(receiptKeys).not.toContain("funding");
    expect(receiptKeys).not.toContain("calculation");
    expect(receiptKeys).not.toContain("transfer");

    const invoiceKeys = Object.keys(json.receipt.invoice);
    expect(invoiceKeys).not.toContain("debtor");
  });
});

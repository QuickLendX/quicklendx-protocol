import React from "react";
import { render, screen } from "@testing-library/react";
import { InvoiceCard, StatusIcon } from "../InvoiceCard";
import {
  INVOICE_STATUSES,
  INVOICE_STATUS_MAP,
  type InvoiceStatus,
} from "../invoice-status";
import { toPublicInvoice } from "../../invest/_lib/view-model";

function makeInvoice(overrides: Record<string, unknown> = {}) {
  return toPublicInvoice({
    id: "0xabc123",
    business: "GBSXVD727UNXJZ7ZM4VCXBTK3UMPXR7O6LLXS7XVOECGDYH3XFNV7C5K",
    amount: "50000000000",
    currency: "USDC",
    due_date: 1778803200,
    status: "Verified",
    description: "Freight shipment invoice #42",
    category: "Logistics",
    tags: ["freight"],
    created_at: 1776211200,
    updated_at: 1776297600,
    ...overrides,
  });
}

describe("InvoiceCard", () => {
  it("renders default variant with description, p-5 padding, and 7-decimal formatted amount", () => {
    const invoice = makeInvoice();
    render(<InvoiceCard invoice={invoice} />);

    const card = screen.getByTestId("invoice-card-0xabc123");
    expect(card).toHaveClass("p-5");
    expect(card).not.toHaveClass("p-3");
    expect(screen.getByText("5000.0000000")).toBeInTheDocument();
    expect(
      screen.getByText("Freight shipment invoice #42")
    ).toBeInTheDocument();
    expect(
      screen.getByText(INVOICE_STATUS_MAP.Verified.label)
    ).toBeInTheDocument();
  });

  it("renders compact variant with smaller padding and hides description", () => {
    const invoice = makeInvoice();
    render(<InvoiceCard invoice={invoice} variant="compact" />);

    const card = screen.getByTestId("invoice-card-0xabc123");
    expect(card).toHaveClass("p-3");
    expect(card).not.toHaveClass("p-5");
    expect(
      screen.queryByText("Freight shipment invoice #42")
    ).not.toBeInTheDocument();
    expect(screen.getByText("5000.0000000")).toBeInTheDocument();
  });

  it("threads custom decimals into formatAtomicAmount without precision loss", () => {
    const invoice = makeInvoice({ amount: "123456789012345678901" });
    render(<InvoiceCard invoice={invoice} decimals={2} />);

    expect(screen.getByText("1234567890123456789.01")).toBeInTheDocument();
  });

  it("renders bestBidSlot when provided", () => {
    const invoice = makeInvoice();
    render(
      <InvoiceCard
        invoice={invoice}
        bestBidSlot={<span>Best bid: 4,900 USDC</span>}
      />
    );

    expect(screen.getByText("Best bid: 4,900 USDC")).toBeInTheDocument();
  });
});

describe("StatusIcon", () => {
  it.each(INVOICE_STATUSES)(
    "renders an aria-hidden svg icon for %s",
    (status: InvoiceStatus) => {
      const { container } = render(<StatusIcon status={status} size={18} />);
      const svg = container.querySelector("svg");
      expect(svg).not.toBeNull();
      expect(svg).toHaveAttribute("aria-hidden", "true");
      expect(svg).toHaveAttribute("width", "18");
      expect(svg).toHaveAttribute("height", "18");
    }
  );
});

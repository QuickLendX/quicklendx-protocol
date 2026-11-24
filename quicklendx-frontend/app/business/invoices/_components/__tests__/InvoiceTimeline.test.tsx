import React from "react";
import { render, screen, within } from "@testing-library/react";
import "@testing-library/jest-dom";
import { InvoiceTimeline } from "../InvoiceTimeline";
import type { Dispute, Invoice, Settlement } from "../../_lib/data";

const invoice: Invoice = {
  id: "inv-1",
  business: "bus-1",
  amount: "100000000",
  currency: "USDC",
  due_date: 1790000000,
  status: "Verified",
  description: "Test invoice",
  category: "Services",
  tags: [],
  created_at: 1704067200,
  updated_at: 1704067200,
};

describe("InvoiceTimeline", () => {
  it("renders timeline steps sorted by timestamp and formats UTC dates", () => {
    render(
      <InvoiceTimeline
        invoice={invoice}
        disputes={[{
          id: "dispute-1",
          invoice_id: "inv-1",
          initiator: "business-1",
          reason: "Incorrect amount",
          status: "Resolved",
          created_at: 1704167200,
          resolved_at: 1704267200,
        } as Dispute]}
        settlements={[{
          id: "settlement-1",
          invoice_id: "inv-1",
          amount: "100000000",
          payer: "payer-1",
          recipient: "recipient-1",
          timestamp: 1704367200,
          status: "Paid",
        } as Settlement]}
      />
    );

    const timeline = screen.getByRole("list");
    const items = within(timeline).getAllByRole("listitem");
    expect(items).toHaveLength(4);
    expect(within(items[0]).getByText("Invoice created")).toBeInTheDocument();
    expect(within(items[0]).getByText(/Jan 1, 2024/)).toBeInTheDocument();
    expect(within(items[1]).getByText("Dispute opened (Resolved)")).toBeInTheDocument();
    expect(within(items[2]).getByText("Dispute resolved")).toBeInTheDocument();
    expect(within(items[3]).getByText("Settlement (Paid)")).toBeInTheDocument();
  });

  it("renders only the creation step when no updates or related events exist", () => {
    render(<InvoiceTimeline invoice={invoice} />);

    expect(screen.getAllByRole("listitem")).toHaveLength(1);
    expect(screen.getByText("Invoice created")).toBeInTheDocument();
  });
});

import React from "react";
import { render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom";
import InvoiceDetailPage from "../page";
import { InvoiceDetailView } from "../../../_components/InvoiceDetailView";
import * as endpoints from "../../../../lib/api/endpoints";

jest.mock("../../../../lib/api/endpoints");

describe("/invest/invoices/[id] Detail Page", () => {
  const mockedInvoice = {
    id: "0xdetail123456",
    business: "GXXXXX",
    amount: "15000000000",
    currency: "USDC",
    due_date: 1780000000,
    status: "Verified" as const,
    description: "Detailed description of computer hardware invoice",
    category: "Hardware",
    tags: ["tech", "procurement"],
    metadata: {
      customer_name: "Acme Debtor LLC",
      customer_address: "123 Debtor Ave",
      tax_id: "TAX-123456",
    },
    created_at: 1770000000,
    updated_at: 1770000000,
  };

  const mockedBids = [
    {
      bid_id: "bid-1",
      invoice_id: "0xdetail123456",
      investor: "GBSXABCDEFGH12345678907C5K",
      bid_amount: "14500000000",
      expected_return: "500000000",
      timestamp: 1770000000,
      status: "Placed" as const,
      expiration_timestamp: 1780000000,
    },
  ];

  beforeEach(() => {
    jest.clearAllMocks();
    (endpoints.fetchInvoiceById as jest.Mock).mockResolvedValue(mockedInvoice);
    (endpoints.fetchRankedBids as jest.Mock).mockResolvedValue(mockedBids);
  });

  it("renders ranked bids in API order with truncated bidder addresses, and no debtor metadata in DOM", async () => {
    render(
      <InvoiceDetailView
        params={Promise.resolve({ id: "0xdetail123456" })}
        bidFlowEnabled={false}
      />
    );

    await waitFor(() => {
      expect(screen.getByText("0xdetail123456")).toBeInTheDocument();
    });

    // Check bids rendered with truncated address
    expect(screen.getByText("GBSX…7C5K")).toBeInTheDocument();
    expect(
      screen.queryByText("GBSXABCDEFGH12345678907C5K")
    ).not.toBeInTheDocument();

    // Check metadata fields NOT rendered in document.body
    expect(document.body.textContent).not.toContain("Acme Debtor LLC");
    expect(document.body.textContent).not.toContain("TAX-123456");
    expect(document.body.textContent).not.toContain("123 Debtor Ave");
  });

  it("'Place a bid' button is absent by default and present with bidFlowEnabled=true", async () => {
    // 1. Default (absent)
    const { unmount } = render(
      <InvoiceDetailPage params={Promise.resolve({ id: "0xdetail123456" })} />
    );

    await waitFor(() => {
      expect(screen.getByText("0xdetail123456")).toBeInTheDocument();
    });
    expect(
      screen.queryByRole("button", { name: /place a bid/i })
    ).not.toBeInTheDocument();

    unmount();

    // 2. bidFlowEnabled=true (present)
    render(
      <InvoiceDetailView
        params={Promise.resolve({ id: "0xdetail123456" })}
        bidFlowEnabled={true}
      />
    );

    await waitFor(() => {
      expect(screen.getByText("0xdetail123456")).toBeInTheDocument();
    });
    expect(
      screen.getByRole("button", { name: /place a bid/i })
    ).toBeInTheDocument();
  });
});

import React from "react";
import { render, screen } from "@testing-library/react";
import "@testing-library/jest-dom";
import { RankedBidsTable } from "../RankedBidsTable";
import { Bid } from "../../_lib/view-model";

describe("RankedBidsTable", () => {
  const sampleBids: Bid[] = [
    {
      bid_id: "bid-1",
      invoice_id: "inv-1",
      investor: "GBSXABCDEFGH12345678907C5K",
      bid_amount: "48000000000",
      expected_return: "2000000000",
      timestamp: 1770000000,
      status: "Placed",
      expiration_timestamp: 1780000000,
    },
    {
      bid_id: "bid-2",
      invoice_id: "inv-1",
      investor: "GAXMYZ1234567890ABCDEFGH7C5K",
      bid_amount: "47500000000",
      expected_return: "2500000000",
      timestamp: 1770001000,
      status: "Placed",
      expiration_timestamp: 1780000000,
    },
  ];

  it("renders ranked bids in API order with truncated bidder addresses", () => {
    render(<RankedBidsTable bids={sampleBids} />);

    expect(screen.getByText("#1 Best")).toBeInTheDocument();
    expect(screen.getByText("#2")).toBeInTheDocument();

    // Truncated addresses
    expect(screen.getByText("GBSX…7C5K")).toBeInTheDocument();
    expect(screen.getByText("GAXM…7C5K")).toBeInTheDocument();

    // Full addresses must not appear
    expect(
      screen.queryByText("GBSXABCDEFGH12345678907C5K")
    ).not.toBeInTheDocument();
  });

  it("renders empty state when bids list is empty", () => {
    render(<RankedBidsTable bids={[]} />);
    expect(
      screen.getByText("No bids placed yet for this invoice.")
    ).toBeInTheDocument();
  });
});

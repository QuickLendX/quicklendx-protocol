import React from "react";
import { render, screen, within } from "@testing-library/react";
import "@testing-library/jest-dom";
import { BidsTable } from "../BidsTable";
import type { Bid } from "../../_lib/data";

const bid = (overrides: Partial<Bid>): Bid => ({
  bid_id: "bid-1",
  invoice_id: "inv-1",
  investor: "GInvestorAddress",
  bid_amount: "100000000",
  expected_return: "110000000",
  timestamp: 1700000000,
  status: "Placed",
  expiration_timestamp: 1780000000,
  ...overrides,
});

describe("BidsTable", () => {
  it("renders ranked rows in API order with exact atomic amounts", () => {
    render(
      <BidsTable
        bids={[
          bid({ bid_id: "bid-first", bid_amount: "48000000000", expected_return: "50400000000" }),
          bid({ bid_id: "bid-second", bid_amount: "47500000000", expected_return: "50100000000" }),
        ]}
        currency="USDC"
        decimals={7}
      />
    );

    expect(screen.getByRole("region", { name: "Ranked bids table container" })).toBeInTheDocument();
    expect(screen.getByRole("table", { name: "Ranked bids" })).toBeInTheDocument();

    const rows = within(screen.getByRole("table", { name: "Ranked bids" })).getAllByRole("row");
    expect(within(rows[1]).getByText("#1")).toBeInTheDocument();
    expect(within(rows[1]).getByText("4800.0000000")).toBeInTheDocument();
    expect(within(rows[1]).getByText("5040.0000000")).toBeInTheDocument();
    expect(within(rows[1]).getByText("5.00%")).toBeInTheDocument();

    expect(within(rows[2]).getByText("#2")).toBeInTheDocument();
    expect(within(rows[2]).getByText("4750.0000000")).toBeInTheDocument();
  });

  it("renders an accessible empty state", () => {
    render(<BidsTable bids={[]} />);
    expect(screen.getByText("No ranked bids placed on this invoice yet.")).toBeInTheDocument();
  });
});

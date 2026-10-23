import React from "react";
import { render, screen } from "@testing-library/react";
import "@testing-library/jest-dom";
import { FreshnessBanner, isFreshEnoughForAction } from "../FreshnessBanner";

describe("FreshnessBanner Component and isFreshEnoughForAction", () => {
  it("lag 10 gives no role=status/alert and shows 'Data as of'", () => {
    const freshness = {
      lastIndexedLedger: 100,
      indexLagSeconds: 10,
      lastUpdatedAt: "2026-03-15T14:30:00Z",
      cursor: "c1",
    };

    render(<FreshnessBanner freshness={freshness} />);

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByText(/Data as of 14:30 UTC/i)).toBeInTheDocument();
  });

  it("lag 45 gives role=status containing '45'", () => {
    const freshness = {
      lastIndexedLedger: 100,
      indexLagSeconds: 45,
      lastUpdatedAt: "2026-03-15T14:30:00Z",
      cursor: "c1",
    };

    render(<FreshnessBanner freshness={freshness} />);

    const statusBanner = screen.getByRole("status");
    expect(statusBanner).toBeInTheDocument();
    expect(statusBanner).toHaveTextContent("45");
    expect(statusBanner).toHaveTextContent(
      "Data may be up to 45 s behind the network"
    );
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("lag 150 gives role=alert", () => {
    const freshness = {
      lastIndexedLedger: 100,
      indexLagSeconds: 150,
      lastUpdatedAt: "2026-03-15T14:30:00Z",
      cursor: "c1",
    };

    render(<FreshnessBanner freshness={freshness} />);

    const alertBanner = screen.getByRole("alert");
    expect(alertBanner).toBeInTheDocument();
    expect(alertBanner).toHaveTextContent(
      "Data is significantly delayed. Bidding is paused until it catches up."
    );
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("isFreshEnoughForAction returns true for 120 and false for 121", () => {
    expect(
      isFreshEnoughForAction({
        lastIndexedLedger: 100,
        indexLagSeconds: 120,
        lastUpdatedAt: "2026-03-15T14:30:00Z",
        cursor: "c1",
      })
    ).toBe(true);

    expect(
      isFreshEnoughForAction({
        lastIndexedLedger: 100,
        indexLagSeconds: 121,
        lastUpdatedAt: "2026-03-15T14:30:00Z",
        cursor: "c1",
      })
    ).toBe(false);

    expect(isFreshEnoughForAction(null)).toBe(true);
    expect(isFreshEnoughForAction(undefined)).toBe(true);
  });
});

import React from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import "@testing-library/jest-dom";
import PortfolioPage from "../page";
import * as endpoints from "../../lib/api/endpoints";
import { useWallet } from "../../lib/wallet/WalletProvider";

jest.mock("../../lib/api/endpoints");
jest.mock("../../lib/wallet/WalletProvider", () => ({
  useWallet: jest.fn(),
}));

const INVESTOR = "GINVESTOR7XQ2ZK4ABCDEFGHIJKLMNOPQRSTUVWXYZ2345678";

const entryActive = {
  id: "0xpos_1",
  investor: INVESTOR,
  invoice_id: "0xabc123",
  invested_amount: "950000000", // 95.0000000
  expected_return: "50000000", // 5.0000000
  status: "Active" as const,
  invested_at: 1770000000,
};

const entryLarge = {
  id: "0xpos_2",
  investor: INVESTOR,
  invoice_id: "0xdef456",
  invested_amount: "123456789012345678901234567890",
  expected_return: "0",
  status: "Completed" as const,
  invested_at: 1769000000,
};

function connectWallet() {
  (useWallet as jest.Mock).mockReturnValue({
    state: "CONNECTED",
    address: INVESTOR,
  });
}

describe("/portfolio page", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    connectWallet();
    (endpoints.listPortfolio as jest.Mock).mockResolvedValue({
      data: [entryActive, entryLarge],
      next_cursor: null,
      has_more: false,
    });
  });

  it("asks the user to connect a wallet and does not fetch when disconnected", () => {
    (useWallet as jest.Mock).mockReturnValue({
      state: "NOT_CONNECTED",
      address: undefined,
    });

    render(<PortfolioPage />);

    expect(
      screen.getByText(/connect your wallet to view your investment portfolio/i)
    ).toBeInTheDocument();
    expect(endpoints.listPortfolio).not.toHaveBeenCalled();
  });

  it("shows a loading state while the first page is pending", () => {
    (endpoints.listPortfolio as jest.Mock).mockReturnValue(
      new Promise(() => {})
    );

    render(<PortfolioPage />);

    expect(screen.getByText(/loading portfolio/i)).toBeInTheDocument();
  });

  it("fetches the connected wallet's first page and renders the table", async () => {
    render(<PortfolioPage />);

    const table = await screen.findByRole("table");
    expect(table).toBeInTheDocument();

    expect(endpoints.listPortfolio).toHaveBeenCalledWith(INVESTOR, null);

    const link = screen.getByRole("link", { name: "0xabc123" });
    expect(link).toHaveAttribute("href", "/invest/invoices/0xabc123");

    expect(screen.getByText("95.0000000")).toBeInTheDocument();
    expect(screen.getByText("5.0000000")).toBeInTheDocument();
    expect(screen.getByText("Active")).toBeInTheDocument();
    expect(screen.getByText("Completed")).toBeInTheDocument();
  });

  it("formats large atomic amounts exactly without precision loss", async () => {
    render(<PortfolioPage />);

    await screen.findByRole("table");

    expect(
      screen.getByText("12345678901234567890123.4567890")
    ).toBeInTheDocument();
    expect(screen.getByText("0.0000000")).toBeInTheDocument();
  });

  it("shows the empty state when the investor has no positions", async () => {
    (endpoints.listPortfolio as jest.Mock).mockResolvedValue({
      data: [],
      next_cursor: null,
      has_more: false,
    });

    render(<PortfolioPage />);

    expect(
      await screen.findByText(/no investment positions yet/i)
    ).toBeInTheDocument();
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
  });

  it("shows an error with a retry action and recovers on retry", async () => {
    (endpoints.listPortfolio as jest.Mock)
      .mockRejectedValueOnce(new Error("Failed to fetch portfolio: HTTP 500"))
      .mockResolvedValueOnce({
        data: [entryActive],
        next_cursor: null,
        has_more: false,
      });
    const user = userEvent.setup();

    render(<PortfolioPage />);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Failed to fetch portfolio: HTTP 500");

    await user.click(screen.getByRole("button", { name: /try again/i }));

    expect(await screen.findByRole("table")).toBeInTheDocument();
    expect(endpoints.listPortfolio).toHaveBeenCalledTimes(2);
    expect(endpoints.listPortfolio).toHaveBeenLastCalledWith(INVESTOR, null);
  });

  it("pages forward with next_cursor and back to the previous cursor", async () => {
    (endpoints.listPortfolio as jest.Mock).mockImplementation(
      (_investor: string, cursor?: string | null) => {
        if (cursor === "cursor-2") {
          return Promise.resolve({
            data: [entryLarge],
            next_cursor: null,
            has_more: false,
          });
        }
        return Promise.resolve({
          data: [entryActive],
          next_cursor: "cursor-2",
          has_more: true,
        });
      }
    );
    const user = userEvent.setup();

    render(<PortfolioPage />);

    await screen.findByRole("link", { name: "0xabc123" });
    expect(screen.getByText("Page 1")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /next page/i }));

    await waitFor(() => {
      expect(endpoints.listPortfolio).toHaveBeenLastCalledWith(
        INVESTOR,
        "cursor-2"
      );
    });
    expect(await screen.findByRole("link", { name: "0xdef456" })).toBeInTheDocument();
    expect(screen.getByText("Page 2")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /previous page/i }));

    await waitFor(() => {
      expect(endpoints.listPortfolio).toHaveBeenLastCalledWith(INVESTOR, null);
    });
    expect(await screen.findByRole("link", { name: "0xabc123" })).toBeInTheDocument();
  });
});

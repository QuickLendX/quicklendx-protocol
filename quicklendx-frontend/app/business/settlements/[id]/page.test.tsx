import React from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SettlementReceiptPage from "./page";
import * as dataFetcher from "./_lib/data";
import { WalletProvider } from "@/app/lib/useWallet";

// Mock useParams
const mockUseParams = jest.fn();
jest.mock("next/navigation", () => ({
  useParams: () => mockUseParams(),
}));

const mockSettlement: dataFetcher.Settlement = {
  id: "0xsettle123",
  invoice_id: "0xinv8201",
  amount: "50000000000",
  payer: "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ",
  recipient: "GBSXVD727UNXJZ7ZM4VCXBTK3UMPXR7O6LLXS7XVOECGDYH3XFNV7C5K",
  timestamp: 1777390800,
  status: "Paid",
};

const mockInvoice: dataFetcher.Invoice = {
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

describe("SettlementReceiptPage component", () => {
  let getSettlementSpy: jest.SpyInstance;
  let getInvoiceSpy: jest.SpyInstance;

  beforeEach(() => {
    jest.clearAllMocks();
    mockUseParams.mockReturnValue({ id: "0xsettle123" });
    getSettlementSpy = jest
      .spyOn(dataFetcher, "getSettlement")
      .mockResolvedValue(mockSettlement);
    getInvoiceSpy = jest
      .spyOn(dataFetcher, "getInvoice")
      .mockResolvedValue(mockInvoice);
  });

  afterEach(() => {
    getSettlementSpy.mockRestore();
    getInvoiceSpy.mockRestore();
  });

  it("with no wallet: connect prompt renders and neither fetcher is called", async () => {
    render(
      <WalletProvider initialAddress={null}>
        <SettlementReceiptPage />
      </WalletProvider>
    );

    expect(
      screen.getByText("Connect Wallet to View Receipt")
    ).toBeInTheDocument();
    expect(getSettlementSpy).not.toHaveBeenCalled();
    expect(getInvoiceSpy).not.toHaveBeenCalled();
  });

  it("owner: with wallet equal to invoice.business, renders amount, due date, description and calls both fetchers", async () => {
    const ownerAddress = mockInvoice.business;

    render(
      <WalletProvider initialAddress={ownerAddress}>
        <SettlementReceiptPage />
      </WalletProvider>
    );

    // Initial loading skeleton state
    expect(screen.getByRole("status")).toHaveAttribute("aria-busy", "true");

    await waitFor(() => {
      expect(
        screen.getByText("Professional Services, April 2026")
      ).toBeInTheDocument();
    });

    expect(getSettlementSpy).toHaveBeenCalledWith(
      "0xsettle123",
      expect.any(AbortSignal)
    );
    expect(getInvoiceSpy).toHaveBeenCalledWith(
      "0xinv8201",
      expect.any(AbortSignal)
    );

    // Check amount
    expect(screen.getByText("50000000000 USDC")).toBeInTheDocument();
    expect(screen.getByText("✓ Settled")).toBeInTheDocument();
  });

  it("stranger: with an unrelated address, renders no-access message and no amounts appear", async () => {
    const strangerAddress =
      "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    render(
      <WalletProvider initialAddress={strangerAddress}>
        <SettlementReceiptPage />
      </WalletProvider>
    );

    await waitFor(() => {
      expect(
        screen.getByText("You don't have access to this receipt.")
      ).toBeInTheDocument();
    });

    expect(screen.queryByText(/5,000/)).toBeNull();
    expect(screen.queryByText(/50000000000/)).toBeNull();
  });

  it("404 (BusinessLogicError or not found): renders Receipt not found with link to /business/settlements", async () => {
    getSettlementSpy.mockRejectedValue({
      name: "BusinessLogicError",
      message: "The requested resource was not found.",
    });

    render(
      <WalletProvider initialAddress={mockInvoice.business}>
        <SettlementReceiptPage />
      </WalletProvider>
    );

    await waitFor(() => {
      expect(screen.getByText("Receipt not found")).toBeInTheDocument();
    });

    const backLink = screen.getByRole("link", { name: "Back to Settlements" });
    expect(backLink).toBeInTheDocument();
    expect(backLink).toHaveAttribute("href", "/business/settlements");
  });

  it("500 error: renders role='alert' and Retry refetches", async () => {
    getSettlementSpy.mockRejectedValueOnce(new Error("Internal Server Error"));

    render(
      <WalletProvider initialAddress={mockInvoice.business}>
        <SettlementReceiptPage />
      </WalletProvider>
    );

    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });
    expect(screen.getByText("Internal Server Error")).toBeInTheDocument();

    // Now resolve on retry
    getSettlementSpy.mockResolvedValueOnce(mockSettlement);

    const retryButton = screen.getByRole("button", { name: "Retry" });
    await userEvent.click(retryButton);

    await waitFor(() => {
      expect(
        screen.getByText("Professional Services, April 2026")
      ).toBeInTheDocument();
    });
    expect(getSettlementSpy).toHaveBeenCalledTimes(2);
  });
});

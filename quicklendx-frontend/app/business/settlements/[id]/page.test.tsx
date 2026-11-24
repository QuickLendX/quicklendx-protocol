import React from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { notFound } from "next/navigation";
import SettlementReceiptPage from "./page";
import * as dataFetcher from "./_lib/data";
import { toJsonExport, toReceiptViewModel } from "./_lib/view-model";
import { WalletProvider } from "@/app/lib/useWallet";

const SETTLEMENT_ID = "0x5e7711e123";

const mockUseParams = jest.fn();
jest.mock("next/navigation", () => ({
  useParams: () => mockUseParams(),
  notFound: jest.fn(),
}));

const mockNotFound = notFound as unknown as jest.Mock;

const mockSettlement: dataFetcher.Settlement = {
  id: SETTLEMENT_ID,
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
  description: "Professional Services, April 2024",
  category: "Services",
  created_at: 1776000000,
  updated_at: 1777390800,
};

const createObjectURL = jest.fn((_blob: Blob) => "blob:receipt");
const revokeObjectURL = jest.fn();

function readBlob(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(reader.error);
    reader.readAsText(blob);
  });
}

describe("SettlementReceiptPage component", () => {
  let getSettlementSpy: jest.SpyInstance;
  let getInvoiceSpy: jest.SpyInstance;

  beforeAll(() => {
    Object.defineProperty(URL, "createObjectURL", {
      value: createObjectURL,
      writable: true,
    });
    Object.defineProperty(URL, "revokeObjectURL", {
      value: revokeObjectURL,
      writable: true,
    });
  });

  beforeEach(() => {
    jest.clearAllMocks();
    mockUseParams.mockReturnValue({ id: SETTLEMENT_ID });
    getSettlementSpy = jest
      .spyOn(dataFetcher, "getSettlement")
      .mockResolvedValue(mockSettlement);
    getInvoiceSpy = jest
      .spyOn(dataFetcher, "getInvoice")
      .mockResolvedValue(mockInvoice);
  });

  afterEach(() => {
    jest.restoreAllMocks();
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

    expect(screen.getByRole("status")).toHaveAttribute("aria-busy", "true");

    await waitFor(() => {
      expect(
        screen.getByText("Professional Services, April 2024")
      ).toBeInTheDocument();
    });

    expect(getSettlementSpy).toHaveBeenCalledWith(
      SETTLEMENT_ID,
      expect.any(AbortSignal)
    );
    expect(getInvoiceSpy).toHaveBeenCalledWith(
      "0xinv8201",
      expect.any(AbortSignal)
    );
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

    expect(screen.queryByText(/50000000000/)).toBeNull();
  });

  it("invalid id: calls notFound before fetching", async () => {
    mockUseParams.mockReturnValue({ id: "not-hex" });

    render(
      <WalletProvider initialAddress={mockInvoice.business}>
        <SettlementReceiptPage />
      </WalletProvider>
    );

    expect(mockNotFound).toHaveBeenCalled();
    expect(getSettlementSpy).not.toHaveBeenCalled();
    expect(getInvoiceSpy).not.toHaveBeenCalled();
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

    getSettlementSpy.mockResolvedValueOnce(mockSettlement);

    const retryButton = screen.getByRole("button", { name: "Retry" });
    await userEvent.click(retryButton);

    await waitFor(() => {
      expect(
        screen.getByText("Professional Services, April 2024")
      ).toBeInTheDocument();
    });
    expect(getSettlementSpy).toHaveBeenCalledTimes(2);
  });

  it("owner: Download JSON exports the view model without fabricated fields", async () => {
    const clickSpy = jest
      .spyOn(HTMLAnchorElement.prototype, "click")
      .mockImplementation(() => {});

    render(
      <WalletProvider initialAddress={mockInvoice.business}>
        <SettlementReceiptPage />
      </WalletProvider>
    );

    await waitFor(() => {
      expect(
        screen.getByText("Professional Services, April 2024")
      ).toBeInTheDocument();
    });

    await userEvent.click(
      screen.getByRole("button", {
        name: `Download settlement receipt ${SETTLEMENT_ID} as JSON`,
      })
    );

    await waitFor(() => {
      expect(screen.getByText("JSON downloaded.")).toBeInTheDocument();
    });

    expect(createObjectURL).toHaveBeenCalledTimes(1);
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:receipt");
    expect(clickSpy).toHaveBeenCalledTimes(1);

    const blob = createObjectURL.mock.calls[0][0] as Blob;
    expect(blob.type).toBe("application/json");
    await expect(readBlob(blob)).resolves.toBe(
      JSON.stringify(
        toJsonExport(toReceiptViewModel(mockSettlement, mockInvoice)),
        null,
        2
      )
    );
  });
});

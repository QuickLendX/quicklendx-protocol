import React from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { InvoiceForm } from "./InvoiceForm";
import {
  WalletProvider,
  WalletStatus,
} from "../../../lib/wallet/WalletProvider";
import * as contractClient from "../../../lib/stellar/contract-client";
import { ContractError } from "../../../lib/stellar/contract-errors";

// Mock router
const mockPush = jest.fn();
jest.mock("next/navigation", () => ({
  useRouter: () => ({
    push: mockPush,
  }),
}));

// Mock toast
jest.mock("react-hot-toast", () => ({
  success: jest.fn(),
  error: jest.fn(),
}));

// Mock tokens
jest.mock("../../../lib/stellar/tokens", () => ({
  getConfiguredTokens: () => [
    {
      code: "USDC",
      address: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
      decimals: 7,
    },
  ],
}));

describe("InvoiceForm Component Tests", () => {
  const defaultWalletAddress =
    "GARFWKJWKJG7DFIOVV7CZRPAQCFPEU5RQLHJ6UL33GDU6N7JIUKAMPP6";

  beforeEach(() => {
    jest.clearAllMocks();
  });

  const renderFormWithWallet = (
    walletStatus: WalletStatus = "CONNECTED",
    network = "testnet",
    expectedNetwork = "testnet",
    customSigner?: (xdr: string) => Promise<string>
  ) => {
    return render(
      <WalletProvider
        initialStatus={walletStatus}
        initialAddress={
          walletStatus === "CONNECTED" ? defaultWalletAddress : null
        }
        initialNetwork={network}
        expectedNetwork={expectedNetwork}
        signTransaction={customSigner}
      >
        <InvoiceForm />
      </WalletProvider>
    );
  };

  const fillValidForm = async (user: ReturnType<typeof userEvent.setup>) => {
    const amountInput = screen.getByLabelText(/^Amount/i);
    await user.clear(amountInput);
    await user.type(amountInput, "250.75");

    const dueDateInput = screen.getByLabelText(/^Due Date/i);
    const tomorrow = new Date();
    tomorrow.setDate(tomorrow.getDate() + 2);
    const dateStr = tomorrow.toISOString().split("T")[0];
    await user.clear(dueDateInput);
    await user.type(dueDateInput, dateStr);

    const descInput = screen.getByRole("textbox", { name: /^Description$/i });
    await user.clear(descInput);
    await user.type(descInput, "Consulting services rendered");
  };

  it("double click gives 1 call to invokeContract", async () => {
    const user = userEvent.setup();
    let resolveInvoke: (val: any) => void;
    const invokePromise = new Promise((res) => {
      resolveInvoke = res;
    });

    const invokeSpy = jest
      .spyOn(contractClient, "invokeContract")
      .mockImplementation(() => invokePromise as any);

    renderFormWithWallet();
    await fillValidForm(user);

    const submitBtn = screen.getByRole("button", { name: /Create Invoice/i });

    // Click twice rapidly
    await user.click(submitBtn);
    await user.click(submitBtn);

    expect(invokeSpy).toHaveBeenCalledTimes(1);

    // Resolve
    resolveInvoke!({
      txHash: "0xhash",
      result: "deadbeef00112233",
    });

    await waitFor(() => {
      expect(mockPush).toHaveBeenCalledWith(
        "/business/invoices/deadbeef00112233"
      );
    });
  });

  it("after a timeout error, 'Retry' calls with the same nonce", async () => {
    const user = userEvent.setup();
    const invokeSpy = jest.spyOn(contractClient, "invokeContract");

    // First call times out
    invokeSpy.mockRejectedValueOnce(
      new ContractError("Transaction polling timed out.")
    );

    renderFormWithWallet();
    await fillValidForm(user);

    const submitBtn = screen.getByRole("button", { name: /Create Invoice/i });
    await user.click(submitBtn);

    // Verify error shown with role="alert"
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Transaction polling timed out.");

    // Button should now show "Retry"
    const retryBtn = screen.getByRole("button", { name: /Retry/i });
    expect(retryBtn).toBeInTheDocument();

    const firstNonce = (invokeSpy.mock.calls[0][0].args[7] as any)._value;

    // Next call succeeds
    invokeSpy.mockResolvedValueOnce({
      txHash: "0xhash2",
      result: "cafebabe44556677",
    });

    await user.click(retryBtn);

    expect(invokeSpy).toHaveBeenCalledTimes(2);
    const secondNonce = (invokeSpy.mock.calls[1][0].args[7] as any)._value;

    // Nonce must be identical across retry
    expect(firstNonce).toEqual(secondNonce);

    await waitFor(() => {
      expect(mockPush).toHaveBeenCalledWith(
        "/business/invoices/cafebabe44556677"
      );
    });
  });

  it("after editing the description, the next submit uses a different nonce", async () => {
    const user = userEvent.setup();
    const invokeSpy = jest.spyOn(contractClient, "invokeContract");

    invokeSpy.mockRejectedValueOnce(
      new ContractError("Transaction polling timed out.")
    );

    renderFormWithWallet();
    await fillValidForm(user);

    const submitBtn = screen.getByRole("button", { name: /Create Invoice/i });
    await user.click(submitBtn);

    await screen.findByRole("alert");
    const firstNonce = (invokeSpy.mock.calls[0][0].args[7] as any)._value;

    // Edit description
    const descInput = screen.getByRole("textbox", { name: /^Description$/i });
    await user.type(descInput, " updated addition");

    invokeSpy.mockResolvedValueOnce({
      txHash: "0xhash3",
      result: "abcdef11223344",
    });

    const submitBtnAfterEdit = screen.getByRole("button", { name: /Retry/i });
    await user.click(submitBtnAfterEdit);

    expect(invokeSpy).toHaveBeenCalledTimes(2);
    const secondNonce = (invokeSpy.mock.calls[1][0].args[7] as any)._value;

    // Nonce must be different
    expect(firstNonce).not.toEqual(secondNonce);
  });

  it("code 1601 shows the KYC-pending copy", async () => {
    const user = userEvent.setup();
    jest
      .spyOn(contractClient, "invokeContract")
      .mockRejectedValueOnce(
        new ContractError("Your KYC is still under review.", 1601)
      );

    renderFormWithWallet();
    await fillValidForm(user);

    const submitBtn = screen.getByRole("button", { name: /Create Invoice/i });
    await user.click(submitBtn);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Your KYC is still under review.");
  });

  it("success navigates to /business/invoices/0x…", async () => {
    const user = userEvent.setup();
    jest.spyOn(contractClient, "invokeContract").mockResolvedValueOnce({
      txHash: "0xhash123",
      result:
        "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
    });

    renderFormWithWallet();
    await fillValidForm(user);

    const submitBtn = screen.getByRole("button", { name: /Create Invoice/i });
    await user.click(submitBtn);

    await waitFor(() => {
      expect(mockPush).toHaveBeenCalledWith(
        "/business/invoices/0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
      );
    });
  });

  it("with the wallet in WRONG_NETWORK, the submit button is disabled with an explanatory hint", () => {
    renderFormWithWallet("WRONG_NETWORK", "mainnet", "testnet");

    const submitBtn = screen.getByRole("button", { name: /Create Invoice/i });
    expect(submitBtn).toBeDisabled();

    const hint = screen.getByText(
      /Wallet is connected to the wrong network. Switch to testnet to submit./i
    );
    expect(hint).toBeInTheDocument();
  });
});

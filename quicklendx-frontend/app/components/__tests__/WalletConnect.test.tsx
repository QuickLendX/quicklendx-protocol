import React from "react";
import { TextEncoder } from "util";
import { webcrypto } from "crypto";
import { render, screen, waitFor, within } from "@testing-library/react";

if (typeof globalThis.TextEncoder === "undefined") {
  globalThis.TextEncoder = TextEncoder;
}
if (!globalThis.crypto?.subtle) {
  Object.defineProperty(globalThis, "crypto", {
    value: webcrypto,
    configurable: true,
  });
}
import userEvent from "@testing-library/user-event";
import {
  WalletConnect,
  truncateAddress,
  type WalletConnectionState,
} from "../WalletConnect";
import type { WalletContextValue } from "../../lib/wallet/WalletProvider";
import {
  configureWalletTelemetry,
  resetWalletTelemetry,
  setWalletTelemetryProvider,
  trackWalletConnectAttempt,
  trackWalletConnected,
  trackWalletConnectFailed,
  trackWalletDisconnected,
  type WalletTelemetryProvider,
} from "../../lib/wallet-telemetry";

const mockUseWallet = jest.fn<WalletContextValue, []>();

jest.mock("../../lib/wallet/WalletProvider", () => ({
  useWallet: () => mockUseWallet(),
}));

jest.mock("../../lib/wallet-telemetry", () => {
  const actual = jest.requireActual("../../lib/wallet-telemetry");
  return {
    ...actual,
    trackWalletConnectAttempt: jest.fn((...args: unknown[]) =>
      actual.trackWalletConnectAttempt(...args)
    ),
    trackWalletConnected: jest.fn((...args: unknown[]) =>
      actual.trackWalletConnected(...args)
    ),
    trackWalletConnectFailed: jest.fn((...args: unknown[]) =>
      actual.trackWalletConnectFailed(...args)
    ),
    trackWalletDisconnected: jest.fn((...args: unknown[]) =>
      actual.trackWalletDisconnected(...args)
    ),
  };
});

const TEST_PUBLIC_KEY =
  "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7";

function setWalletState(
  state: WalletConnectionState,
  overrides: Partial<WalletContextValue> = {}
): {
  connect: jest.Mock;
  disconnect: jest.Mock;
} {
  const connect = (overrides.connect as jest.Mock) ?? jest.fn();
  const disconnect = (overrides.disconnect as jest.Mock) ?? jest.fn();
  mockUseWallet.mockReturnValue({
    state,
    address: overrides.address,
    connect,
    disconnect,
  });
  return { connect, disconnect };
}

describe("WalletConnect", () => {
  let providerTrackEvent: jest.Mock;

  beforeEach(() => {
    jest.clearAllMocks();
    providerTrackEvent = jest.fn();
    const mockProvider: WalletTelemetryProvider = {
      name: "test-provider",
      trackEvent: providerTrackEvent,
    };
    resetWalletTelemetry();
    configureWalletTelemetry({ enabled: true });
    setWalletTelemetryProvider(mockProvider);
  });

  afterEach(() => {
    resetWalletTelemetry();
  });

  describe("connection states", () => {
    it("renders Connect Wallet button and fires onDisconnect in NOT_CONNECTED state", () => {
      setWalletState("NOT_CONNECTED");
      const onDisconnect = jest.fn();

      render(<WalletConnect onDisconnect={onDisconnect} />);

      expect(
        screen.getByRole("button", { name: "Connect Wallet" })
      ).toBeInTheDocument();
      expect(onDisconnect).toHaveBeenCalledTimes(1);
    });

    it("renders disabled Connecting… button with spinner in CONNECTING state", async () => {
      const user = userEvent.setup();
      setWalletState("CONNECTING");

      render(<WalletConnect />);

      await user.click(screen.getByRole("button", { name: "Connect Wallet" }));

      const dialog = screen.getByRole("dialog");
      const connectingBtn = within(dialog).getByRole("button", {
        name: /Connecting…/i,
      });
      expect(connectingBtn).toBeDisabled();
      expect(
        within(dialog).getByRole("status", { name: "Connecting…" })
      ).toBeInTheDocument();

      // Escape must not close the modal while connecting
      await user.keyboard("{Escape}");
      expect(screen.getByRole("dialog")).toBeInTheDocument();
    });

    it("renders truncated address pill, explorer link, and fires onConnect in CONNECTED state", async () => {
      const user = userEvent.setup();
      const { disconnect } = setWalletState("CONNECTED", {
        address: TEST_PUBLIC_KEY,
      });
      const onConnect = jest.fn();

      render(<WalletConnect expectedNetwork="testnet" onConnect={onConnect} />);

      expect(onConnect).toHaveBeenCalledWith(TEST_PUBLIC_KEY);
      expect(
        screen.getByText(truncateAddress(TEST_PUBLIC_KEY))
      ).toBeInTheDocument();

      const explorerLink = screen.getByRole("link", {
        name: new RegExp(`View account ${TEST_PUBLIC_KEY} on Stellar Expert`),
      });
      expect(explorerLink).toHaveAttribute(
        "href",
        `https://stellar.expert/explorer/testnet/account/${TEST_PUBLIC_KEY}`
      );

      // Open account menu and disconnect
      await user.click(
        screen.getByRole("button", {
          name: "Wallet connected — click to open account menu",
        })
      );
      await user.click(screen.getByRole("button", { name: "Disconnect" }));

      expect(disconnect).toHaveBeenCalledTimes(1);
      expect(trackWalletDisconnected).toHaveBeenCalledWith({
        address: TEST_PUBLIC_KEY,
        network: "testnet",
      });
    });

    it("renders wrong-network pill and inline alert banner in WRONG_NETWORK state", async () => {
      const user = userEvent.setup();
      const { disconnect } = setWalletState("WRONG_NETWORK", {
        address: TEST_PUBLIC_KEY,
      });

      render(<WalletConnect expectedNetwork="public" />);

      expect(screen.getByText(/Wrong network —/i)).toBeInTheDocument();

      const alert = screen.getByRole("alert");
      expect(alert).toHaveTextContent(
        /Wrong network detected\. Switch to Mainnet in Freighter to continue\./i
      );

      // Clicking Disconnect in banner calls disconnect and emits telemetry
      await user.click(
        within(alert).getByRole("button", { name: "Disconnect" })
      );
      expect(disconnect).toHaveBeenCalledTimes(1);
      expect(trackWalletDisconnected).toHaveBeenCalledWith({
        address: TEST_PUBLIC_KEY,
        network: "public",
      });

      // Clicking "fix" opens the connect modal and hides the inline banner
      await user.click(screen.getByRole("button", { name: "fix" }));
      expect(screen.getByRole("dialog")).toBeInTheDocument();
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });

    it("renders error copy and Try Again button in ERROR state", async () => {
      const user = userEvent.setup();
      const connect = jest.fn().mockRejectedValueOnce(new Error("rejected"));
      setWalletState("ERROR", { connect });

      render(<WalletConnect />);

      await user.click(screen.getByRole("button", { name: "Connect Wallet" }));

      const dialog = screen.getByRole("dialog");
      expect(
        within(dialog).getByText("Something went wrong. Please try again.")
      ).toBeInTheDocument();

      // Triggering connect failure updates the error message
      await user.click(
        within(dialog).getByRole("button", { name: "Try Again" })
      );

      await waitFor(() => {
        expect(
          within(dialog).getByText(
            "Connection was declined. You can try again at any time."
          )
        ).toBeInTheDocument();
      });
      expect(trackWalletConnectFailed).toHaveBeenCalledWith({
        error: "rejected",
        walletType: "freighter",
        network: "testnet",
      });
    });

    it("renders Get Freighter link without Try Again when error is not_installed", async () => {
      const user = userEvent.setup();
      const connect = jest
        .fn()
        .mockRejectedValueOnce(new Error("not_installed"));
      setWalletState("ERROR", { connect });

      render(<WalletConnect />);

      await user.click(screen.getByRole("button", { name: "Connect Wallet" }));
      const dialog = screen.getByRole("dialog");

      await user.click(within(dialog).getByRole("button", { name: "Connect" }));

      await waitFor(() => {
        expect(
          within(dialog).getByText(/Freighter is not installed\./i)
        ).toBeInTheDocument();
      });
      expect(
        within(dialog).getByRole("link", { name: "Get Freighter" })
      ).toHaveAttribute("href", "https://freighter.app");
      expect(
        within(dialog).queryByRole("button", { name: "Try Again" })
      ).not.toBeInTheDocument();
    });

    it("renders fallback trigger without firing onDisconnect in DISCONNECTING state", () => {
      setWalletState("DISCONNECTING");
      const onDisconnect = jest.fn();

      render(<WalletConnect onDisconnect={onDisconnect} />);

      expect(
        screen.getByRole("button", { name: "Connect Wallet" })
      ).toBeInTheDocument();
      expect(onDisconnect).not.toHaveBeenCalled();
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });
  });

  describe("connect flow and telemetry privacy", () => {
    it("emits trackWalletConnectAttempt when initiating connection from modal", async () => {
      const user = userEvent.setup();
      const { connect } = setWalletState("NOT_CONNECTED");

      render(<WalletConnect expectedNetwork="testnet" />);

      await user.click(screen.getByRole("button", { name: "Connect Wallet" }));
      const dialog = screen.getByRole("dialog");
      await user.click(within(dialog).getByRole("button", { name: "Connect" }));

      expect(connect).toHaveBeenCalledTimes(1);
      expect(trackWalletConnectAttempt).toHaveBeenCalledWith({
        walletType: "freighter",
        network: "testnet",
      });
    });

    it("never transmits raw public key (PII) to telemetry provider on connect or disconnect", async () => {
      const user = userEvent.setup();
      setWalletState("CONNECTED", { address: TEST_PUBLIC_KEY });

      render(<WalletConnect expectedNetwork="testnet" />);

      await waitFor(() => {
        expect(providerTrackEvent).toHaveBeenCalledWith(
          "wallet_connected",
          expect.objectContaining({
            walletType: "freighter",
            network: "testnet",
            address_hash: expect.stringMatching(/^[0-9a-f]{16}$/),
          })
        );
      });

      await user.click(
        screen.getByRole("button", {
          name: "Wallet connected — click to open account menu",
        })
      );
      await user.click(screen.getByRole("button", { name: "Disconnect" }));

      await waitFor(() => {
        expect(providerTrackEvent).toHaveBeenCalledWith(
          "wallet_disconnected",
          expect.objectContaining({
            network: "testnet",
            address_hash: expect.stringMatching(/^[0-9a-f]{16}$/),
          })
        );
      });

      // Verify no telemetry payload ever contains the raw public key or raw address field
      for (const [, payload] of providerTrackEvent.mock.calls) {
        expect(payload).not.toHaveProperty("address");
        expect(JSON.stringify(payload)).not.toContain(TEST_PUBLIC_KEY);
      }
    });
  });
});

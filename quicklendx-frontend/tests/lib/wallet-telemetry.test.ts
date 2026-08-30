/**
 * Unit Tests for Wallet Telemetry & Analytics Provider Module (tests/lib path)
 */

import {
  configureWalletTelemetry,
  createProviderFromConfig,
  CustomTelemetryProvider,
  getWalletTelemetryProvider,
  GoogleAnalyticsTelemetryProvider,
  MixpanelTelemetryProvider,
  NoopTelemetryProvider,
  resetWalletTelemetry,
  resolveConfigFromEnv,
  SelfHostedTelemetryProvider,
  setWalletTelemetryProvider,
  trackWalletConnectAttempt,
  trackWalletConnected,
  trackWalletConnectFailed,
  trackWalletDisconnected,
  trackWalletEvent,
  trackWalletNetworkChanged,
  WalletTelemetryProvider,
  WalletTelemetryService,
} from "../../app/lib/wallet-telemetry";

describe("Wallet Telemetry (tests/lib) - Forwarding & Disabled Behavior", () => {
  let mockTrackEvent: jest.Mock;

  beforeEach(() => {
    resetWalletTelemetry();
    mockTrackEvent = jest.fn();
    setWalletTelemetryProvider({
      name: "test-provider",
      trackEvent: mockTrackEvent,
    });
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  it("asserts events are forwarded when telemetry is enabled", async () => {
    configureWalletTelemetry({ enabled: true });

    await trackWalletConnected({
      address: "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ",
      walletType: "freighter",
      network: "testnet",
    });

    expect(mockTrackEvent).toHaveBeenCalledTimes(1);
    expect(mockTrackEvent).toHaveBeenCalledWith(
      "wallet_connected",
      expect.objectContaining({
        address: "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ",
        walletType: "freighter",
        network: "testnet",
        timestamp: expect.any(Number),
      })
    );
  });

  it("asserts events are NOT forwarded when telemetry is disabled", async () => {
    const noopProvider = new NoopTelemetryProvider();
    setWalletTelemetryProvider(noopProvider);
    configureWalletTelemetry({ enabled: false, provider: "noop" });

    await trackWalletConnected({
      address: "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ",
      walletType: "freighter",
      network: "testnet",
    });

    expect(mockTrackEvent).not.toHaveBeenCalled();
  });

  it("asserts connect attempt, failure, and disconnect are forwarded when enabled", async () => {
    configureWalletTelemetry({ enabled: true });

    await trackWalletConnectAttempt({ walletType: "freighter", network: "testnet" });
    await trackWalletConnectFailed({
      error: "User rejected connection",
      walletType: "freighter",
      network: "testnet",
    });
    await trackWalletDisconnected({
      address: "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ",
      network: "testnet",
      reason: "user_initiated",
    });

    expect(mockTrackEvent).toHaveBeenCalledTimes(3);
    expect(mockTrackEvent).toHaveBeenNthCalledWith(
      1,
      "wallet_connect_attempt",
      expect.objectContaining({ walletType: "freighter" })
    );
    expect(mockTrackEvent).toHaveBeenNthCalledWith(
      2,
      "wallet_connect_failed",
      expect.objectContaining({ error: "User rejected connection" })
    );
    expect(mockTrackEvent).toHaveBeenNthCalledWith(
      3,
      "wallet_disconnected",
      expect.objectContaining({ reason: "user_initiated" })
    );
  });
});

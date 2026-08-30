/**
 * Unit Tests for Wallet Telemetry & Analytics Provider Module
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
} from "../wallet-telemetry";

describe("Wallet Telemetry - Provider Implementations", () => {
  const originalWindow = global.window;
  const originalFetch = global.fetch;

  beforeEach(() => {
    jest.clearAllMocks();
  });

  afterEach(() => {
    global.window = originalWindow;
    global.fetch = originalFetch;
  });

  describe("NoopTelemetryProvider", () => {
    it("safely handles trackEvent, identify, and reset without throwing", () => {
      const noop = new NoopTelemetryProvider(false);
      expect(noop.name).toBe("noop");
      expect(() => {
        noop.trackEvent("test_event", { foo: "bar" });
        noop.identify("user_123", { plan: "pro" });
        noop.reset();
      }).not.toThrow();
    });

    it("logs in debug mode without failing", () => {
      const debugSpy = jest.spyOn(console, "debug").mockImplementation(() => {});
      const noop = new NoopTelemetryProvider(true);
      noop.trackEvent("test_event", { foo: "bar" });
      noop.identify("user_123");
      noop.reset();
      expect(debugSpy).toHaveBeenCalled();
      debugSpy.mockRestore();
    });
  });

  describe("MixpanelTelemetryProvider", () => {
    it("forwards events to window.mixpanel.track when present", async () => {
      const mixpanelTrackMock = jest.fn();
      const mixpanelIdentifyMock = jest.fn();
      const mixpanelPeopleSetMock = jest.fn();

      (global as any).window = {
        mixpanel: {
          track: mixpanelTrackMock,
          identify: mixpanelIdentifyMock,
          people: {
            set: mixpanelPeopleSetMock,
          },
        },
      };

      const provider = new MixpanelTelemetryProvider("dummy_token");
      expect(provider.name).toBe("mixpanel");

      await provider.trackEvent("wallet_connected", { address: "GBBX...123", network: "testnet" });
      expect(mixpanelTrackMock).toHaveBeenCalledTimes(1);
      expect(mixpanelTrackMock).toHaveBeenCalledWith(
        "wallet_connected",
        expect.objectContaining({
          address: "GBBX...123",
          network: "testnet",
          timestamp: expect.any(Number),
        })
      );

      provider.identify("user_1", { is_admin: true });
      expect(mixpanelIdentifyMock).toHaveBeenCalledWith("user_1");
      expect(mixpanelPeopleSetMock).toHaveBeenCalledWith({ is_admin: true });
    });

    it("forwards events via HTTP fetch when window.mixpanel is undefined", async () => {
      (global as any).window = undefined;
      const fetchMock = jest.fn().mockResolvedValue({ ok: true });
      (global as any).fetch = fetchMock;

      const provider = new MixpanelTelemetryProvider("test_token_123", "https://api.mixpanel.com/track");
      await provider.trackEvent("wallet_disconnected", { address: "GBBX...456" });

      expect(fetchMock).toHaveBeenCalledTimes(1);
      expect(fetchMock).toHaveBeenCalledWith(
        "https://api.mixpanel.com/track",
        expect.objectContaining({
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: expect.stringContaining("test_token_123"),
        })
      );
    });
  });

  describe("GoogleAnalyticsTelemetryProvider", () => {
    it("forwards events to window.gtag when available", () => {
      const gtagMock = jest.fn();
      (global as any).window = {
        gtag: gtagMock,
      };

      const provider = new GoogleAnalyticsTelemetryProvider("G-XXXXX");
      expect(provider.name).toBe("google-analytics");

      provider.trackEvent("wallet_connect_attempt", { walletType: "freighter" });
      expect(gtagMock).toHaveBeenCalledWith(
        "event",
        "wallet_connect_attempt",
        expect.objectContaining({
          walletType: "freighter",
          send_to: "G-XXXXX",
        })
      );

      provider.identify("usr_99", { role: "borrower" });
      expect(gtagMock).toHaveBeenCalledWith(
        "set",
        "user_properties",
        expect.objectContaining({
          user_id: "usr_99",
          role: "borrower",
        })
      );
    });

    it("pushes to dataLayer if gtag function is absent", () => {
      const dataLayerMock: any[] = [];
      (global as any).window = {
        dataLayer: dataLayerMock,
      };

      const provider = new GoogleAnalyticsTelemetryProvider();
      provider.trackEvent("wallet_connect_failed", { error: "SDK_NOT_FOUND" });

      expect(dataLayerMock.length).toBe(1);
      expect(dataLayerMock[0]).toEqual(
        expect.objectContaining({
          event: "wallet_connect_failed",
          error: "SDK_NOT_FOUND",
        })
      );
    });
  });

  describe("SelfHostedTelemetryProvider", () => {
    it("posts JSON payload to configured endpoint", async () => {
      const fetchMock = jest.fn().mockResolvedValue({ ok: true });
      (global as any).fetch = fetchMock;

      const provider = new SelfHostedTelemetryProvider("/api/v1/telemetry", "secret_key");
      expect(provider.name).toBe("self-hosted");

      await provider.trackEvent("wallet_connected", { address: "GBBX..." });

      expect(fetchMock).toHaveBeenCalledTimes(1);
      expect(fetchMock).toHaveBeenCalledWith(
        "/api/v1/telemetry",
        expect.objectContaining({
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            Authorization: "Bearer secret_key",
          },
          body: JSON.stringify({
            event: "wallet_connected",
            properties: { address: "GBBX..." },
            timestamp: expect.any(Number),
          }),
        })
      );
    });

    it("silently catches network errors without throwing to caller", async () => {
      const warnSpy = jest.spyOn(console, "warn").mockImplementation(() => {});
      (global as any).fetch = jest.fn().mockRejectedValue(new Error("Network Error"));

      const provider = new SelfHostedTelemetryProvider("/api/telemetry");
      await expect(provider.trackEvent("wallet_event")).resolves.not.toThrow();

      warnSpy.mockRestore();
    });
  });

  describe("CustomTelemetryProvider", () => {
    it("delegates trackEvent to the provided handler", async () => {
      const handlerMock = jest.fn();
      const customProvider = new CustomTelemetryProvider(handlerMock);
      expect(customProvider.name).toBe("custom");

      await customProvider.trackEvent("custom_event", { data: 123 });
      expect(handlerMock).toHaveBeenCalledWith("custom_event", { data: 123 });
    });
  });
});

describe("Wallet Telemetry - Environment Config & Resolution", () => {
  const originalEnv = process.env;

  beforeEach(() => {
    process.env = { ...originalEnv };
  });

  afterAll(() => {
    process.env = originalEnv;
  });

  it("resolves config based on environment variables", () => {
    process.env.NEXT_PUBLIC_ANALYTICS_ENABLED = "true";
    process.env.NEXT_PUBLIC_ANALYTICS_PROVIDER = "mixpanel";
    process.env.NEXT_PUBLIC_MIXPANEL_TOKEN = "mp_token_xyz";

    const config = resolveConfigFromEnv();
    expect(config.enabled).toBe(true);
    expect(config.provider).toBe("mixpanel");
    expect(config.apiKey).toBe("mp_token_xyz");

    const provider = createProviderFromConfig(config);
    expect(provider.name).toBe("mixpanel");
  });

  it("resolves to NoopTelemetryProvider when disabled", () => {
    process.env.NEXT_PUBLIC_ANALYTICS_ENABLED = "false";
    process.env.NEXT_PUBLIC_ANALYTICS_PROVIDER = "mixpanel";

    const config = resolveConfigFromEnv();
    expect(config.enabled).toBe(false);

    const provider = createProviderFromConfig(config);
    expect(provider.name).toBe("noop");
  });

  it("creates GoogleAnalytics provider when ga specified", () => {
    const config = { enabled: true, provider: "ga" as const, apiKey: "G-12345" };
    const provider = createProviderFromConfig(config);
    expect(provider.name).toBe("google-analytics");
  });

  it("creates SelfHosted provider when self-hosted specified", () => {
    const config = {
      enabled: true,
      provider: "self-hosted" as const,
      endpoint: "/custom/analytics",
    };
    const provider = createProviderFromConfig(config);
    expect(provider.name).toBe("self-hosted");
  });
});

describe("Wallet Telemetry - Service & Event Forwarding", () => {
  beforeEach(() => {
    resetWalletTelemetry();
  });

  it("forwards events when telemetry is enabled", async () => {
    const mockTrackEvent = jest.fn();
    const customProvider: WalletTelemetryProvider = {
      name: "mock-provider",
      trackEvent: mockTrackEvent,
    };

    const service = new WalletTelemetryService({ enabled: true });
    service.setProvider(customProvider);

    await service.trackEvent("wallet_connected", {
      address: "GBBX789",
      network: "testnet",
    });

    expect(mockTrackEvent).toHaveBeenCalledTimes(1);
    expect(mockTrackEvent).toHaveBeenCalledWith(
      "wallet_connected",
      expect.objectContaining({
        address: "GBBX789",
        network: "testnet",
        timestamp: expect.any(Number),
      })
    );
  });

  it("does not forward events when disabled", async () => {
    const mockTrackEvent = jest.fn();
    const service = new WalletTelemetryService({
      enabled: false,
      provider: "noop",
    });

    await service.trackEvent("wallet_connected", { address: "GBBX123" });
    expect(mockTrackEvent).not.toHaveBeenCalled();
    expect(service.isEnabled()).toBe(false);
  });

  it("attaches defaultProperties to all events", async () => {
    const mockTrackEvent = jest.fn();
    const customProvider: WalletTelemetryProvider = {
      name: "mock-provider",
      trackEvent: mockTrackEvent,
    };

    const service = new WalletTelemetryService({
      enabled: true,
      defaultProperties: { app: "quicklendx", version: "1.0.0" },
    });
    service.setProvider(customProvider);

    await service.trackEvent("wallet_disconnected", { reason: "user_logout" });

    expect(mockTrackEvent).toHaveBeenCalledWith(
      "wallet_disconnected",
      expect.objectContaining({
        app: "quicklendx",
        version: "1.0.0",
        reason: "user_logout",
        timestamp: expect.any(Number),
      })
    );
  });

  it("handles provider runtime errors gracefully", async () => {
    const warnSpy = jest.spyOn(console, "warn").mockImplementation(() => {});
    const throwingProvider: WalletTelemetryProvider = {
      name: "failing-provider",
      trackEvent: jest.fn().mockRejectedValue(new Error("Tracking failed")),
    };

    const service = new WalletTelemetryService({ enabled: true });
    service.setProvider(throwingProvider);

    await expect(service.trackEvent("event_x")).resolves.not.toThrow();
    expect(warnSpy).toHaveBeenCalled();
    warnSpy.mockRestore();
  });
});

describe("Wallet Telemetry - Global Helpers & Typed Events", () => {
  let mockTrackEvent: jest.Mock;

  beforeEach(() => {
    resetWalletTelemetry();
    mockTrackEvent = jest.fn();
    setWalletTelemetryProvider({
      name: "test-provider",
      trackEvent: mockTrackEvent,
    });
    configureWalletTelemetry({ enabled: true });
  });

  it("trackWalletEvent sends generic event", async () => {
    await trackWalletEvent("custom_wallet_action", { step: 1 });
    expect(mockTrackEvent).toHaveBeenCalledWith(
      "custom_wallet_action",
      expect.objectContaining({ step: 1 })
    );
  });

  it("trackWalletConnectAttempt forwards event with correct name and props", async () => {
    await trackWalletConnectAttempt({ walletType: "freighter", network: "testnet" });
    expect(mockTrackEvent).toHaveBeenCalledWith(
      "wallet_connect_attempt",
      expect.objectContaining({
        walletType: "freighter",
        network: "testnet",
      })
    );
  });

  it("trackWalletConnected forwards event with address and network", async () => {
    await trackWalletConnected({
      address: "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ",
      walletType: "freighter",
      network: "public",
    });
    expect(mockTrackEvent).toHaveBeenCalledWith(
      "wallet_connected",
      expect.objectContaining({
        address: "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ",
        walletType: "freighter",
        network: "public",
      })
    );
  });

  it("trackWalletConnectFailed forwards error message", async () => {
    await trackWalletConnectFailed({
      error: "User rejected connection",
      walletType: "freighter",
      network: "testnet",
    });
    expect(mockTrackEvent).toHaveBeenCalledWith(
      "wallet_connect_failed",
      expect.objectContaining({
        error: "User rejected connection",
        walletType: "freighter",
      })
    );
  });

  it("trackWalletDisconnected forwards disconnect event", async () => {
    await trackWalletDisconnected({
      address: "GBBX...99",
      reason: "manual_disconnect",
    });
    expect(mockTrackEvent).toHaveBeenCalledWith(
      "wallet_disconnected",
      expect.objectContaining({
        address: "GBBX...99",
        reason: "manual_disconnect",
      })
    );
  });

  it("trackWalletNetworkChanged forwards old and new network", async () => {
    await trackWalletNetworkChanged({
      network: "public",
      previousNetwork: "testnet",
    });
    expect(mockTrackEvent).toHaveBeenCalledWith(
      "wallet_network_changed",
      expect.objectContaining({
        network: "public",
        previousNetwork: "testnet",
      })
    );
  });

  it("getWalletTelemetryProvider returns current provider", () => {
    const provider = getWalletTelemetryProvider();
    expect(provider.name).toBe("test-provider");
  });
});

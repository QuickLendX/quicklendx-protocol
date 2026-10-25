/**
 * Unit Tests for Wallet Telemetry & Analytics Provider Module
 */

import {
  configureWalletTelemetry,
  createProviderFromConfig,
  CustomTelemetryProvider,
  getWalletTelemetryProvider,
  GoogleAnalyticsTelemetryProvider,
  hashAddress,
  mapErrorToReason,
  MixpanelTelemetryProvider,
  NoopTelemetryProvider,
  resetWalletTelemetry,
  resolveConfigFromEnv,
  sanitizeEventProps,
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

describe("Wallet Telemetry - Privacy & Hashing Utilities", () => {
  it("computes 16 lowercase hex characters for SHA-256('qlx:' + address)", async () => {
    const address = "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ";
    const hash = await hashAddress(address);
    // Precomputed SHA-256("qlx:" + address).slice(0, 16)
    expect(hash).toBe("a524b427d9e0f9e8");
    expect(hash).toHaveLength(16);
    expect(hash).toMatch(/^[0-9a-f]{16}$/);
  });

  it("returns empty string when address is empty", async () => {
    expect(await hashAddress("")).toBe("");
  });

  it("maps error messages to classified failure reasons", () => {
    expect(mapErrorToReason("User rejected the request (GBBX...)")).toBe(
      "rejected"
    );
    expect(mapErrorToReason("declined by user")).toBe("rejected");
    expect(mapErrorToReason("Freighter is not installed")).toBe(
      "not_installed"
    );
    expect(mapErrorToReason("Connection timed out after 60s")).toBe("timeout");
    expect(mapErrorToReason("SDK not found")).toBe("sdk_unavailable");
    expect(mapErrorToReason(new Error("Unknown internal crash"))).toBe(
      "unknown"
    );
    expect(mapErrorToReason(undefined)).toBe("unknown");
  });

  it("sanitizes event props, hashing addresses and dropping disallowed keys", async () => {
    const sanitized = await sanitizeEventProps("wallet_connected", {
      address: "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ",
      network: "testnet",
      email: "leaked@example.com",
    });

    expect(sanitized.address_hash).toBe("a524b427d9e0f9e8");
    expect(sanitized.network).toBe("testnet");
    expect((sanitized as any).address).toBeUndefined();
    expect((sanitized as any).email).toBeUndefined();
  });
});

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
      const debugSpy = jest
        .spyOn(console, "debug")
        .mockImplementation(() => {});
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

      await provider.trackEvent("wallet_connected", {
        address_hash: "a524b427d9e0f9e8",
        network: "testnet",
      });
      expect(mixpanelTrackMock).toHaveBeenCalledTimes(1);
      expect(mixpanelTrackMock).toHaveBeenCalledWith(
        "wallet_connected",
        expect.objectContaining({
          address_hash: "a524b427d9e0f9e8",
          network: "testnet",
          timestamp: expect.any(Number),
        })
      );

      provider.identify("hash_user_1", { is_admin: true });
      expect(mixpanelIdentifyMock).toHaveBeenCalledWith("hash_user_1");
      expect(mixpanelPeopleSetMock).toHaveBeenCalledWith({ is_admin: true });
    });

    it("forwards events via HTTP fetch when window.mixpanel is undefined", async () => {
      (global as any).window = undefined;
      const fetchMock = jest.fn().mockResolvedValue({ ok: true });
      (global as any).fetch = fetchMock;

      const provider = new MixpanelTelemetryProvider(
        "test_token_123",
        "https://api.mixpanel.com/track"
      );
      await provider.trackEvent("wallet_disconnected", {
        address_hash: "a524b427d9e0f9e8",
      });

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
    it("forwards events to window.gtag when available with send_to", () => {
      const gtagMock = jest.fn();
      (global as any).window = {
        gtag: gtagMock,
      };

      const provider = new GoogleAnalyticsTelemetryProvider("G-ABC123");
      expect(provider.name).toBe("google-analytics");

      provider.trackEvent("wallet_connect_attempt", {
        walletType: "freighter",
      });
      expect(gtagMock).toHaveBeenCalledWith(
        "event",
        "wallet_connect_attempt",
        expect.objectContaining({
          walletType: "freighter",
          send_to: "G-ABC123",
        })
      );

      provider.identify("a524b427d9e0f9e8", { role: "borrower" });
      expect(gtagMock).toHaveBeenCalledWith(
        "set",
        "user_properties",
        expect.objectContaining({
          user_id: "a524b427d9e0f9e8",
          role: "borrower",
        })
      );
    });

    it("pushes to dataLayer if gtag function is absent", () => {
      const dataLayerMock: any[] = [];
      (global as any).window = {
        dataLayer: dataLayerMock,
      };

      const provider = new GoogleAnalyticsTelemetryProvider("G-DEF456");
      provider.trackEvent("wallet_connect_failed", {
        reason: "sdk_unavailable",
      });

      expect(dataLayerMock.length).toBe(1);
      expect(dataLayerMock[0]).toEqual(
        expect.objectContaining({
          event: "wallet_connect_failed",
          reason: "sdk_unavailable",
          send_to: "G-DEF456",
        })
      );
    });
  });

  describe("SelfHostedTelemetryProvider", () => {
    it("posts JSON payload without Authorization header in any configuration", async () => {
      const fetchMock = jest.fn().mockResolvedValue({ ok: true });
      (global as any).fetch = fetchMock;

      const provider = new SelfHostedTelemetryProvider(
        "https://analytics.example.com/events"
      );
      expect(provider.name).toBe("self-hosted");

      await provider.trackEvent("wallet_connected", {
        address_hash: "a524b427d9e0f9e8",
      });

      expect(fetchMock).toHaveBeenCalledTimes(1);
      const [url, init] = fetchMock.mock.calls[0];
      expect(url).toBe("https://analytics.example.com/events");
      expect(init.method).toBe("POST");
      expect(init.headers).toEqual({
        "Content-Type": "application/json",
      });
      expect(init.headers.Authorization).toBeUndefined();
      expect(JSON.parse(init.body)).toEqual({
        event: "wallet_connected",
        properties: { address_hash: "a524b427d9e0f9e8" },
        timestamp: expect.any(Number),
      });
    });

    it("identify endpoint also sends no Authorization header", async () => {
      const fetchMock = jest.fn().mockResolvedValue({ ok: true });
      (global as any).fetch = fetchMock;

      const provider = new SelfHostedTelemetryProvider(
        "https://analytics.example.com/events"
      );
      await provider.identify("a524b427d9e0f9e8");

      expect(fetchMock).toHaveBeenCalledTimes(1);
      const [url, init] = fetchMock.mock.calls[0];
      expect(url).toBe("https://analytics.example.com/events/identify");
      expect(init.headers.Authorization).toBeUndefined();
    });

    it("silently catches network errors without throwing to caller", async () => {
      const warnSpy = jest.spyOn(console, "warn").mockImplementation(() => {});
      (global as any).fetch = jest
        .fn()
        .mockRejectedValue(new Error("Network Error"));

      const provider = new SelfHostedTelemetryProvider(
        "https://analytics.example.com/events"
      );
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
    delete process.env.NEXT_PUBLIC_ANALYTICS_ENABLED;
    delete process.env.NEXT_PUBLIC_ANALYTICS_PROVIDER;
    delete process.env.NEXT_PUBLIC_MIXPANEL_TOKEN;
    delete process.env.NEXT_PUBLIC_GA_MEASUREMENT_ID;
    delete process.env.NEXT_PUBLIC_ANALYTICS_ENDPOINT;
  });

  afterAll(() => {
    process.env = originalEnv;
  });

  it("with no analytics env and NODE_ENV=production, createProviderFromConfig(resolveConfigFromEnv()).name === 'noop'", () => {
    (process.env as Record<string, string | undefined>).NODE_ENV = "production";
    const config = resolveConfigFromEnv();
    expect(config.enabled).toBe(false);
    expect(config.provider).toBe("noop");

    const provider = createProviderFromConfig(config);
    expect(provider.name).toBe("noop");
  });

  it("with PROVIDER=ga and NEXT_PUBLIC_MIXPANEL_TOKEN=mp_123 but no GA id, provider is noop", () => {
    const warnSpy = jest.spyOn(console, "warn").mockImplementation(() => {});
    process.env.NEXT_PUBLIC_ANALYTICS_ENABLED = "true";
    process.env.NEXT_PUBLIC_ANALYTICS_PROVIDER = "ga";
    process.env.NEXT_PUBLIC_MIXPANEL_TOKEN = "mp_123";

    const config = resolveConfigFromEnv();
    expect(config.provider).toBe("noop");
    expect(config.enabled).toBe(false);

    const provider = createProviderFromConfig(config);
    expect(provider.name).toBe("noop");
    expect(warnSpy).toHaveBeenCalled();
    warnSpy.mockRestore();
  });

  it("with NEXT_PUBLIC_GA_MEASUREMENT_ID=G-ABC123, a gtag spy receives send_to: 'G-ABC123'", () => {
    process.env.NEXT_PUBLIC_ANALYTICS_ENABLED = "true";
    process.env.NEXT_PUBLIC_ANALYTICS_PROVIDER = "ga";
    process.env.NEXT_PUBLIC_GA_MEASUREMENT_ID = "G-ABC123";

    const config = resolveConfigFromEnv();
    expect(config.enabled).toBe(true);
    expect(config.provider).toBe("ga");

    const provider = createProviderFromConfig(config);
    expect(provider.name).toBe("google-analytics");

    const gtagSpy = jest.fn();
    (global as any).window = { gtag: gtagSpy };

    provider.trackEvent("wallet_connect_attempt", { walletType: "freighter" });
    expect(gtagSpy).toHaveBeenCalledWith(
      "event",
      "wallet_connect_attempt",
      expect.objectContaining({
        send_to: "G-ABC123",
        walletType: "freighter",
      })
    );
  });

  it("falls back to noop when GA measurement ID format is invalid", () => {
    const warnSpy = jest.spyOn(console, "warn").mockImplementation(() => {});
    process.env.NEXT_PUBLIC_ANALYTICS_ENABLED = "true";
    process.env.NEXT_PUBLIC_ANALYTICS_PROVIDER = "ga";
    process.env.NEXT_PUBLIC_GA_MEASUREMENT_ID = "invalid-format";

    const config = resolveConfigFromEnv();
    expect(config.provider).toBe("noop");
    const provider = createProviderFromConfig(config);
    expect(provider.name).toBe("noop");

    expect(warnSpy).toHaveBeenCalled();
    warnSpy.mockRestore();
  });

  it("resolves Mixpanel provider when NEXT_PUBLIC_MIXPANEL_TOKEN is set", () => {
    process.env.NEXT_PUBLIC_ANALYTICS_ENABLED = "true";
    process.env.NEXT_PUBLIC_ANALYTICS_PROVIDER = "mixpanel";
    process.env.NEXT_PUBLIC_MIXPANEL_TOKEN = "mp_token_xyz";

    const config = resolveConfigFromEnv();
    expect(config.enabled).toBe(true);
    expect(config.provider).toBe("mixpanel");
    expect(config.mixpanelToken).toBe("mp_token_xyz");

    const provider = createProviderFromConfig(config);
    expect(provider.name).toBe("mixpanel");
  });

  it("resolves self-hosted provider with valid HTTPS endpoint in production", () => {
    (process.env as Record<string, string | undefined>).NODE_ENV = "production";
    process.env.NEXT_PUBLIC_ANALYTICS_ENABLED = "true";
    process.env.NEXT_PUBLIC_ANALYTICS_PROVIDER = "self-hosted";
    process.env.NEXT_PUBLIC_ANALYTICS_ENDPOINT =
      "https://analytics.quicklendx.com/api/events";

    const config = resolveConfigFromEnv();
    expect(config.enabled).toBe(true);
    expect(config.provider).toBe("self-hosted");

    const provider = createProviderFromConfig(config);
    expect(provider.name).toBe("self-hosted");
  });

  it("falls back to noop when self-hosted endpoint is non-HTTPS in production", () => {
    (process.env as Record<string, string | undefined>).NODE_ENV = "production";
    process.env.NEXT_PUBLIC_ANALYTICS_ENABLED = "true";
    process.env.NEXT_PUBLIC_ANALYTICS_PROVIDER = "self-hosted";
    process.env.NEXT_PUBLIC_ANALYTICS_ENDPOINT =
      "http://insecure.quicklendx.com/events";

    const config = resolveConfigFromEnv();
    expect(config.enabled).toBe(false);
    expect(config.provider).toBe("noop");

    const provider = createProviderFromConfig(config);
    expect(provider.name).toBe("noop");
  });

  it("resolves to NoopTelemetryProvider when disabled", () => {
    process.env.NEXT_PUBLIC_ANALYTICS_ENABLED = "false";
    process.env.NEXT_PUBLIC_ANALYTICS_PROVIDER = "mixpanel";
    process.env.NEXT_PUBLIC_MIXPANEL_TOKEN = "mp_token_xyz";

    const config = resolveConfigFromEnv();
    expect(config.enabled).toBe(false);
    expect(config.provider).toBe("noop");

    const provider = createProviderFromConfig(config);
    expect(provider.name).toBe("noop");
  });
});

describe("Wallet Telemetry - Service & Event Forwarding", () => {
  beforeEach(() => {
    resetWalletTelemetry();
  });

  it("forwards events with hashed address when telemetry is enabled", async () => {
    const mockTrackEvent = jest.fn();
    const customProvider: WalletTelemetryProvider = {
      name: "mock-provider",
      trackEvent: mockTrackEvent,
    };

    const service = new WalletTelemetryService({ enabled: true });
    service.setProvider(customProvider);

    await service.trackEvent("wallet_connected", {
      address: "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ",
      network: "testnet",
    });

    expect(mockTrackEvent).toHaveBeenCalledTimes(1);
    expect(mockTrackEvent).toHaveBeenCalledWith(
      "wallet_connected",
      expect.objectContaining({
        address_hash: "a524b427d9e0f9e8",
        network: "testnet",
        timestamp: expect.any(Number),
      })
    );
    // Address key must not be forwarded
    const payload = mockTrackEvent.mock.calls[0][1];
    expect(payload.address).toBeUndefined();
    expect(JSON.stringify(payload)).not.toContain("GBBXKXTNVC");
  });

  it("drops unknown property keys like email from event payload", async () => {
    const mockTrackEvent = jest.fn();
    const customProvider: WalletTelemetryProvider = {
      name: "mock-provider",
      trackEvent: mockTrackEvent,
    };

    const service = new WalletTelemetryService({ enabled: true });
    service.setProvider(customProvider);

    await service.trackEvent("wallet_connected", {
      address: "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ",
      network: "testnet",
      email: "x@y.z",
      creditScore: 800,
    });

    const payload = mockTrackEvent.mock.calls[0][1];
    expect(payload.email).toBeUndefined();
    expect(payload.creditScore).toBeUndefined();
    expect(payload.address_hash).toBe("a524b427d9e0f9e8");
    expect(payload.network).toBe("testnet");
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

  it("preserves an injected provider during ordinary configuration updates", async () => {
    const mockTrackEvent = jest.fn();
    const service = new WalletTelemetryService({ enabled: true });

    service.setProvider({
      name: "test-provider",
      trackEvent: mockTrackEvent,
    });

    service.configure({ enabled: true });

    await service.trackEvent("wallet_connected");

    expect(service.getProvider().name).toBe("test-provider");
    expect(mockTrackEvent).toHaveBeenCalledTimes(1);
    expect(mockTrackEvent).toHaveBeenCalledWith(
      "wallet_connected",
      expect.objectContaining({
        timestamp: expect.any(Number),
      })
    );
  });

  it("replaces an injected provider when configuration explicitly overrides it", async () => {
    const mockTrackEvent = jest.fn();
    const service = new WalletTelemetryService({ enabled: true });

    service.setProvider({
      name: "test-provider",
      trackEvent: mockTrackEvent,
    });

    service.configure({ enabled: true, provider: "noop" });

    await service.trackEvent("wallet_connected");

    expect(service.getProvider().name).toBe("noop");
    expect(mockTrackEvent).not.toHaveBeenCalled();
  });

  it("does not track or identify when telemetry is disabled with an injected provider", async () => {
    const mockTrackEvent = jest.fn();
    const mockIdentify = jest.fn();
    const service = new WalletTelemetryService({ enabled: true });

    service.setProvider({
      name: "test-provider",
      trackEvent: mockTrackEvent,
      identify: mockIdentify,
    });

    service.configure({ enabled: false });

    await service.trackEvent("wallet_connected");
    await service.identify("user-123");

    expect(mockTrackEvent).not.toHaveBeenCalled();
    expect(mockIdentify).not.toHaveBeenCalled();
  });

  it("clears the injected provider state when reset is called", () => {
    const service = new WalletTelemetryService({ enabled: true });

    service.setProvider({
      name: "test-provider",
      trackEvent: jest.fn(),
    });

    service.reset();

    expect(service.getProvider().name).toBe("noop");
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

  it("hashes userId when identify is called", async () => {
    const mockIdentify = jest.fn();
    const customProvider: WalletTelemetryProvider = {
      name: "mock-provider",
      trackEvent: jest.fn(),
      identify: mockIdentify,
    };

    const service = new WalletTelemetryService({ enabled: true });
    service.setProvider(customProvider);

    const rawAddress = "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ";
    await service.identify(rawAddress, { plan: "pro" });

    expect(mockIdentify).toHaveBeenCalledWith("a524b427d9e0f9e8", {
      plan: "pro",
    });
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

  it("trackWalletEvent sends generic event and drops unknown props on typed events", async () => {
    await trackWalletEvent("custom_wallet_action", { step: 1 });
    expect(mockTrackEvent).toHaveBeenCalledWith(
      "custom_wallet_action",
      expect.objectContaining({ step: 1 })
    );

    // Unknown property passed to typed event is dropped
    await trackWalletEvent("wallet_connected", {
      address: "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ",
      network: "testnet",
      email: "x@y.z",
    });
    const lastPayload = mockTrackEvent.mock.calls[1][1];
    expect(lastPayload.email).toBeUndefined();
    expect(lastPayload.address).toBeUndefined();
    expect(lastPayload.address_hash).toBe("a524b427d9e0f9e8");
  });

  it("trackWalletConnectAttempt forwards event with correct name and props", async () => {
    await trackWalletConnectAttempt({
      walletType: "freighter",
      network: "testnet",
    });
    expect(mockTrackEvent).toHaveBeenCalledWith(
      "wallet_connect_attempt",
      expect.objectContaining({
        walletType: "freighter",
        network: "testnet",
      })
    );
  });

  it("trackWalletConnected reaches provider with address_hash equal to precomputed fixture and no address key", async () => {
    const A = "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ";
    await trackWalletConnected({
      address: A,
      walletType: "freighter",
      network: "testnet",
    });

    expect(mockTrackEvent).toHaveBeenCalledTimes(1);
    const [eventName, payload] = mockTrackEvent.mock.calls[0];
    expect(eventName).toBe("wallet_connected");
    expect(payload.address_hash).toBe("a524b427d9e0f9e8");
    expect(payload.address).toBeUndefined();
    expect(payload.network).toBe("testnet");
    expect(payload.walletType).toBe("freighter");

    const jsonStr = JSON.stringify(payload);
    expect(jsonStr).not.toContain(A);
  });

  it("trackWalletConnectFailed sends mapped reason and omits raw error message", async () => {
    await trackWalletConnectFailed({
      error:
        "User rejected the request (GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ)",
      walletType: "freighter",
      network: "testnet",
    });

    expect(mockTrackEvent).toHaveBeenCalledTimes(1);
    const [eventName, payload] = mockTrackEvent.mock.calls[0];
    expect(eventName).toBe("wallet_connect_failed");
    expect(payload.reason).toBe("rejected");
    expect(payload.error).toBeUndefined();
    expect(payload.walletType).toBe("freighter");
    expect(payload.network).toBe("testnet");

    const jsonStr = JSON.stringify(payload);
    expect(jsonStr).not.toContain("GBBX");
  });

  it("trackWalletDisconnected forwards disconnect event with address_hash", async () => {
    await trackWalletDisconnected({
      address: "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ",
      reason: "manual_disconnect",
    });

    expect(mockTrackEvent).toHaveBeenCalledWith(
      "wallet_disconnected",
      expect.objectContaining({
        address_hash: "a524b427d9e0f9e8",
        reason: "manual_disconnect",
      })
    );
    const payload = mockTrackEvent.mock.calls[0][1];
    expect(payload.address).toBeUndefined();
  });

  it("trackWalletNetworkChanged forwards old and new network with optional address_hash", async () => {
    await trackWalletNetworkChanged({
      network: "public",
      previousNetwork: "testnet",
      address: "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ",
    });

    expect(mockTrackEvent).toHaveBeenCalledWith(
      "wallet_network_changed",
      expect.objectContaining({
        network: "public",
        previousNetwork: "testnet",
        address_hash: "a524b427d9e0f9e8",
      })
    );
    const payload = mockTrackEvent.mock.calls[0][1];
    expect(payload.address).toBeUndefined();
  });

  it("getWalletTelemetryProvider returns current provider", () => {
    const provider = getWalletTelemetryProvider();
    expect(provider.name).toBe("test-provider");
  });
});

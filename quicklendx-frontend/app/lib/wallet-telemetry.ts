/**
 * Wallet Telemetry & Analytics Provider Module
 *
 * Provides a configurable telemetry provider interface with real implementations
 * (Mixpanel, Google Analytics, Self-Hosted HTTP) and fallback No-op for
 * development / disabled states.
 */

// ---------------------------------------------------------------------------
// Types & Interfaces
// ---------------------------------------------------------------------------

export type TelemetryProviderType =
  | "noop"
  | "mixpanel"
  | "ga"
  | "google-analytics"
  | "self-hosted"
  | "custom";

export interface WalletTelemetryEventProps {
  [key: string]: unknown;
}

export interface WalletTelemetryProvider {
  /** Provider identifier */
  readonly name: string;
  /** Track an analytics / telemetry event */
  trackEvent(name: string, props?: Record<string, unknown>): Promise<void> | void;
  /** Optional identify user */
  identify?(userId: string, traits?: Record<string, unknown>): Promise<void> | void;
  /** Optional reset session / identity */
  reset?(): Promise<void> | void;
}

export interface WalletTelemetryConfig {
  /** Explicit enable/disable toggle */
  enabled?: boolean;
  /** Selected provider type */
  provider?: TelemetryProviderType;
  /** Self-hosted or backend analytics endpoint */
  endpoint?: string;
  /** Mixpanel token or tracking ID */
  apiKey?: string;
  /** Enable debug console logging */
  debug?: boolean;
  /** Default metadata properties attached to all events */
  defaultProperties?: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Window Declarations for Browser Analytics Libraries
// ---------------------------------------------------------------------------

declare global {
  interface Window {
    mixpanel?: {
      track: (eventName: string, properties?: Record<string, unknown>) => void;
      identify?: (id: string) => void;
      people?: {
        set: (props: Record<string, unknown>) => void;
      };
    };
    gtag?: (...args: any[]) => void;
    dataLayer?: any[];
  }
}

// ---------------------------------------------------------------------------
// Provider Implementations
// ---------------------------------------------------------------------------

/**
 * No-Op Telemetry Provider
 * Used in development or when analytics is disabled.
 */
export class NoopTelemetryProvider implements WalletTelemetryProvider {
  readonly name = "noop";

  constructor(private readonly debug: boolean = false) {}

  trackEvent(name: string, props?: Record<string, unknown>): void {
    if (this.debug) {
      console.debug(`[wallet-telemetry:noop] trackEvent: ${name}`, props);
    }
  }

  identify(userId: string, traits?: Record<string, unknown>): void {
    if (this.debug) {
      console.debug(`[wallet-telemetry:noop] identify: ${userId}`, traits);
    }
  }

  reset(): void {
    if (this.debug) {
      console.debug("[wallet-telemetry:noop] reset");
    }
  }
}

/**
 * Mixpanel Telemetry Provider
 * Forwards events to window.mixpanel or via configured API endpoint.
 */
export class MixpanelTelemetryProvider implements WalletTelemetryProvider {
  readonly name = "mixpanel";

  constructor(
    private readonly token?: string,
    private readonly endpoint: string = "https://api.mixpanel.com/track"
  ) {}

  async trackEvent(name: string, props?: Record<string, unknown>): Promise<void> {
    const payload = {
      ...props,
      timestamp: Date.now(),
    };

    if (typeof window !== "undefined" && window.mixpanel && typeof window.mixpanel.track === "function") {
      window.mixpanel.track(name, payload);
      return;
    }

    if (this.token && typeof fetch !== "undefined") {
      try {
        const body = JSON.stringify([
          {
            event: name,
            properties: {
              token: this.token,
              ...payload,
            },
          },
        ]);
        await fetch(this.endpoint, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body,
        });
      } catch (err) {
        console.warn(`[wallet-telemetry:mixpanel] Failed to send event "${name}":`, err);
      }
    }
  }

  identify(userId: string, traits?: Record<string, unknown>): void {
    if (typeof window !== "undefined" && window.mixpanel) {
      window.mixpanel.identify?.(userId);
      if (traits && window.mixpanel.people) {
        window.mixpanel.people.set(traits);
      }
    }
  }

  reset(): void {
    // Optional mixpanel reset
  }
}

/**
 * Google Analytics (GA4 / gtag) Telemetry Provider
 */
export class GoogleAnalyticsTelemetryProvider implements WalletTelemetryProvider {
  readonly name = "google-analytics";

  constructor(private readonly measurementId?: string) {}

  trackEvent(name: string, props?: Record<string, unknown>): void {
    const eventPayload = {
      ...props,
      send_to: this.measurementId,
    };

    if (typeof window !== "undefined" && typeof window.gtag === "function") {
      window.gtag("event", name, eventPayload);
    } else if (typeof window !== "undefined" && Array.isArray(window.dataLayer)) {
      window.dataLayer.push({
        event: name,
        ...eventPayload,
      });
    }
  }

  identify(userId: string, traits?: Record<string, unknown>): void {
    if (typeof window !== "undefined" && typeof window.gtag === "function") {
      window.gtag("set", "user_properties", {
        user_id: userId,
        ...traits,
      });
    }
  }

  reset(): void {
    // Optional GA reset
  }
}

/**
 * Self-Hosted HTTP Analytics Service Provider
 * Sends telemetry events directly to QuickLendX's analytics service / endpoint.
 */
export class SelfHostedTelemetryProvider implements WalletTelemetryProvider {
  readonly name = "self-hosted";

  constructor(
    private readonly endpoint: string = "/api/analytics/events",
    private readonly apiKey?: string
  ) {}

  async trackEvent(name: string, props?: Record<string, unknown>): Promise<void> {
    if (typeof fetch === "undefined") {
      return;
    }

    try {
      const headers: Record<string, string> = {
        "Content-Type": "application/json",
      };
      if (this.apiKey) {
        headers["Authorization"] = `Bearer ${this.apiKey}`;
      }

      await fetch(this.endpoint, {
        method: "POST",
        headers,
        body: JSON.stringify({
          event: name,
          properties: props || {},
          timestamp: Date.now(),
        }),
      });
    } catch (err) {
      // Telemetry errors should not break application flow
      console.warn(`[wallet-telemetry:self-hosted] Failed to forward event "${name}":`, err);
    }
  }

  async identify(userId: string, traits?: Record<string, unknown>): Promise<void> {
    if (typeof fetch === "undefined") return;

    try {
      await fetch(`${this.endpoint}/identify`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          userId,
          traits: traits || {},
          timestamp: Date.now(),
        }),
      });
    } catch (err) {
      console.warn("[wallet-telemetry:self-hosted] Failed to identify user:", err);
    }
  }

  reset(): void {
    // Noop for self-hosted
  }
}

/**
 * Custom Telemetry Provider
 * Allows runtime injection of a custom callback handler.
 */
export class CustomTelemetryProvider implements WalletTelemetryProvider {
  readonly name = "custom";

  constructor(
    private readonly handler: (name: string, props?: Record<string, unknown>) => Promise<void> | void
  ) {}

  async trackEvent(name: string, props?: Record<string, unknown>): Promise<void> {
    await this.handler(name, props);
  }
}

// ---------------------------------------------------------------------------
// Configuration Resolver
// ---------------------------------------------------------------------------

/**
 * Reads telemetry configuration from environment variables.
 */
export function resolveConfigFromEnv(): WalletTelemetryConfig {
  const envEnabled =
    typeof process !== "undefined"
      ? process.env.NEXT_PUBLIC_ANALYTICS_ENABLED ?? process.env.ANALYTICS_ENABLED
      : undefined;

  const nodeEnv = typeof process !== "undefined" ? process.env.NODE_ENV : "development";

  // Enabled determination: explicit env var > nodeEnv (disabled in dev/test by default unless explicit)
  const enabled =
    envEnabled !== undefined
      ? envEnabled === "true" || envEnabled === "1"
      : nodeEnv === "production";

  const rawProvider =
    typeof process !== "undefined"
      ? (process.env.NEXT_PUBLIC_ANALYTICS_PROVIDER ??
         process.env.ANALYTICS_PROVIDER ??
         "self-hosted").toLowerCase()
      : "self-hosted";

  let provider: TelemetryProviderType = "noop";
  if (rawProvider === "mixpanel") {
    provider = "mixpanel";
  } else if (rawProvider === "ga" || rawProvider === "google-analytics") {
    provider = "ga";
  } else if (rawProvider === "self-hosted") {
    provider = "self-hosted";
  } else if (rawProvider === "noop") {
    provider = "noop";
  } else {
    provider = "self-hosted";
  }

  const endpoint =
    typeof process !== "undefined"
      ? process.env.NEXT_PUBLIC_ANALYTICS_ENDPOINT ??
        process.env.ANALYTICS_ENDPOINT ??
        "/api/analytics/events"
      : "/api/analytics/events";

  const apiKey =
    typeof process !== "undefined"
      ? process.env.NEXT_PUBLIC_MIXPANEL_TOKEN ??
        process.env.MIXPANEL_TOKEN ??
        process.env.NEXT_PUBLIC_ANALYTICS_API_KEY ??
        process.env.ANALYTICS_API_KEY
      : undefined;

  return {
    enabled,
    provider,
    endpoint,
    apiKey,
    debug: nodeEnv === "development",
  };
}

/**
 * Instantiates the appropriate telemetry provider based on configuration.
 */
export function createProviderFromConfig(config: WalletTelemetryConfig): WalletTelemetryProvider {
  if (!config.enabled || config.provider === "noop") {
    return new NoopTelemetryProvider(config.debug);
  }

  switch (config.provider) {
    case "mixpanel":
      return new MixpanelTelemetryProvider(config.apiKey, config.endpoint);
    case "ga":
    case "google-analytics":
      return new GoogleAnalyticsTelemetryProvider(config.apiKey);
    case "self-hosted":
      return new SelfHostedTelemetryProvider(config.endpoint, config.apiKey);
    default:
      return new SelfHostedTelemetryProvider(config.endpoint, config.apiKey);
  }
}

// ---------------------------------------------------------------------------
// Wallet Telemetry Service (Manager)
// ---------------------------------------------------------------------------

export class WalletTelemetryService {
  private config: WalletTelemetryConfig;
  private provider: WalletTelemetryProvider;

  constructor(initialConfig?: Partial<WalletTelemetryConfig>) {
    const envConfig = resolveConfigFromEnv();
    this.config = { ...envConfig, ...initialConfig };
    this.provider = createProviderFromConfig(this.config);
  }

  /** Update runtime configuration and instantiate corresponding provider */
  configure(config: Partial<WalletTelemetryConfig>): void {
    this.config = { ...this.config, ...config };
    this.provider = createProviderFromConfig(this.config);
  }

  /** Explicitly set a custom provider instance */
  setProvider(provider: WalletTelemetryProvider): void {
    this.provider = provider;
  }

  /** Get active provider instance */
  getProvider(): WalletTelemetryProvider {
    return this.provider;
  }

  /** Get active config */
  getConfig(): WalletTelemetryConfig {
    return { ...this.config };
  }

  /** Check if telemetry is actively enabled */
  isEnabled(): boolean {
    return Boolean(this.config.enabled) && this.provider.name !== "noop";
  }

  /** Reset service to defaults (useful in testing) */
  reset(): void {
    const envConfig = resolveConfigFromEnv();
    this.config = envConfig;
    this.provider = createProviderFromConfig(this.config);
  }

  /** Track event through active provider */
  async trackEvent(name: string, props?: Record<string, unknown>): Promise<void> {
    if (!this.config.enabled && this.provider.name === "noop") {
      return;
    }

    const payload = {
      ...this.config.defaultProperties,
      ...props,
      timestamp: Date.now(),
    };

    try {
      await this.provider.trackEvent(name, payload);
    } catch (err) {
      console.warn(`[wallet-telemetry] Error tracking event "${name}":`, err);
    }
  }

  /** Identify user session */
  async identify(userId: string, traits?: Record<string, unknown>): Promise<void> {
    if (!this.config.enabled && this.provider.name === "noop") return;
    try {
      await this.provider.identify?.(userId, traits);
    } catch (err) {
      console.warn("[wallet-telemetry] Error identifying user:", err);
    }
  }
}

// ---------------------------------------------------------------------------
// Singleton Instance & Exported Helper Functions
// ---------------------------------------------------------------------------

export const walletTelemetry = new WalletTelemetryService();

/**
 * Configure the global wallet telemetry service
 */
export function configureWalletTelemetry(config: Partial<WalletTelemetryConfig>): void {
  walletTelemetry.configure(config);
}

/**
 * Set a custom telemetry provider instance
 */
export function setWalletTelemetryProvider(provider: WalletTelemetryProvider): void {
  walletTelemetry.setProvider(provider);
}

/**
 * Get current telemetry provider instance
 */
export function getWalletTelemetryProvider(): WalletTelemetryProvider {
  return walletTelemetry.getProvider();
}

/**
 * Reset wallet telemetry service to default environment configuration
 */
export function resetWalletTelemetry(): void {
  walletTelemetry.reset();
}

/**
 * Generic wallet event tracker
 */
export async function trackWalletEvent(
  name: string,
  props?: Record<string, unknown>
): Promise<void> {
  await walletTelemetry.trackEvent(name, props);
}

// ---------------------------------------------------------------------------
// Typed Specific Wallet Lifecycle Event Helpers
// ---------------------------------------------------------------------------

export interface WalletConnectAttemptProps {
  walletType?: string;
  network?: string;
  source?: string;
  [key: string]: unknown;
}

export interface WalletConnectedProps {
  address: string;
  walletType?: string;
  network?: string;
  source?: string;
  [key: string]: unknown;
}

export interface WalletConnectFailedProps {
  error: string;
  walletType?: string;
  network?: string;
  reason?: string;
  [key: string]: unknown;
}

export interface WalletDisconnectedProps {
  address?: string;
  network?: string;
  reason?: string;
  [key: string]: unknown;
}

export interface WalletNetworkChangedProps {
  network: string;
  previousNetwork?: string;
  address?: string;
  [key: string]: unknown;
}

/**
 * Emitted when a connection attempt begins
 */
export async function trackWalletConnectAttempt(props?: WalletConnectAttemptProps): Promise<void> {
  await trackWalletEvent("wallet_connect_attempt", props);
}

/**
 * Emitted when a wallet is successfully connected
 */
export async function trackWalletConnected(props: WalletConnectedProps): Promise<void> {
  await trackWalletEvent("wallet_connected", props);
}

/**
 * Emitted when a wallet connection attempt fails or is rejected
 */
export async function trackWalletConnectFailed(props: WalletConnectFailedProps): Promise<void> {
  await trackWalletEvent("wallet_connect_failed", props);
}

/**
 * Emitted when a wallet is disconnected
 */
export async function trackWalletDisconnected(props?: WalletDisconnectedProps): Promise<void> {
  await trackWalletEvent("wallet_disconnected", props);
}

/**
 * Emitted when the wallet network changes
 */
export async function trackWalletNetworkChanged(props: WalletNetworkChangedProps): Promise<void> {
  await trackWalletEvent("wallet_network_changed", props);
}

/**
 * Wallet Telemetry & Analytics Provider Module
 *
 * Provides a configurable, privacy-preserving telemetry provider interface with
 * implementations for Mixpanel, Google Analytics (GA4), Self-Hosted HTTP,
 * and fallback No-op for development or disabled states.
 *
 * Privacy Guarantees:
 * - Telemetry is opt-in only (NEXT_PUBLIC_ANALYTICS_ENABLED="true").
 * - Raw Stellar addresses are never transmitted; SHA-256("qlx:" + address) 16-hex hashes are used instead.
 * - Raw extension error messages are never transmitted; only classified failure reasons are allowed.
 * - Unknown or sensitive event properties outside each event's typed allowlist are dropped.
 * - No Authorization / Bearer tokens are sent from client-side bundles.
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

export type WalletFailureReason =
  | "not_installed"
  | "rejected"
  | "timeout"
  | "sdk_unavailable"
  | "unknown";

export interface WalletTelemetryEventProps {
  [key: string]: unknown;
}

export interface WalletTelemetryProvider {
  /** Provider identifier */
  readonly name: string;
  /** Track an analytics / telemetry event */
  trackEvent(
    name: string,
    props?: Record<string, unknown>
  ): Promise<void> | void;
  /** Optional identify user */
  identify?(
    userId: string,
    traits?: Record<string, unknown>
  ): Promise<void> | void;
  /** Optional reset session / identity */
  reset?(): Promise<void> | void;
}

export interface WalletTelemetryConfig {
  /** Explicit enable/disable toggle (opt-in only) */
  enabled?: boolean;
  /** Selected provider type */
  provider?: TelemetryProviderType;
  /** Self-hosted analytics endpoint (required for self-hosted provider) */
  endpoint?: string;
  /** Mixpanel token or tracking ID (backward compatible alias) */
  apiKey?: string;
  /** Mixpanel project token */
  mixpanelToken?: string;
  /** Google Analytics measurement ID (e.g. G-XXXXX) */
  gaMeasurementId?: string;
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
// Hashing & Sanitization Utilities
// ---------------------------------------------------------------------------

/**
 * Computes a privacy-preserving address hash:
 * The first 16 lowercase hex characters of SHA-256("qlx:" + address).
 */
export async function hashAddress(address: string): Promise<string> {
  if (!address) return "";
  const encoder = new TextEncoder();
  const data = encoder.encode(`qlx:${address}`);

  let subtle: SubtleCrypto | undefined = globalThis.crypto?.subtle;
  if (!subtle && typeof window !== "undefined") {
    subtle = window.crypto?.subtle;
  }
  if (!subtle) {
    try {
      const nodeCrypto = require("node:crypto");
      subtle = nodeCrypto.webcrypto?.subtle;
    } catch {
      // ignore
    }
  }

  if (!subtle) {
    throw new Error("crypto.subtle is not available");
  }

  const hashBuffer = await subtle.digest("SHA-256", data);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  const hex = hashArray.map((b) => b.toString(16).padStart(2, "0")).join("");
  return hex.slice(0, 16);
}

const VALID_FAILURE_REASONS: ReadonlySet<WalletFailureReason> =
  new Set<WalletFailureReason>([
    "not_installed",
    "rejected",
    "timeout",
    "sdk_unavailable",
    "unknown",
  ]);

/**
 * Maps an error string or Error instance to a safe WalletFailureReason enum.
 * Never leaks the raw error message.
 */
export function mapErrorToReason(error?: unknown): WalletFailureReason {
  if (!error) return "unknown";
  if (
    typeof error === "string" &&
    VALID_FAILURE_REASONS.has(error as WalletFailureReason)
  ) {
    return error as WalletFailureReason;
  }

  const errStr = (
    typeof error === "string"
      ? error
      : (error as any)?.message || (error as any)?.code || String(error)
  ).toLowerCase();

  if (
    errStr.includes("not_installed") ||
    errStr.includes("not installed") ||
    errStr.includes("freighter is not installed")
  ) {
    return "not_installed";
  }
  if (
    errStr.includes("rejected") ||
    errStr.includes("declined") ||
    errStr.includes("denied") ||
    errStr.includes("cancel")
  ) {
    return "rejected";
  }
  if (errStr.includes("timeout") || errStr.includes("timed out")) {
    return "timeout";
  }
  if (
    errStr.includes("sdk_unavailable") ||
    errStr.includes("sdk unavailable") ||
    errStr.includes("sdk_not_found") ||
    errStr.includes("sdk not found")
  ) {
    return "sdk_unavailable";
  }
  return "unknown";
}

/**
 * Strict allowlist of property keys allowed for each standard wallet event.
 */
export const EVENT_PROPERTY_ALLOWLIST: Record<string, ReadonlySet<string>> = {
  wallet_connect_attempt: new Set([
    "walletType",
    "network",
    "source",
    "timestamp",
  ]),
  wallet_connected: new Set([
    "address_hash",
    "walletType",
    "network",
    "source",
    "timestamp",
  ]),
  wallet_connect_failed: new Set([
    "reason",
    "walletType",
    "network",
    "timestamp",
  ]),
  wallet_disconnected: new Set([
    "address_hash",
    "network",
    "reason",
    "timestamp",
  ]),
  wallet_network_changed: new Set([
    "network",
    "previousNetwork",
    "address_hash",
    "timestamp",
  ]),
};

/**
 * Sanitizes event properties according to event allowlist rules.
 * - Hashes any raw address to address_hash.
 * - Maps any raw error to a safe failure reason.
 * - Drops unknown property keys.
 */
export async function sanitizeEventProps(
  name: string,
  props?: Record<string, unknown>,
  defaultProps?: Record<string, unknown>
): Promise<Record<string, unknown>> {
  const merged: Record<string, unknown> = {
    ...(defaultProps || {}),
    ...(props || {}),
  };

  const allowedKeys = EVENT_PROPERTY_ALLOWLIST[name];
  const defaultKeys = defaultProps ? Object.keys(defaultProps) : [];
  const sanitized: Record<string, unknown> = {};

  // Process address -> address_hash
  if ("address" in merged && merged.address) {
    try {
      sanitized.address_hash = await hashAddress(String(merged.address));
    } catch {
      // ignore
    }
  } else if ("address_hash" in merged && merged.address_hash) {
    sanitized.address_hash = String(merged.address_hash);
  }

  // Process error / reason for wallet_connect_failed
  if (name === "wallet_connect_failed") {
    if ("error" in merged && merged.error) {
      sanitized.reason = mapErrorToReason(merged.error);
    } else if ("reason" in merged && merged.reason) {
      sanitized.reason = mapErrorToReason(merged.reason);
    } else {
      sanitized.reason = "unknown";
    }
  }

  for (const [key, value] of Object.entries(merged)) {
    // Explicitly forbidden raw identifier keys
    if (key === "address" || key === "error") {
      continue;
    }
    // If the event has a defined allowlist, keep only allowed or configured default keys
    if (allowedKeys) {
      if (allowedKeys.has(key) || defaultKeys.includes(key)) {
        sanitized[key] = value;
      }
    } else {
      // Custom event without a strict predefined allowlist
      sanitized[key] = value;
    }
  }

  return sanitized;
}

// ---------------------------------------------------------------------------
// Provider Implementations
// ---------------------------------------------------------------------------

/**
 * No-Op Telemetry Provider
 * Used when analytics is disabled or unconfigured.
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
 * Requires NEXT_PUBLIC_MIXPANEL_TOKEN.
 */
export class MixpanelTelemetryProvider implements WalletTelemetryProvider {
  readonly name = "mixpanel";

  constructor(
    private readonly token?: string,
    private readonly endpoint: string = "https://api.mixpanel.com/track"
  ) {}

  async trackEvent(
    name: string,
    props?: Record<string, unknown>
  ): Promise<void> {
    const payload = {
      ...props,
      timestamp: Date.now(),
    };

    if (
      typeof window !== "undefined" &&
      window.mixpanel &&
      typeof window.mixpanel.track === "function"
    ) {
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
        console.warn(
          `[wallet-telemetry:mixpanel] Failed to send event "${name}":`,
          err
        );
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
 * Requires NEXT_PUBLIC_GA_MEASUREMENT_ID matching /^G-[A-Z0-9]{4,}$/.
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
    } else if (
      typeof window !== "undefined" &&
      Array.isArray(window.dataLayer)
    ) {
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
 * Sends no Authorization header in any configuration.
 */
export class SelfHostedTelemetryProvider implements WalletTelemetryProvider {
  readonly name = "self-hosted";

  constructor(private readonly endpoint: string = "/api/analytics/events") {}

  async trackEvent(
    name: string,
    props?: Record<string, unknown>
  ): Promise<void> {
    if (typeof fetch === "undefined") {
      return;
    }

    try {
      const headers: Record<string, string> = {
        "Content-Type": "application/json",
      };

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
      console.warn(
        `[wallet-telemetry:self-hosted] Failed to forward event "${name}":`,
        err
      );
    }
  }

  async identify(
    userId: string,
    traits?: Record<string, unknown>
  ): Promise<void> {
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
      console.warn(
        "[wallet-telemetry:self-hosted] Failed to identify user:",
        err
      );
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
    private readonly handler: (
      name: string,
      props?: Record<string, unknown>
    ) => Promise<void> | void
  ) {}

  async trackEvent(
    name: string,
    props?: Record<string, unknown>
  ): Promise<void> {
    await this.handler(name, props);
  }
}

// ---------------------------------------------------------------------------
// Configuration Resolver & Factory
// ---------------------------------------------------------------------------

const GA_MEASUREMENT_ID_REGEX = /^G-[A-Z0-9]{4,}$/;

/**
 * Reads telemetry configuration strictly from public environment variables.
 * Telemetry is opt-in only: enabled ONLY when NEXT_PUBLIC_ANALYTICS_ENABLED === "true".
 */
export function resolveConfigFromEnv(): WalletTelemetryConfig {
  const envEnabled =
    typeof process !== "undefined"
      ? process.env.NEXT_PUBLIC_ANALYTICS_ENABLED
      : undefined;

  const nodeEnv =
    typeof process !== "undefined" ? process.env.NODE_ENV : "development";

  if (envEnabled !== "true") {
    return {
      enabled: false,
      provider: "noop",
      debug: nodeEnv === "development",
    };
  }

  const rawProvider = (
    typeof process !== "undefined"
      ? (process.env.NEXT_PUBLIC_ANALYTICS_PROVIDER ?? "noop")
      : "noop"
  ).toLowerCase();

  const mixpanelToken =
    typeof process !== "undefined"
      ? process.env.NEXT_PUBLIC_MIXPANEL_TOKEN
      : undefined;

  const gaMeasurementId =
    typeof process !== "undefined"
      ? process.env.NEXT_PUBLIC_GA_MEASUREMENT_ID
      : undefined;

  const endpoint =
    typeof process !== "undefined"
      ? process.env.NEXT_PUBLIC_ANALYTICS_ENDPOINT
      : undefined;

  let provider: TelemetryProviderType = "noop";

  if (rawProvider === "mixpanel") {
    if (mixpanelToken) {
      provider = "mixpanel";
    } else {
      provider = "noop";
    }
  } else if (rawProvider === "ga" || rawProvider === "google-analytics") {
    if (gaMeasurementId && GA_MEASUREMENT_ID_REGEX.test(gaMeasurementId)) {
      provider = "ga";
    } else {
      console.warn(
        `[wallet-telemetry] Invalid or missing Google Analytics measurement ID: "${gaMeasurementId}". Falling back to noop.`
      );
      provider = "noop";
    }
  } else if (rawProvider === "self-hosted") {
    if (!endpoint) {
      provider = "noop";
    } else if (nodeEnv === "production" && !endpoint.startsWith("https:")) {
      provider = "noop";
    } else {
      provider = "self-hosted";
    }
  } else {
    provider = "noop";
  }

  return {
    enabled: provider !== "noop",
    provider,
    endpoint,
    apiKey: mixpanelToken,
    mixpanelToken,
    gaMeasurementId,
    debug: nodeEnv === "development",
  };
}

/**
 * Instantiates the appropriate telemetry provider based on configuration.
 */
export function createProviderFromConfig(
  config: WalletTelemetryConfig
): WalletTelemetryProvider {
  if (!config.enabled || config.provider === "noop" || !config.provider) {
    return new NoopTelemetryProvider(config.debug);
  }

  switch (config.provider) {
    case "mixpanel": {
      const token = config.mixpanelToken ?? config.apiKey;
      if (!token) {
        return new NoopTelemetryProvider(config.debug);
      }
      return new MixpanelTelemetryProvider(token, config.endpoint);
    }
    case "ga":
    case "google-analytics": {
      const measurementId = config.gaMeasurementId ?? config.apiKey;
      if (!measurementId || !GA_MEASUREMENT_ID_REGEX.test(measurementId)) {
        console.warn(
          `[wallet-telemetry] Invalid or missing Google Analytics measurement ID: "${measurementId}". Falling back to noop.`
        );
        return new NoopTelemetryProvider(config.debug);
      }
      return new GoogleAnalyticsTelemetryProvider(measurementId);
    }
    case "self-hosted": {
      if (!config.endpoint) {
        return new NoopTelemetryProvider(config.debug);
      }
      const nodeEnv =
        typeof process !== "undefined" ? process.env.NODE_ENV : "development";
      if (nodeEnv === "production" && !config.endpoint.startsWith("https:")) {
        return new NoopTelemetryProvider(config.debug);
      }
      return new SelfHostedTelemetryProvider(config.endpoint);
    }
    case "custom":
    default:
      return new NoopTelemetryProvider(config.debug);
  }
}

// ---------------------------------------------------------------------------
// Wallet Telemetry Service (Manager)
// ---------------------------------------------------------------------------

export class WalletTelemetryService {
  private config: WalletTelemetryConfig;
  private provider: WalletTelemetryProvider;
  /**
   * True when the active provider was supplied explicitly (via
   * `setProvider`) instead of being derived from configuration.
   */
  private hasCustomProvider = false;

  constructor(initialConfig?: Partial<WalletTelemetryConfig>) {
    const envConfig = resolveConfigFromEnv();
    this.config = { ...envConfig, ...initialConfig };
    this.provider = createProviderFromConfig(this.config);
  }

  /** Update runtime configuration and instantiate corresponding provider */
  configure(config: Partial<WalletTelemetryConfig>): void {
    this.config = { ...this.config, ...config };

    if (this.hasCustomProvider && config.provider === undefined) {
      return;
    }

    this.hasCustomProvider = false;
    this.provider = createProviderFromConfig(this.config);
  }

  /** Explicitly set a custom provider instance */
  setProvider(provider: WalletTelemetryProvider): void {
    this.provider = provider;
    this.hasCustomProvider = true;
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
    this.hasCustomProvider = false;
    this.provider = createProviderFromConfig(this.config);
  }

  /** Track event through active provider with sanitization & hashing */
  async trackEvent(
    name: string,
    props?: Record<string, unknown>
  ): Promise<void> {
    if (!this.config.enabled) {
      return;
    }

    try {
      const sanitized = await sanitizeEventProps(
        name,
        props,
        this.config.defaultProperties
      );
      const payload = {
        ...sanitized,
        timestamp: Date.now(),
      };
      await this.provider.trackEvent(name, payload);
    } catch (err) {
      console.warn(`[wallet-telemetry] Error tracking event "${name}":`, err);
    }
  }

  /** Identify user session using privacy-preserving address hash */
  async identify(
    userId: string,
    traits?: Record<string, unknown>
  ): Promise<void> {
    if (!this.config.enabled) return;
    try {
      const hashedId = await hashAddress(userId);
      await this.provider.identify?.(hashedId, traits);
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
export function configureWalletTelemetry(
  config: Partial<WalletTelemetryConfig>
): void {
  walletTelemetry.configure(config);
}

/**
 * Set a custom telemetry provider instance
 */
export function setWalletTelemetryProvider(
  provider: WalletTelemetryProvider
): void {
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
  address?: string;
  address_hash?: string;
  walletType?: string;
  network?: string;
  source?: string;
  [key: string]: unknown;
}

export interface WalletConnectFailedProps {
  error?: string;
  reason?: WalletFailureReason | string;
  walletType?: string;
  network?: string;
  [key: string]: unknown;
}

export interface WalletDisconnectedProps {
  address?: string;
  address_hash?: string;
  network?: string;
  reason?: string;
  [key: string]: unknown;
}

export interface WalletNetworkChangedProps {
  network: string;
  previousNetwork?: string;
  address?: string;
  address_hash?: string;
  [key: string]: unknown;
}

/**
 * Emitted when a connection attempt begins
 */
export async function trackWalletConnectAttempt(
  props?: WalletConnectAttemptProps
): Promise<void> {
  await trackWalletEvent("wallet_connect_attempt", props);
}

/**
 * Emitted when a wallet is successfully connected
 */
export async function trackWalletConnected(
  props: WalletConnectedProps
): Promise<void> {
  await trackWalletEvent("wallet_connected", props);
}

/**
 * Emitted when a wallet connection attempt fails or is rejected
 */
export async function trackWalletConnectFailed(
  props: WalletConnectFailedProps
): Promise<void> {
  await trackWalletEvent("wallet_connect_failed", props);
}

/**
 * Emitted when a wallet is disconnected
 */
export async function trackWalletDisconnected(
  props?: WalletDisconnectedProps
): Promise<void> {
  await trackWalletEvent("wallet_disconnected", props);
}

/**
 * Emitted when the wallet network changes
 */
export async function trackWalletNetworkChanged(
  props: WalletNetworkChangedProps
): Promise<void> {
  await trackWalletEvent("wallet_network_changed", props);
}

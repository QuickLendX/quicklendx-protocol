/**
 * Webhook Payload Versioning & Compatibility Window (#851)
 *
 * Provides:
 * - Explicit payload version constants and a migration registry
 * - A compatibility window so integrators can stay on older schema versions
 *   for a configurable window (default: 90 days)
 * - Per-subscriber "version pin" that the dispatcher respects
 * - Shape-safe transformation from newer → older versions
 */

// ---------------------------------------------------------------------------
// Version constants
// ---------------------------------------------------------------------------

/** Current (latest) schema version produced by the indexer */
export const CURRENT_WEBHOOK_VERSION = 2 as const;

/** Oldest schema version still served within the compatibility window */
export const MIN_SUPPORTED_WEBHOOK_VERSION = 1 as const;

export type WebhookVersion =
  | typeof MIN_SUPPORTED_WEBHOOK_VERSION
  | typeof CURRENT_WEBHOOK_VERSION;

// ---------------------------------------------------------------------------
// Contract event types (mirrored from on-chain events.rs)
// ---------------------------------------------------------------------------

export type WebhookEventType =
  | "invoice.uploaded"
  | "invoice.verified"
  | "invoice.cancelled"
  | "invoice.settled"
  | "invoice.defaulted"
  | "invoice.expired"
  | "invoice.funded"
  | "bid.placed"
  | "bid.accepted"
  | "bid.withdrawn"
  | "bid.expired"
  | "escrow.created"
  | "escrow.released"
  | "escrow.refunded"
  | "payment.recorded"
  | "payment.partial"
  | "dispute.created"
  | "dispute.resolved";

// ---------------------------------------------------------------------------
// Canonical (v2) envelope
// ---------------------------------------------------------------------------

export interface WebhookEnvelopeV2<T = unknown> {
  /** Schema version – always present so consumers can branch */
  version: 2;
  /** Unique delivery ID (idempotency key) */
  delivery_id: string;
  /** ISO-8601 timestamp of event creation */
  created_at: string;
  /** Cursor identifying the on-chain position that produced this event */
  cursor: ChainCursor;
  event_type: WebhookEventType;
  payload: T;
}

// ---------------------------------------------------------------------------
// Legacy (v1) envelope – kept for backward-compat window
// ---------------------------------------------------------------------------

export interface WebhookEnvelopeV1<T = unknown> {
  version: 1;
  delivery_id: string;
  created_at: string;
  event_type: WebhookEventType;
  /** v1 did not include a cursor or event_index */
  ledger_seq: number;
  tx_hash: string;
  payload: T;
}

export type WebhookEnvelope<T = unknown> =
  | WebhookEnvelopeV1<T>
  | WebhookEnvelopeV2<T>;

// ---------------------------------------------------------------------------
// Chain cursor (shared with cursor module)
// ---------------------------------------------------------------------------

export interface ChainCursor {
  /** Ledger sequence number */
  ledger_seq: number;
  /** Transaction hash (hex string) */
  tx_hash: string;
  /** Position of this event within the transaction */
  event_index: number;
}

// ---------------------------------------------------------------------------
// Subscriber version pin
// ---------------------------------------------------------------------------

export interface SubscriberConfig {
  subscriber_id: string;
  /** URL to POST webhook payloads to */
  endpoint_url: string;
  /** Which schema version this subscriber wants to receive */
  pinned_version: WebhookVersion;
  /**
   * Unix timestamp after which the pin is considered expired.
   * Once expired, the dispatcher will upgrade the subscriber to CURRENT_WEBHOOK_VERSION
   * and emit a `webhook.version_upgraded` meta event.
   */
  pin_expires_at: number | null;
  /** Shared secret for HMAC-SHA256 signature */
  secret: string;
  /** Subscribed event types; empty array = subscribe to all */
  event_types: WebhookEventType[];
}

// ---------------------------------------------------------------------------
// Compatibility window configuration
// ---------------------------------------------------------------------------

export interface CompatibilityWindowConfig {
  /**
   * How many seconds older versions are supported after a new version ships.
   * Default: 90 days in seconds.
   */
  window_seconds: number;
  /** Whether to force-upgrade expired pins or just warn */
  force_upgrade_expired_pins: boolean;
}

export const DEFAULT_COMPATIBILITY_WINDOW: CompatibilityWindowConfig = {
  window_seconds: 90 * 24 * 60 * 60, // 90 days
  force_upgrade_expired_pins: true,
};

import { z } from "zod";

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

export enum WebhookSecretStatus {
  /** Only the primary secret is active. */
  Active = "active",
  /** A pending secret exists alongside the primary (dual-verify window). */
  Rotating = "rotating",
}

// ---------------------------------------------------------------------------
// Core domain types
// ---------------------------------------------------------------------------

/**
 * Persisted record for a single subscriber's webhook secret state.
 * Secrets are stored as hex-encoded HMAC-SHA256 keys.
 *
 * SECURITY: Never expose `primary_secret` or `pending_secret` in API
 * responses or log output.
 */
export interface WebhookSubscriberSecret {
  /** Unique subscriber identifier (opaque string, e.g. UUID). */
  subscriber_id: string;
  /** The currently active signing secret (hex). */
  primary_secret: string;
  /** A newly generated secret awaiting promotion (hex), or null. */
  pending_secret: string | null;
  /** ISO-8601 timestamp when the pending secret was created. */
  pending_created_at: string | null;
  /**
   * Grace period in seconds during which both primary and pending secrets
   * are accepted for verification.  Defaults to 3600 (1 hour).
   */
  grace_period_seconds: number;
  /** Current rotation status. */
  status: WebhookSecretStatus;
  /** ISO-8601 timestamp of record creation. */
  created_at: string;
  /** ISO-8601 timestamp of last update. */
  updated_at: string;
}

// ---------------------------------------------------------------------------
// Zod schemas (runtime validation)
// ---------------------------------------------------------------------------

export const InitiateRotationRequestSchema = z.object({
  /**
   * Optional override for the grace period (seconds).
   * Must be between 60 s and 86 400 s (24 h).
   */
  grace_period_seconds: z
    .number()
    .int()
    .min(60)
    .max(86_400)
    .optional()
    .default(3600),
});

export type InitiateRotationRequest = z.infer<
  typeof InitiateRotationRequestSchema
>;

export const RegisterSubscriberRequestSchema = z.object({
  subscriber_id: z.string().min(1).max(128),
  grace_period_seconds: z
    .number()
    .int()
    .min(60)
    .max(86_400)
    .optional()
    .default(3600),
});

export type RegisterSubscriberRequest = z.infer<
  typeof RegisterSubscriberRequestSchema
>;

// ---------------------------------------------------------------------------
// Safe public-facing response shapes (no secrets)
// ---------------------------------------------------------------------------

export interface SubscriberSecretPublicView {
  subscriber_id: string;
  status: WebhookSecretStatus;
  /** Whether a pending secret currently exists. */
  has_pending_secret: boolean;
  pending_created_at: string | null;
  grace_period_seconds: number;
  created_at: string;
  updated_at: string;
}

export interface InitiateRotationResponse {
  subscriber_id: string;
  status: WebhookSecretStatus;
  /**
   * The newly generated pending secret, returned **once** at initiation.
   * The caller must store this value; it will never be returned again.
   */
  new_secret: string;
  grace_period_seconds: number;
  pending_created_at: string;
}

export interface FinalizeRotationResponse {
  subscriber_id: string;
  status: WebhookSecretStatus;
  message: string;
}

// ---------------------------------------------------------------------------
// Webhook verification types
// ---------------------------------------------------------------------------

export interface WebhookVerificationResult {
  valid: boolean;
  /** Which secret matched: "primary" | "pending" | null */
  matched_secret: "primary" | "pending" | null;
}

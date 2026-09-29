/**
 * Webhook Versioning Registry & Downgrade Transforms (#851)
 *
 * The migration registry maps (fromVersion → toVersion) pairs to a pure
 * transform function. Transforms are applied in sequence to step a v2 payload
 * down to v1 when a subscriber has an active version pin.
 */

import {
  ChainCursor,
  CURRENT_WEBHOOK_VERSION,
  MIN_SUPPORTED_WEBHOOK_VERSION,
  WebhookEnvelopeV1,
  WebhookEnvelopeV2,
  WebhookVersion,
} from "./types";

// ---------------------------------------------------------------------------
// Transform type
// ---------------------------------------------------------------------------

type VersionDowngrader<From, To> = (envelope: From) => To;

// ---------------------------------------------------------------------------
// v2 → v1 downgrade
// ---------------------------------------------------------------------------

function downgradeV2ToV1<T>(
  envelope: WebhookEnvelopeV2<T>
): WebhookEnvelopeV1<T> {
  return {
    version: 1,
    delivery_id: envelope.delivery_id,
    created_at: envelope.created_at,
    event_type: envelope.event_type,
    // v1 subscribers get ledger_seq + tx_hash at the top level; event_index
    // is dropped because v1 schema did not define it.
    ledger_seq: envelope.cursor.ledger_seq,
    tx_hash: envelope.cursor.tx_hash,
    payload: envelope.payload,
  };
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/**
 * Ordered list of downgrade functions from CURRENT_WEBHOOK_VERSION down to
 * MIN_SUPPORTED_WEBHOOK_VERSION. Add new entries here whenever a new schema
 * version is released.
 */
const DOWNGRADE_CHAIN: Array<
  VersionDowngrader<WebhookEnvelopeV2, WebhookEnvelopeV1>
> = [
  // step 2 → 1
  downgradeV2ToV1,
];

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Transform a v2 (canonical) envelope into the schema version requested by
 * a pinned subscriber. Returns the envelope unchanged if the target version
 * equals CURRENT_WEBHOOK_VERSION.
 *
 * @throws {Error} if targetVersion is outside the supported range
 */
export function transformEnvelope<T>(
  envelope: WebhookEnvelopeV2<T>,
  targetVersion: WebhookVersion
): WebhookEnvelopeV1<T> | WebhookEnvelopeV2<T> {
  if (targetVersion === CURRENT_WEBHOOK_VERSION) {
    return envelope;
  }

  if (targetVersion < MIN_SUPPORTED_WEBHOOK_VERSION) {
    throw new Error(
      `Webhook schema version ${targetVersion} is no longer supported. ` +
        `Minimum supported version is ${MIN_SUPPORTED_WEBHOOK_VERSION}.`
    );
  }

  // Apply downgrade steps from current → target
  const steps = CURRENT_WEBHOOK_VERSION - targetVersion;
  if (steps > DOWNGRADE_CHAIN.length) {
    throw new Error(
      `No downgrade path registered from v${CURRENT_WEBHOOK_VERSION} to v${targetVersion}.`
    );
  }

  // Apply steps sequentially (cast needed because TS can't track the chain)
  let result: unknown = envelope;
  for (let i = 0; i < steps; i++) {
    result = DOWNGRADE_CHAIN[i](result as WebhookEnvelopeV2<unknown>);
  }
  return result as WebhookEnvelopeV1<T>;
}

/**
 * Build a canonical v2 envelope ready for dispatch.
 */
export function buildEnvelopeV2<T>(opts: {
  delivery_id: string;
  cursor: ChainCursor;
  event_type: import("./types").WebhookEventType;
  payload: T;
}): WebhookEnvelopeV2<T> {
  return {
    version: CURRENT_WEBHOOK_VERSION,
    delivery_id: opts.delivery_id,
    created_at: new Date().toISOString(),
    cursor: opts.cursor,
    event_type: opts.event_type,
    payload: opts.payload,
  };
}

/**
 * Returns true if a subscriber's pinned version is still within the
 * compatibility window.
 */
export function isPinActive(
  pinExpiresAt: number | null,
  nowMs: number = Date.now()
): boolean {
  if (pinExpiresAt === null) return true; // no expiry = always active
  return nowMs < pinExpiresAt * 1000;
}

/**
 * Compute pin expiry timestamp (Unix seconds) from the version-ship date and
 * the configured window.
 */
export function computePinExpiry(
  versionShippedAtMs: number,
  windowSeconds: number
): number {
  return Math.floor(versionShippedAtMs / 1000) + windowSeconds;
}

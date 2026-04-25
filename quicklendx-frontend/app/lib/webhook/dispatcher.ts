/**
 * Webhook Dispatcher (#851)
 *
 * Dispatches a canonical v2 envelope to all active subscribers, applying
 * per-subscriber version transforms and HMAC-SHA256 signing. Respects the
 * compatibility window: expired pins are either force-upgraded or warned.
 */

import { createHmac, randomUUID } from "crypto";
import type {
  CompatibilityWindowConfig,
  SubscriberConfig,
  WebhookEnvelopeV2,
  WebhookEventType,
} from "./types";
import {
  DEFAULT_COMPATIBILITY_WINDOW,
  CURRENT_WEBHOOK_VERSION,
} from "./types";
import { buildEnvelopeV2, isPinActive, transformEnvelope } from "./versioning";
import type { ChainCursor } from "./types";

// ---------------------------------------------------------------------------
// In-memory subscriber store (replace with DB-backed store in production)
// ---------------------------------------------------------------------------

const subscriberStore = new Map<string, SubscriberConfig>();

export function registerSubscriber(config: SubscriberConfig): void {
  if (!config.subscriber_id || !config.endpoint_url || !config.secret) {
    throw new Error("subscriber_id, endpoint_url, and secret are required.");
  }
  if (
    config.pinned_version < 1 ||
    config.pinned_version > CURRENT_WEBHOOK_VERSION
  ) {
    throw new Error(
      `pinned_version must be between 1 and ${CURRENT_WEBHOOK_VERSION}.`
    );
  }
  subscriberStore.set(config.subscriber_id, config);
}

export function getSubscriber(subscriberId: string): SubscriberConfig | undefined {
  return subscriberStore.get(subscriberId);
}

export function updateSubscriberPin(
  subscriberId: string,
  newVersion: number,
  pinExpiresAt: number | null
): void {
  const sub = subscriberStore.get(subscriberId);
  if (!sub) throw new Error(`Subscriber ${subscriberId} not found.`);
  if (newVersion < 1 || newVersion > CURRENT_WEBHOOK_VERSION) {
    throw new Error(`Invalid version pin: ${newVersion}`);
  }
  subscriberStore.set(subscriberId, {
    ...sub,
    pinned_version: newVersion as SubscriberConfig["pinned_version"],
    pin_expires_at: pinExpiresAt,
  });
}

// ---------------------------------------------------------------------------
// HMAC signing
// ---------------------------------------------------------------------------

function signPayload(secret: string, body: string): string {
  return `sha256=${createHmac("sha256", secret).update(body).digest("hex")}`;
}

// ---------------------------------------------------------------------------
// HTTP delivery (stub – replace with your HTTP client or queue)
// ---------------------------------------------------------------------------

interface DeliveryResult {
  subscriber_id: string;
  status: "delivered" | "failed" | "skipped";
  http_status?: number;
  error?: string;
}

async function deliverToEndpoint(
  url: string,
  body: string,
  signature: string
): Promise<number> {
  // In a real implementation this would use `fetch` or `axios`.
  // We throw so tests can mock this boundary.
  const response = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-QuickLendX-Signature": signature,
      "X-QuickLendX-Version": String(CURRENT_WEBHOOK_VERSION),
    },
    body,
  });
  return response.status;
}

// ---------------------------------------------------------------------------
// Dispatcher
// ---------------------------------------------------------------------------

export async function dispatchEvent<T>(opts: {
  cursor: ChainCursor;
  event_type: WebhookEventType;
  payload: T;
  compatWindow?: CompatibilityWindowConfig;
  nowMs?: number;
}): Promise<DeliveryResult[]> {
  const compatWindow = opts.compatWindow ?? DEFAULT_COMPATIBILITY_WINDOW;
  const nowMs = opts.nowMs ?? Date.now();

  const canonical: WebhookEnvelopeV2<T> = buildEnvelopeV2({
    delivery_id: randomUUID(),
    cursor: opts.cursor,
    event_type: opts.event_type,
    payload: opts.payload,
  });

  const results: DeliveryResult[] = [];

  for (const sub of subscriberStore.values()) {
    // Filter by subscribed event types
    if (
      sub.event_types.length > 0 &&
      !sub.event_types.includes(opts.event_type)
    ) {
      results.push({ subscriber_id: sub.subscriber_id, status: "skipped" });
      continue;
    }

    // Resolve effective version
    let effectiveVersion = sub.pinned_version;
    if (!isPinActive(sub.pin_expires_at, nowMs)) {
      if (compatWindow.force_upgrade_expired_pins) {
        // Auto-upgrade to latest
        effectiveVersion = CURRENT_WEBHOOK_VERSION;
        // Persist the upgrade
        updateSubscriberPin(sub.subscriber_id, CURRENT_WEBHOOK_VERSION, null);
        console.warn(
          `[webhook] Subscriber ${sub.subscriber_id} pin expired – ` +
            `auto-upgraded to v${CURRENT_WEBHOOK_VERSION}.`
        );
      } else {
        console.warn(
          `[webhook] Subscriber ${sub.subscriber_id} pin expired but ` +
            `force_upgrade_expired_pins is disabled – delivering at v${sub.pinned_version}.`
        );
      }
    }

    // Transform envelope to subscriber's pinned version
    const envelope = transformEnvelope(canonical, effectiveVersion);
    const body = JSON.stringify(envelope);
    const signature = signPayload(sub.secret, body);

    try {
      const httpStatus = await deliverToEndpoint(
        sub.endpoint_url,
        body,
        signature
      );
      results.push({
        subscriber_id: sub.subscriber_id,
        status: httpStatus >= 200 && httpStatus < 300 ? "delivered" : "failed",
        http_status: httpStatus,
      });
    } catch (err) {
      results.push({
        subscriber_id: sub.subscriber_id,
        status: "failed",
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }

  return results;
}

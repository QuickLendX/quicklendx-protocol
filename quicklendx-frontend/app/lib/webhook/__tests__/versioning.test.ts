/**
 * Tests for webhook payload versioning (#851)
 *
 * Run with: npx jest app/lib/webhook/__tests__/versioning.test.ts
 *
 * These tests use no external dependencies – they exercise the pure transform
 * and envelope-building logic directly.
 */

import {
  buildEnvelopeV2,
  computePinExpiry,
  isPinActive,
  transformEnvelope,
} from "../versioning";
import type { WebhookEnvelopeV2 } from "../types";
import {
  CURRENT_WEBHOOK_VERSION,
  MIN_SUPPORTED_WEBHOOK_VERSION,
} from "../types";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const CURSOR = { ledger_seq: 1000, tx_hash: "abc123", event_index: 0 };

function makeEnvelope<T>(payload: T): WebhookEnvelopeV2<T> {
  return buildEnvelopeV2({
    delivery_id: "test-delivery-id",
    cursor: CURSOR,
    event_type: "invoice.uploaded",
    payload,
  });
}

// ---------------------------------------------------------------------------
// buildEnvelopeV2
// ---------------------------------------------------------------------------

describe("buildEnvelopeV2", () => {
  it("sets version to CURRENT_WEBHOOK_VERSION", () => {
    const env = makeEnvelope({ foo: "bar" });
    expect(env.version).toBe(CURRENT_WEBHOOK_VERSION);
  });

  it("embeds the cursor", () => {
    const env = makeEnvelope({ foo: "bar" });
    expect(env.cursor).toEqual(CURSOR);
  });

  it("includes created_at as an ISO-8601 string", () => {
    const env = makeEnvelope({});
    expect(() => new Date(env.created_at)).not.toThrow();
    expect(env.created_at).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });
});

// ---------------------------------------------------------------------------
// transformEnvelope
// ---------------------------------------------------------------------------

describe("transformEnvelope", () => {
  it("returns the same envelope when target === CURRENT", () => {
    const env = makeEnvelope({ amount: 1000 });
    const result = transformEnvelope(env, CURRENT_WEBHOOK_VERSION);
    expect(result).toBe(env); // same reference
  });

  it("downgrades v2 → v1 correctly", () => {
    const env = makeEnvelope({ amount: 1000 });
    const v1 = transformEnvelope(env, 1);
    expect(v1.version).toBe(1);
    // v1 must hoist ledger_seq and tx_hash to top level
    expect((v1 as any).ledger_seq).toBe(CURSOR.ledger_seq);
    expect((v1 as any).tx_hash).toBe(CURSOR.tx_hash);
    // v1 must NOT expose event_index
    expect((v1 as any).event_index).toBeUndefined();
    // Cursor wrapper must be removed in v1
    expect((v1 as any).cursor).toBeUndefined();
  });

  it("preserves delivery_id and event_type through downgrade", () => {
    const env = makeEnvelope({});
    const v1 = transformEnvelope(env, 1) as any;
    expect(v1.delivery_id).toBe("test-delivery-id");
    expect(v1.event_type).toBe("invoice.uploaded");
  });

  it("throws for unsupported versions below MIN_SUPPORTED", () => {
    const env = makeEnvelope({});
    expect(() => transformEnvelope(env, 0 as any)).toThrow(
      /no longer supported/i
    );
  });
});

// ---------------------------------------------------------------------------
// isPinActive / computePinExpiry
// ---------------------------------------------------------------------------

describe("isPinActive", () => {
  it("returns true when pin_expires_at is null (no expiry)", () => {
    expect(isPinActive(null)).toBe(true);
  });

  it("returns true when pin has not yet expired", () => {
    const futureUnix = Math.floor(Date.now() / 1000) + 3600;
    expect(isPinActive(futureUnix, Date.now())).toBe(true);
  });

  it("returns false when pin has expired", () => {
    const pastUnix = Math.floor(Date.now() / 1000) - 1;
    expect(isPinActive(pastUnix, Date.now())).toBe(false);
  });
});

describe("computePinExpiry", () => {
  it("adds windowSeconds to the shipped timestamp", () => {
    const shipped = 1_000_000_000; // ms
    const window = 7776000; // 90 days in seconds
    expect(computePinExpiry(shipped, window)).toBe(
      Math.floor(shipped / 1000) + window
    );
  });
});

// ---------------------------------------------------------------------------
// Schema constants sanity check
// ---------------------------------------------------------------------------

describe("version constants", () => {
  it("MIN_SUPPORTED <= CURRENT", () => {
    expect(MIN_SUPPORTED_WEBHOOK_VERSION).toBeLessThanOrEqual(
      CURRENT_WEBHOOK_VERSION
    );
  });
});

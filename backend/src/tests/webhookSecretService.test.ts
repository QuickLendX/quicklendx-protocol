/**
 * Unit tests for WebhookSecretService and WebhookSecretStore.
 *
 * Coverage targets: ≥95% branches, functions, lines, statements.
 */

import { createHmac } from "crypto";
import {
  WebhookSecretService,
  WebhookSecretStore,
  WebhookSecretError,
} from "../services/webhookSecretService";
import { WebhookSecretStatus } from "../types/webhook";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeService(): {
  service: WebhookSecretService;
  store: WebhookSecretStore;
} {
  const store = new WebhookSecretStore();
  const service = new WebhookSecretService(store);
  return { service, store };
}

function computeExpectedSig(payload: string, secretHex: string): string {
  const hmac = createHmac("sha256", Buffer.from(secretHex, "hex"));
  hmac.update(Buffer.from(payload));
  return `sha256=${hmac.digest("hex")}`;
}

// ---------------------------------------------------------------------------
// WebhookSecretStore
// ---------------------------------------------------------------------------

describe("WebhookSecretStore", () => {
  it("stores and retrieves a record", () => {
    const store = new WebhookSecretStore();
    const now = new Date().toISOString();
    const record = {
      subscriber_id: "sub-1",
      primary_secret: "aabbcc",
      pending_secret: null,
      pending_created_at: null,
      grace_period_seconds: 3600,
      status: WebhookSecretStatus.Active,
      created_at: now,
      updated_at: now,
    };
    store.set(record);
    expect(store.get("sub-1")).toEqual(record);
  });

  it("returns undefined for unknown subscriber", () => {
    const store = new WebhookSecretStore();
    expect(store.get("nope")).toBeUndefined();
  });

  it("has() returns correct boolean", () => {
    const store = new WebhookSecretStore();
    expect(store.has("x")).toBe(false);
    const now = new Date().toISOString();
    store.set({
      subscriber_id: "x",
      primary_secret: "aa",
      pending_secret: null,
      pending_created_at: null,
      grace_period_seconds: 3600,
      status: WebhookSecretStatus.Active,
      created_at: now,
      updated_at: now,
    });
    expect(store.has("x")).toBe(true);
  });

  it("delete() removes a record", () => {
    const store = new WebhookSecretStore();
    const now = new Date().toISOString();
    store.set({
      subscriber_id: "del-me",
      primary_secret: "aa",
      pending_secret: null,
      pending_created_at: null,
      grace_period_seconds: 3600,
      status: WebhookSecretStatus.Active,
      created_at: now,
      updated_at: now,
    });
    expect(store.delete("del-me")).toBe(true);
    expect(store.has("del-me")).toBe(false);
    expect(store.delete("del-me")).toBe(false);
  });

  it("_clear() empties the store", () => {
    const store = new WebhookSecretStore();
    const now = new Date().toISOString();
    store.set({
      subscriber_id: "a",
      primary_secret: "aa",
      pending_secret: null,
      pending_created_at: null,
      grace_period_seconds: 3600,
      status: WebhookSecretStatus.Active,
      created_at: now,
      updated_at: now,
    });
    store._clear();
    expect(store._all()).toHaveLength(0);
  });

  it("_all() returns all records", () => {
    const store = new WebhookSecretStore();
    const now = new Date().toISOString();
    store.set({
      subscriber_id: "a",
      primary_secret: "aa",
      pending_secret: null,
      pending_created_at: null,
      grace_period_seconds: 3600,
      status: WebhookSecretStatus.Active,
      created_at: now,
      updated_at: now,
    });
    store.set({
      subscriber_id: "b",
      primary_secret: "bb",
      pending_secret: null,
      pending_created_at: null,
      grace_period_seconds: 3600,
      status: WebhookSecretStatus.Active,
      created_at: now,
      updated_at: now,
    });
    expect(store._all()).toHaveLength(2);
  });
});

// ---------------------------------------------------------------------------
// generateSecret
// ---------------------------------------------------------------------------

describe("WebhookSecretService.generateSecret", () => {
  it("returns a 64-character hex string (32 bytes)", () => {
    const { service } = makeService();
    const secret = service.generateSecret();
    expect(secret).toMatch(/^[0-9a-f]{64}$/);
  });

  it("generates unique secrets on each call", () => {
    const { service } = makeService();
    const secrets = new Set(Array.from({ length: 20 }, () => service.generateSecret()));
    expect(secrets.size).toBe(20);
  });
});

// ---------------------------------------------------------------------------
// registerSubscriber
// ---------------------------------------------------------------------------

describe("WebhookSecretService.registerSubscriber", () => {
  it("registers a new subscriber and returns initial secret", () => {
    const { service } = makeService();
    const result = service.registerSubscriber("sub-1");

    expect(result.view.subscriber_id).toBe("sub-1");
    expect(result.view.status).toBe(WebhookSecretStatus.Active);
    expect(result.view.has_pending_secret).toBe(false);
    expect(result.initial_secret).toMatch(/^[0-9a-f]{64}$/);
  });

  it("uses custom grace period", () => {
    const { service } = makeService();
    const result = service.registerSubscriber("sub-grace", 7200);
    expect(result.view.grace_period_seconds).toBe(7200);
  });

  it("uses default grace period of 3600 when not specified", () => {
    const { service } = makeService();
    const result = service.registerSubscriber("sub-default");
    expect(result.view.grace_period_seconds).toBe(3600);
  });

  it("throws SUBSCRIBER_ALREADY_EXISTS on duplicate registration", () => {
    const { service } = makeService();
    service.registerSubscriber("dup");
    expect(() => service.registerSubscriber("dup")).toThrow(WebhookSecretError);
    try {
      service.registerSubscriber("dup");
    } catch (err) {
      expect(err).toBeInstanceOf(WebhookSecretError);
      expect((err as WebhookSecretError).code).toBe("SUBSCRIBER_ALREADY_EXISTS");
      expect((err as WebhookSecretError).status).toBe(409);
    }
  });

  it("public view does not contain secret fields", () => {
    const { service } = makeService();
    const result = service.registerSubscriber("safe-sub");
    const view = result.view as unknown as Record<string, unknown>;
    expect(view).not.toHaveProperty("primary_secret");
    expect(view).not.toHaveProperty("pending_secret");
  });
});

// ---------------------------------------------------------------------------
// getSubscriberView
// ---------------------------------------------------------------------------

describe("WebhookSecretService.getSubscriberView", () => {
  it("returns public view for existing subscriber", () => {
    const { service } = makeService();
    service.registerSubscriber("view-sub");
    const view = service.getSubscriberView("view-sub");
    expect(view.subscriber_id).toBe("view-sub");
    expect(view.status).toBe(WebhookSecretStatus.Active);
  });

  it("throws SUBSCRIBER_NOT_FOUND for unknown subscriber", () => {
    const { service } = makeService();
    expect(() => service.getSubscriberView("ghost")).toThrow(WebhookSecretError);
    try {
      service.getSubscriberView("ghost");
    } catch (err) {
      expect((err as WebhookSecretError).code).toBe("SUBSCRIBER_NOT_FOUND");
      expect((err as WebhookSecretError).status).toBe(404);
    }
  });
});

// ---------------------------------------------------------------------------
// initiateRotation
// ---------------------------------------------------------------------------

describe("WebhookSecretService.initiateRotation", () => {
  it("generates a pending secret and enters Rotating status", () => {
    const { service } = makeService();
    service.registerSubscriber("rot-sub");
    const result = service.initiateRotation("rot-sub");

    expect(result.status).toBe(WebhookSecretStatus.Rotating);
    expect(result.new_secret).toMatch(/^[0-9a-f]{64}$/);
    expect(result.grace_period_seconds).toBe(3600);
    expect(result.pending_created_at).toBeTruthy();
  });

  it("accepts a custom grace period override", () => {
    const { service } = makeService();
    service.registerSubscriber("rot-grace");
    const result = service.initiateRotation("rot-grace", 1800);
    expect(result.grace_period_seconds).toBe(1800);
  });

  it("uses subscriber's existing grace period when none provided", () => {
    const { service } = makeService();
    service.registerSubscriber("rot-default", 7200);
    const result = service.initiateRotation("rot-default");
    expect(result.grace_period_seconds).toBe(7200);
  });

  it("throws ROTATION_ALREADY_IN_PROGRESS if rotation is active", () => {
    const { service } = makeService();
    service.registerSubscriber("double-rot");
    service.initiateRotation("double-rot");
    expect(() => service.initiateRotation("double-rot")).toThrow(WebhookSecretError);
    try {
      service.initiateRotation("double-rot");
    } catch (err) {
      expect((err as WebhookSecretError).code).toBe("ROTATION_ALREADY_IN_PROGRESS");
      expect((err as WebhookSecretError).status).toBe(409);
    }
  });

  it("throws SUBSCRIBER_NOT_FOUND for unknown subscriber", () => {
    const { service } = makeService();
    expect(() => service.initiateRotation("ghost")).toThrow(WebhookSecretError);
  });

  it("view reflects has_pending_secret = true after initiation", () => {
    const { service } = makeService();
    service.registerSubscriber("pending-view");
    service.initiateRotation("pending-view");
    const view = service.getSubscriberView("pending-view");
    expect(view.has_pending_secret).toBe(true);
    expect(view.pending_created_at).not.toBeNull();
  });
});

// ---------------------------------------------------------------------------
// finalizeRotation
// ---------------------------------------------------------------------------

describe("WebhookSecretService.finalizeRotation", () => {
  it("promotes pending to primary and clears pending", () => {
    const { service } = makeService();
    service.registerSubscriber("fin-sub");
    const rotResult = service.initiateRotation("fin-sub");
    const newSecret = rotResult.new_secret;

    const finResult = service.finalizeRotation("fin-sub");
    expect(finResult.status).toBe(WebhookSecretStatus.Active);
    expect(finResult.message).toContain("finalized");

    // After finalization, only the new secret should verify.
    const payload = "test-payload";
    const sig = computeExpectedSig(payload, newSecret);
    const verifyResult = service.verifySignature("fin-sub", payload, sig);
    expect(verifyResult.valid).toBe(true);
    expect(verifyResult.matched_secret).toBe("primary");
  });

  it("throws NO_ROTATION_IN_PROGRESS when not rotating", () => {
    const { service } = makeService();
    service.registerSubscriber("no-rot");
    expect(() => service.finalizeRotation("no-rot")).toThrow(WebhookSecretError);
    try {
      service.finalizeRotation("no-rot");
    } catch (err) {
      expect((err as WebhookSecretError).code).toBe("NO_ROTATION_IN_PROGRESS");
      expect((err as WebhookSecretError).status).toBe(409);
    }
  });

  it("throws SUBSCRIBER_NOT_FOUND for unknown subscriber", () => {
    const { service } = makeService();
    expect(() => service.finalizeRotation("ghost")).toThrow(WebhookSecretError);
  });

  it("view reflects has_pending_secret = false after finalization", () => {
    const { service } = makeService();
    service.registerSubscriber("fin-view");
    service.initiateRotation("fin-view");
    service.finalizeRotation("fin-view");
    const view = service.getSubscriberView("fin-view");
    expect(view.has_pending_secret).toBe(false);
    expect(view.status).toBe(WebhookSecretStatus.Active);
  });
});

// ---------------------------------------------------------------------------
// cancelRotation
// ---------------------------------------------------------------------------

describe("WebhookSecretService.cancelRotation", () => {
  it("cancels rotation and reverts to Active status", () => {
    const { service } = makeService();
    service.registerSubscriber("cancel-sub");
    service.initiateRotation("cancel-sub");
    const view = service.cancelRotation("cancel-sub");

    expect(view.status).toBe(WebhookSecretStatus.Active);
    expect(view.has_pending_secret).toBe(false);
  });

  it("throws NO_ROTATION_IN_PROGRESS when not rotating", () => {
    const { service } = makeService();
    service.registerSubscriber("no-cancel");
    expect(() => service.cancelRotation("no-cancel")).toThrow(WebhookSecretError);
    try {
      service.cancelRotation("no-cancel");
    } catch (err) {
      expect((err as WebhookSecretError).code).toBe("NO_ROTATION_IN_PROGRESS");
    }
  });

  it("throws SUBSCRIBER_NOT_FOUND for unknown subscriber", () => {
    const { service } = makeService();
    expect(() => service.cancelRotation("ghost")).toThrow(WebhookSecretError);
  });

  it("old primary secret still works after cancel", () => {
    const { service } = makeService();
    const { initial_secret } = service.registerSubscriber("cancel-verify");
    service.initiateRotation("cancel-verify");
    service.cancelRotation("cancel-verify");

    const payload = "hello";
    const sig = computeExpectedSig(payload, initial_secret);
    const result = service.verifySignature("cancel-verify", payload, sig);
    expect(result.valid).toBe(true);
    expect(result.matched_secret).toBe("primary");
  });
});

// ---------------------------------------------------------------------------
// computeSignature
// ---------------------------------------------------------------------------

describe("WebhookSecretService.computeSignature", () => {
  it("returns sha256= prefixed hex string", () => {
    const { service } = makeService();
    const secret = service.generateSecret();
    const sig = service.computeSignature("hello", secret);
    expect(sig).toMatch(/^sha256=[0-9a-f]{64}$/);
  });

  it("accepts Buffer payload", () => {
    const { service } = makeService();
    const secret = service.generateSecret();
    const sigStr = service.computeSignature("hello", secret);
    const sigBuf = service.computeSignature(Buffer.from("hello"), secret);
    expect(sigStr).toBe(sigBuf);
  });

  it("produces deterministic output for same inputs", () => {
    const { service } = makeService();
    const secret = service.generateSecret();
    const sig1 = service.computeSignature("payload", secret);
    const sig2 = service.computeSignature("payload", secret);
    expect(sig1).toBe(sig2);
  });

  it("produces different output for different payloads", () => {
    const { service } = makeService();
    const secret = service.generateSecret();
    expect(service.computeSignature("a", secret)).not.toBe(
      service.computeSignature("b", secret)
    );
  });

  it("produces different output for different secrets", () => {
    const { service } = makeService();
    const s1 = service.generateSecret();
    const s2 = service.generateSecret();
    expect(service.computeSignature("payload", s1)).not.toBe(
      service.computeSignature("payload", s2)
    );
  });
});

// ---------------------------------------------------------------------------
// verifySignature – primary secret
// ---------------------------------------------------------------------------

describe("WebhookSecretService.verifySignature – primary secret", () => {
  it("returns valid=true and matched_secret=primary for correct signature", () => {
    const { service } = makeService();
    const { initial_secret } = service.registerSubscriber("verify-primary");
    const payload = "event-data";
    const sig = computeExpectedSig(payload, initial_secret);

    const result = service.verifySignature("verify-primary", payload, sig);
    expect(result.valid).toBe(true);
    expect(result.matched_secret).toBe("primary");
  });

  it("returns valid=false for wrong signature", () => {
    const { service } = makeService();
    service.registerSubscriber("verify-wrong");
    const result = service.verifySignature(
      "verify-wrong",
      "payload",
      "sha256=deadbeef"
    );
    expect(result.valid).toBe(false);
    expect(result.matched_secret).toBeNull();
  });

  it("returns valid=false for missing sha256= prefix", () => {
    const { service } = makeService();
    const { initial_secret } = service.registerSubscriber("verify-prefix");
    const payload = "data";
    const rawHex = createHmac("sha256", Buffer.from(initial_secret, "hex"))
      .update(payload)
      .digest("hex");

    const result = service.verifySignature("verify-prefix", payload, rawHex);
    expect(result.valid).toBe(false);
  });

  it("returns valid=false for empty signature string", () => {
    const { service } = makeService();
    service.registerSubscriber("verify-empty");
    const result = service.verifySignature("verify-empty", "data", "");
    expect(result.valid).toBe(false);
  });

  it("accepts Buffer payload for verification", () => {
    const { service } = makeService();
    const { initial_secret } = service.registerSubscriber("verify-buf");
    const payload = Buffer.from("buffer-payload");
    const sig = service.computeSignature(payload, initial_secret);
    const result = service.verifySignature("verify-buf", payload, sig);
    expect(result.valid).toBe(true);
  });

  it("throws SUBSCRIBER_NOT_FOUND for unknown subscriber", () => {
    const { service } = makeService();
    expect(() =>
      service.verifySignature("ghost", "data", "sha256=abc")
    ).toThrow(WebhookSecretError);
  });
});

// ---------------------------------------------------------------------------
// verifySignature – dual-verify window (rotation)
// ---------------------------------------------------------------------------

describe("WebhookSecretService.verifySignature – dual-verify window", () => {
  it("accepts old (primary) secret during rotation window", () => {
    const { service } = makeService();
    const { initial_secret } = service.registerSubscriber("dual-old");
    service.initiateRotation("dual-old");

    const payload = "event";
    const sig = computeExpectedSig(payload, initial_secret);
    const result = service.verifySignature("dual-old", payload, sig);

    expect(result.valid).toBe(true);
    expect(result.matched_secret).toBe("primary");
  });

  it("accepts new (pending) secret during rotation window", () => {
    const { service } = makeService();
    service.registerSubscriber("dual-new");
    const { new_secret } = service.initiateRotation("dual-new");

    const payload = "event";
    const sig = computeExpectedSig(payload, new_secret);
    const result = service.verifySignature("dual-new", payload, sig);

    expect(result.valid).toBe(true);
    expect(result.matched_secret).toBe("pending");
  });

  it("rejects invalid signature during rotation window", () => {
    const { service } = makeService();
    service.registerSubscriber("dual-invalid");
    service.initiateRotation("dual-invalid");

    const result = service.verifySignature(
      "dual-invalid",
      "event",
      "sha256=badhash"
    );
    expect(result.valid).toBe(false);
  });

  it("old secret rejected after finalization", () => {
    const { service } = makeService();
    const { initial_secret } = service.registerSubscriber("post-fin");
    service.initiateRotation("post-fin");
    service.finalizeRotation("post-fin");

    const payload = "event";
    const oldSig = computeExpectedSig(payload, initial_secret);
    const result = service.verifySignature("post-fin", payload, oldSig);
    expect(result.valid).toBe(false);
  });

  it("new secret accepted as primary after finalization", () => {
    const { service } = makeService();
    service.registerSubscriber("post-fin-new");
    const { new_secret } = service.initiateRotation("post-fin-new");
    service.finalizeRotation("post-fin-new");

    const payload = "event";
    const sig = computeExpectedSig(payload, new_secret);
    const result = service.verifySignature("post-fin-new", payload, sig);
    expect(result.valid).toBe(true);
    expect(result.matched_secret).toBe("primary");
  });
});

// ---------------------------------------------------------------------------
// Grace period auto-expiry (lazy promotion)
// ---------------------------------------------------------------------------

describe("WebhookSecretService – grace period auto-expiry", () => {
  it("auto-promotes pending secret after grace period elapses", () => {
    const { service, store } = makeService();
    const { initial_secret } = service.registerSubscriber("expire-sub");
    const { new_secret } = service.initiateRotation("expire-sub", 60);

    // Manually backdate the pending_created_at to simulate elapsed grace period.
    const record = store.get("expire-sub")!;
    const expired = new Date(Date.now() - 120_000).toISOString(); // 2 min ago
    store.set({ ...record, pending_created_at: expired });

    const payload = "event";

    // Old secret should now be rejected (pending was auto-promoted to primary).
    const oldSig = computeExpectedSig(payload, initial_secret);
    const oldResult = service.verifySignature("expire-sub", payload, oldSig);
    expect(oldResult.valid).toBe(false);

    // New secret should now be accepted as primary.
    const newSig = computeExpectedSig(payload, new_secret);
    const newResult = service.verifySignature("expire-sub", payload, newSig);
    expect(newResult.valid).toBe(true);
    expect(newResult.matched_secret).toBe("primary");

    // Status should be Active after auto-promotion.
    const view = service.getSubscriberView("expire-sub");
    expect(view.status).toBe(WebhookSecretStatus.Active);
    expect(view.has_pending_secret).toBe(false);
  });

  it("does not auto-promote before grace period elapses", () => {
    const { service } = makeService();
    service.registerSubscriber("no-expire");
    const { new_secret } = service.initiateRotation("no-expire", 3600);

    const payload = "event";
    const sig = computeExpectedSig(payload, new_secret);
    const result = service.verifySignature("no-expire", payload, sig);
    // Pending secret should still be accepted.
    expect(result.valid).toBe(true);
    expect(result.matched_secret).toBe("pending");
  });
});

// ---------------------------------------------------------------------------
// Singleton
// ---------------------------------------------------------------------------

describe("WebhookSecretService singleton", () => {
  it("getInstance returns the same instance", () => {
    const a = WebhookSecretService.getInstance();
    const b = WebhookSecretService.getInstance();
    expect(a).toBe(b);
  });
});

// ---------------------------------------------------------------------------
// WebhookSecretError
// ---------------------------------------------------------------------------

describe("WebhookSecretError", () => {
  it("has correct name, code, and status", () => {
    const err = new WebhookSecretError("msg", "MY_CODE", 422);
    expect(err.name).toBe("WebhookSecretError");
    expect(err.code).toBe("MY_CODE");
    expect(err.status).toBe(422);
    expect(err.message).toBe("msg");
    expect(err instanceof Error).toBe(true);
  });
});

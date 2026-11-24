import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import crypto from "crypto";
import type { NextFunction, Request, Response } from "express";
import { NonceStore, NONCE_WINDOW_MS, DEFAULT_NONCE_MAX_ENTRIES, nonceStore, requireSignature } from "../middleware/request-signing";

const T0 = Date.UTC(2026, 0, 1, 12, 0, 0);

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(T0);
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllEnvs();
});

describe("NonceStore", () => {
  it("accepts a fresh nonce and rejects a replay for the same key", () => {
    const store = new NonceStore();
    expect(store.checkAndAdd("key-a", "n1", T0)).toBe(true);
    expect(store.checkAndAdd("key-a", "n1", T0)).toBe(false);
  });

  it("scopes nonces per API key", () => {
    const store = new NonceStore();
    expect(store.checkAndAdd("key-a", "shared", T0)).toBe(true);
    // A different key may reuse the nonce and cannot burn key-a's entry.
    expect(store.checkAndAdd("key-b", "shared", T0)).toBe(true);
    expect(store.checkAndAdd("key-a", "shared", T0)).toBe(false);
    expect(store.checkAndAdd("key-b", "shared", T0)).toBe(false);
  });

  it("cannot collide across (scope, nonce) boundaries", () => {
    const store = new NonceStore();
    expect(store.checkAndAdd("a", "b:c", T0)).toBe(true);
    expect(store.checkAndAdd("a:b", "c", T0)).toBe(true);
  });

  it("forgets a nonce once its window has passed", () => {
    const store = new NonceStore();
    expect(store.checkAndAdd("k", "n", T0)).toBe(true);
    vi.setSystemTime(T0 + NONCE_WINDOW_MS - 1);
    expect(store.checkAndAdd("k", "n", T0)).toBe(false);
    vi.setSystemTime(T0 + NONCE_WINDOW_MS + 1);
    expect(store.checkAndAdd("k", "n", T0)).toBe(true);
  });

  it("prunes deterministically on every insert (no random sampling)", () => {
    const store = new NonceStore();
    for (let i = 0; i < 50; i++) store.checkAndAdd("k", `old-${i}`, T0);
    expect(store.size).toBe(50);

    vi.setSystemTime(T0 + NONCE_WINDOW_MS + 1);
    const random = vi.spyOn(Math, "random").mockReturnValue(0.99); // would never prune under the old 5% sampling
    store.checkAndAdd("k", "fresh", Date.now());
    random.mockRestore();

    expect(store.size).toBe(1);
  });

  it("keeps the queue ordered by expiry when timestamps arrive out of order", () => {
    const store = new NonceStore();
    store.checkAndAdd("k", "late", T0 + 60_000);
    store.checkAndAdd("k", "early", T0 - 60_000);

    // 'early' expires first even though it was inserted second.
    vi.setSystemTime(T0 - 60_000 + NONCE_WINDOW_MS + 1);
    store.checkAndAdd("k", "probe", Date.now());
    expect(store.checkAndAdd("k", "early", T0 - 60_000)).toBe(true);
    expect(store.checkAndAdd("k", "late", T0 + 60_000)).toBe(false);
  });

  it("caps entries and evicts the oldest first", () => {
    const store = new NonceStore(NONCE_WINDOW_MS, 3);
    store.checkAndAdd("k", "n1", T0);
    store.checkAndAdd("k", "n2", T0 + 1);
    store.checkAndAdd("k", "n3", T0 + 2);
    store.checkAndAdd("k", "n4", T0 + 3);

    expect(store.size).toBe(3);
    expect(store.checkAndAdd("k", "n1", T0)).toBe(true); // evicted -> accepted again
    expect(store.checkAndAdd("k", "n4", T0 + 3)).toBe(false); // newest retained
  });

  it("stays bounded under a flood of unique nonces", () => {
    const store = new NonceStore(NONCE_WINDOW_MS, 100);
    for (let i = 0; i < 1_000; i++) store.checkAndAdd("k", `n-${i}`, T0 + i);
    expect(store.size).toBe(100);
  });

  it("reads NONCE_MAX_ENTRIES from the environment and falls back on bad values", () => {
    vi.stubEnv("NONCE_MAX_ENTRIES", "2");
    const capped = new NonceStore();
    capped.checkAndAdd("k", "a", T0);
    capped.checkAndAdd("k", "b", T0);
    capped.checkAndAdd("k", "c", T0);
    expect(capped.size).toBe(2);

    vi.stubEnv("NONCE_MAX_ENTRIES", "not-a-number");
    const fallback = new NonceStore();
    for (let i = 0; i < 5; i++) fallback.checkAndAdd("k", `n${i}`, T0);
    expect(fallback.size).toBe(5);
    expect(DEFAULT_NONCE_MAX_ENTRIES).toBeGreaterThan(5);
  });
});

describe("requireSignature nonce handling", () => {
  const secret = "signing-secret-hash";

  function makeReq(keyId: string, nonce: string, opts: { badSignature?: boolean } = {}): Request {
    const timestamp = String(Date.now());
    const method = "POST";
    const originalUrl = "/api/v1/bids";
    const emptySha = crypto.createHash("sha256").update("").digest("hex");
    const payload = `${method}${originalUrl}${emptySha}${timestamp}${nonce}`;
    const good = crypto.createHmac("sha256", secret).update(payload).digest("hex");
    return {
      method,
      originalUrl,
      headers: {
        "x-signature": opts.badSignature ? "0".repeat(good.length) : good,
        "x-timestamp": timestamp,
        "x-nonce": nonce,
      },
      apiKey: { id: keyId, signing_secret_hash: secret },
    } as unknown as Request;
  }

  function run(req: Request): { status: number | null; body: any; nextCalled: boolean } {
    const out = { status: null as number | null, body: undefined as any, nextCalled: false };
    const res = {
      status(code: number) {
        out.status = code;
        return this;
      },
      json(body: unknown) {
        out.body = body;
        return this;
      },
    } as unknown as Response;
    const next: NextFunction = () => {
      out.nextCalled = true;
    };
    requireSignature(req, res, next);
    return out;
  }

  beforeEach(() => nonceStore._clearForTest());

  it("does not consume the nonce when the signature is invalid", () => {
    const bad = run(makeReq("key-1", "nonce-1", { badSignature: true }));
    expect(bad.status).toBe(401);
    expect(bad.body.error.code).toBe("INVALID_SIGNATURE");
    expect(nonceStore.size).toBe(0);

    // The legitimate holder can still use the nonce afterwards.
    const good = run(makeReq("key-1", "nonce-1"));
    expect(good.nextCalled).toBe(true);
  });

  it("rejects a replay of a validly signed request", () => {
    expect(run(makeReq("key-1", "nonce-2")).nextCalled).toBe(true);
    const replay = run(makeReq("key-1", "nonce-2"));
    expect(replay.status).toBe(401);
    expect(replay.body.error.code).toBe("REPLAY_DETECTED");
    expect(replay.nextCalled).toBe(false);
  });

  it("allows a different API key to use the same nonce", () => {
    expect(run(makeReq("key-1", "nonce-3")).nextCalled).toBe(true);
    expect(run(makeReq("key-2", "nonce-3")).nextCalled).toBe(true);
  });

  it("lets an attacker who knows a nonce neither burn it nor learn it is in use", () => {
    // Attacker (unsigned/invalid) tries key-1's observed nonce first.
    const attack = run(makeReq("key-1", "nonce-4", { badSignature: true }));
    expect(attack.body.error.code).toBe("INVALID_SIGNATURE");
    expect(run(makeReq("key-1", "nonce-4")).nextCalled).toBe(true);
  });
});

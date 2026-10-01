import { describe, it, expect } from "vitest";
import { ApiKeysController } from "../api-keys";
import type { RevokeApiKeyRequest } from "../api-keys";
import { FrozenClock } from "../../../services/clock";
import { InMemoryApiKeyService } from "../../../services/api-key-service";
import { ApiKeyError } from "../../../errors/api-key-error";
import {
  StubApiKeyService,
  StubAuthorizationService,
  KEY_ID,
  OWNER_ID,
  ACTOR_ID,
  buildApiKey,
  buildRevokedApiKey,
  FIXED_TIME,
  transientError,
  systemError,
} from "../../../__fakes__/stub-services";

interface Harness {
  clock: FrozenClock;
  apiKeys: StubApiKeyService;
  authz: StubAuthorizationService;
  controller: ApiKeysController;
}

const makeHarness = (
  opts: {
    maxAttempts?: number;
    baseDelayMs?: number;
    onError?: (err: ApiKeyError) => void;
  } = {}
) => {
  const clock = new FrozenClock(FIXED_TIME);
  const apiKeys = new StubApiKeyService();
  const authz = new StubAuthorizationService();
  const controller = new ApiKeysController(apiKeys, authz, clock, opts);
  return { clock, apiKeys, authz, controller };
};

const okRequest = (
  overrides: Partial<RevokeApiKeyRequest> = {}
): RevokeApiKeyRequest => ({
  apiKeyId: KEY_ID,
  actorId: ACTOR_ID,
  ...overrides,
});

/** Flush the microtask queue so in-flight async work can progress synchronously. */
const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

const deferred = <T>() => {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
};

const activeKey = (overrides: Record<string, unknown> = {}) =>
  buildApiKey({ id: KEY_ID, ownerId: OWNER_ID, ...overrides });

const revokedKey = (overrides: Record<string, unknown> = {}) =>
  buildRevokedApiKey({ id: KEY_ID, ownerId: OWNER_ID, ...overrides });

describe("revokeApiKey — success path", () => {
  it("revokes an active key and reports the new state", async () => {
    const { clock, apiKeys, authz, controller } = makeHarness();
    authz.canActImpl = async () => true;
    apiKeys.getApiKeyImpl = async () => activeKey();
    apiKeys.revokeApiKeyImpl = async (id, by) =>
      buildRevokedApiKey({ id, revokedBy: by, revokedAt: clock.now() });

    const result = await controller.revokeApiKey(okRequest());

    expect(result).toEqual({
      success: true,
      apiKeyId: KEY_ID,
      ownerId: OWNER_ID,
      revokedAt: FIXED_TIME,
      wasAlreadyRevoked: false,
    });
    expect(apiKeys.calls.revokeApiKey).toHaveLength(1);
    expect(apiKeys.calls.revokeApiKey[0]!.args).toEqual([
      KEY_ID,
      ACTOR_ID,
      activeKey().version,
    ]);
  });

  it("is idempotent for an already-revoked key (no mutation)", async () => {
    const { apiKeys, authz, controller } = makeHarness();
    authz.canActImpl = async () => true;
    const key = revokedKey();
    apiKeys.getApiKeyImpl = async () => key;

    const result = await controller.revokeApiKey(okRequest());

    expect(result.success).toBe(true);
    expect(result.wasAlreadyRevoked).toBe(true);
    expect(result.revokedAt).toBe(key.revokedAt);
    expect(result.ownerId).toBe(OWNER_ID);
    // The key is already revoked -> no revoke call, no write.
    expect(apiKeys.calls.revokeApiKey).toHaveLength(0);
  });

  it("is idempotent for an expired key", async () => {
    const { apiKeys, authz, controller } = makeHarness();
    authz.canActImpl = async () => true;
    const expired = buildApiKey({
      id: KEY_ID,
      ownerId: OWNER_ID,
      status: "expired",
      revokedAt: FIXED_TIME,
    });
    apiKeys.getApiKeyImpl = async () => expired;

    const result = await controller.revokeApiKey(okRequest());

    expect(result.success).toBe(true);
    expect(result.wasAlreadyRevoked).toBe(true);
    expect(apiKeys.calls.revokeApiKey).toHaveLength(0);
  });

  it("falls back to 'now' when a pre-revoked record has a null revokedAt", async () => {
    const { clock, apiKeys, authz, controller } = makeHarness();
    authz.canActImpl = async () => true;
    // Deliberately inconsistent record: non-active status but no timestamp.
    apiKeys.getApiKeyImpl = async () =>
      buildApiKey({
        id: KEY_ID,
        ownerId: OWNER_ID,
        status: "revoked",
        revokedAt: null,
        revokedBy: null,
      });

    const result = await controller.revokeApiKey(okRequest());

    expect(result.success).toBe(true);
    expect(result.wasAlreadyRevoked).toBe(true);
    expect(result.revokedAt).toBe(clock.now());
    expect(apiKeys.calls.revokeApiKey).toHaveLength(0);
  });
});

describe("revokeApiKey — validation boundary", () => {
  for (const [label, apiKeyId] of [
    ["undefined", undefined],
    ["null", null],
    ["empty string", ""],
    ["whitespace only", "   "],
    ["non-string (number)", 12345],
    ["malformed UUID", "not-a-uuid"],
    ["UUID without dashes", "018f2b5e3a1c4d2e9b7f6a5c4d3e2f10"],
  ]) {
    it(`rejects invalid apiKeyId: ${label}`, async () => {
      const { apiKeys, authz, controller } = makeHarness();
      const result = await controller
        .revokeApiKey(okRequest({ apiKeyId }))
        .then(() => "ok")
        .catch((e) => e);
      expect(result).toBeInstanceOf(ApiKeyError);
      expect((result as ApiKeyError).code).toBe("VALIDATION_ERROR");
      // Validation happens before any service/authorization call.
      expect(apiKeys.calls.getApiKey).toHaveLength(0);
      expect(authz.calls).toHaveLength(0);
    });
  }

  it("rejects an over-long id as VALIDATION_ERROR", async () => {
    const { controller } = makeHarness();
    const longId = `${KEY_ID}${"x".repeat(300)}`;
    await expect(
      controller.revokeApiKey(okRequest({ apiKeyId: longId }))
    ).rejects.toMatchObject({
      code: "VALIDATION_ERROR",
    });
  });
});

describe("revokeApiKey — authorization / permission boundary", () => {
  it("rejects missing actor identity as UNAUTHORIZED", async () => {
    const { controller } = makeHarness();
    await expect(
      controller.revokeApiKey(okRequest({ actorId: undefined }))
    ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  });

  it("rejects empty/whitespace actor identity as UNAUTHORIZED", async () => {
    const { controller } = makeHarness();
    await expect(
      controller.revokeApiKey(okRequest({ actorId: "  " }))
    ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  });

  it("rejects non-string actor identity as UNAUTHORIZED", async () => {
    const { controller } = makeHarness();
    await expect(
      controller.revokeApiKey(okRequest({ actorId: 42 }))
    ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  });

  it("rejected by the authorization service as FORBIDDEN (terminal, no retry)", async () => {
    const { clock, apiKeys, authz, controller } = makeHarness();
    authz.canActImpl = async () => false; // explicitly not permitted
    apiKeys.getApiKeyImpl = async () => activeKey();

    await expect(controller.revokeApiKey(okRequest())).rejects.toMatchObject({
      code: "FORBIDDEN",
    });
    // Forbidden is terminal: authorization is consulted once, no revoke attempted.
    expect(authz.calls).toHaveLength(1);
    expect(apiKeys.calls.getApiKey).toHaveLength(0);
    expect(apiKeys.calls.revokeApiKey).toHaveLength(0);
    expect(clock.scheduledDelays).toHaveLength(0);
  });

  it("rejects an over-long actor id as UNAUTHORIZED before any I/O", async () => {
    const { apiKeys, authz, controller } = makeHarness();
    authz.canActImpl = async () => false;
    apiKeys.getApiKeyImpl = async () => activeKey();

    const err = await controller
      .revokeApiKey(okRequest({ actorId: "x".repeat(300) }))
      .then(() => null)
      .catch((e) => e);

    expect(err).toBeInstanceOf(ApiKeyError);
    expect((err as ApiKeyError).code).toBe("UNAUTHORIZED");
    expect((err as ApiKeyError).message).toBe("Actor identity is too long.");
    expect(authz.calls).toHaveLength(0);
    expect(apiKeys.calls.getApiKey).toHaveLength(0);
    expect(apiKeys.calls.revokeApiKey).toHaveLength(0);
  });
});

describe("revokeApiKey — existence boundary", () => {
  it("rejects an unknown key as NOT_FOUND (no revoke attempted)", async () => {
    const { apiKeys, authz, controller } = makeHarness();
    authz.canActImpl = async () => true;
    apiKeys.getApiKeyImpl = async () => null;

    await expect(controller.revokeApiKey(okRequest())).rejects.toMatchObject({
      code: "NOT_FOUND",
    });
    expect(apiKeys.calls.revokeApiKey).toHaveLength(0);
  });
});

describe("revokeApiKey — transient retry boundary", () => {
  it("retries transient failures with exponential backoff and recovers", async () => {
    const { clock, apiKeys, authz, controller } = makeHarness({
      maxAttempts: 3,
      baseDelayMs: 100,
    });
    authz.canActImpl = async () => true;
    apiKeys.getApiKeyImpl = async () => activeKey();

    let revokeCalls = 0;
    apiKeys.revokeApiKeyImpl = async (id, by) => {
      revokeCalls++;
      if (revokeCalls < 3) {
        throw transientError("upstream timeout");
      }
      return buildRevokedApiKey({ id, revokedBy: by, revokedAt: clock.now() });
    };

    const result = await controller.revokeApiKey(okRequest());

    expect(result.success).toBe(true);
    expect(result.wasAlreadyRevoked).toBe(false);
    expect(revokeCalls).toBe(3);
    // 2 retries -> backoff 100ms then 200ms.
    expect(clock.scheduledDelays).toEqual([100, 200]);
  });

  it("exhausts retries on persistent transient errors and rethrows", async () => {
    const { clock, apiKeys, authz, controller } = makeHarness({
      maxAttempts: 3,
      baseDelayMs: 100,
    });
    authz.canActImpl = async () => true;
    apiKeys.getApiKeyImpl = async () => activeKey();
    apiKeys.revokeApiKeyImpl = async () => {
      throw transientError("service degraded");
    };

    await expect(controller.revokeApiKey(okRequest())).rejects.toThrow(
      /service degraded/
    );
    expect(apiKeys.calls.revokeApiKey).toHaveLength(3);
    expect(clock.scheduledDelays).toEqual([100, 200]);
  });

  it("retries a transient authorization failure, then succeeds", async () => {
    const { clock, apiKeys, authz, controller } = makeHarness({
      maxAttempts: 3,
      baseDelayMs: 100,
    });
    let authCalls = 0;
    authz.canActImpl = async () => {
      authCalls++;
      if (authCalls < 2) {
        throw transientError("auth service hiccup");
      }
      return true;
    };
    apiKeys.getApiKeyImpl = async () => activeKey();
    apiKeys.revokeApiKeyImpl = async (id, by) =>
      buildRevokedApiKey({ id, revokedBy: by, revokedAt: clock.now() });

    const result = await controller.revokeApiKey(okRequest());

    expect(result.success).toBe(true);
    expect(authCalls).toBe(2);
    expect(clock.scheduledDelays).toEqual([100]);
  });
});

describe("revokeApiKey — optimistic-concurrency / stale-state recovery", () => {
  it("recovers to idempotent success when a concurrent actor revoked the key", async () => {
    const { clock, apiKeys, authz, controller } = makeHarness({
      maxAttempts: 3,
      baseDelayMs: 100,
    });
    authz.canActImpl = async () => true;

    // First load sees an active key; after the OCC conflict the re-read sees
    // it already revoked by another actor.
    let getCalls = 0;
    apiKeys.getApiKeyImpl = async () => {
      getCalls++;
      return getCalls === 1 ? activeKey() : revokedKey();
    };

    let revokeCalls = 0;
    apiKeys.revokeApiKeyImpl = async () => {
      revokeCalls++;
      throw new ApiKeyError("CONFLICT", "version changed", {
        action: "revokeApiKey",
      });
    };

    const result = await controller.revokeApiKey(okRequest());

    expect(result.success).toBe(true);
    // Recovered by re-reading; not retried via backoff because it resolved in
    // the first attempt's conflict handler.
    expect(result.wasAlreadyRevoked).toBe(true);
    expect(revokeCalls).toBe(1);
    expect(getCalls).toBe(2);
    expect(clock.scheduledDelays).toHaveLength(0);
  });

  it("retries OCC conflict while key stays active, then succeeds", async () => {
    const { clock, apiKeys, authz, controller } = makeHarness({
      maxAttempts: 3,
      baseDelayMs: 100,
    });
    authz.canActImpl = async () => true;
    apiKeys.getApiKeyImpl = async () => activeKey(); // always active, version unchanged

    let revokeCalls = 0;
    apiKeys.revokeApiKeyImpl = async (id, by) => {
      revokeCalls++;
      if (revokeCalls < 2) {
        throw new ApiKeyError("CONFLICT", "version changed", {
          action: "revokeApiKey",
        });
      }
      return buildRevokedApiKey({ id, revokedBy: by, revokedAt: clock.now() });
    };

    const result = await controller.revokeApiKey(okRequest());

    expect(result.success).toBe(true);
    expect(result.wasAlreadyRevoked).toBe(false);
    expect(revokeCalls).toBe(2);
    // First CONFLICT triggered a backoff before the successful retry.
    expect(clock.scheduledDelays).toEqual([100]);
  });

  it("exhausts OCC retries and surfaces the conflict", async () => {
    const { clock, apiKeys, authz, controller } = makeHarness({
      maxAttempts: 3,
      baseDelayMs: 100,
    });
    authz.canActImpl = async () => true;
    apiKeys.getApiKeyImpl = async () => activeKey();
    apiKeys.revokeApiKeyImpl = async () => {
      throw new ApiKeyError("CONFLICT", "stale", { action: "revokeApiKey" });
    };

    const err = await controller
      .revokeApiKey(okRequest())
      .then(() => null)
      .catch((e) => e);
    expect(err).toBeInstanceOf(ApiKeyError);
    expect((err as ApiKeyError).code).toBe("CONFLICT");
    expect(apiKeys.calls.revokeApiKey).toHaveLength(3);
    expect(clock.scheduledDelays).toEqual([100, 200]);
  });

  it("surfaces NOT_FOUND when the key is removed mid-revoke after a CONFLICT", async () => {
    const { clock, apiKeys, authz, controller } = makeHarness({
      maxAttempts: 3,
      baseDelayMs: 100,
    });
    authz.canActImpl = async () => true;

    let getCalls = 0;
    apiKeys.getApiKeyImpl = async () => {
      getCalls++;
      // First load: active. Re-read during CONFLICT recovery: removed.
      return getCalls === 1 ? activeKey() : null;
    };
    apiKeys.revokeApiKeyImpl = async () => {
      throw new ApiKeyError("CONFLICT", "version changed", {
        action: "revokeApiKey",
      });
    };

    const err = await controller
      .revokeApiKey(okRequest())
      .then(() => null)
      .catch((e) => e);

    expect(err).toBeInstanceOf(ApiKeyError);
    expect((err as ApiKeyError).code).toBe("NOT_FOUND");
    expect((err as ApiKeyError).message).toContain("removed during revocation");
    expect(apiKeys.calls.revokeApiKey).toHaveLength(1);
    expect(getCalls).toBe(2);
    expect(clock.scheduledDelays).toHaveLength(0);
  });
});

describe("revokeApiKey — system-error normalization", () => {
  it("wraps unexpected service exceptions into a non-leaking SYSTEM_ERROR", async () => {
    const { apiKeys, authz, controller } = makeHarness();
    authz.canActImpl = async () => true;
    apiKeys.getApiKeyImpl = async () => activeKey();
    apiKeys.revokeApiKeyImpl = async () => {
      // A raw, unexpected exception from the store.
      throw new Error("database connection lost secret=abc123");
    };

    const err = await controller
      .revokeApiKey(okRequest({ requestId: "req_sys_1" }))
      .then(() => null)
      .catch((e) => e);

    expect(err).toBeInstanceOf(ApiKeyError);
    expect((err as ApiKeyError).code).toBe("SYSTEM_ERROR");
    // The raw message must not leak into the user-facing message.
    expect((err as ApiKeyError).message).not.toContain("secret=abc123");
    expect((err as ApiKeyError).message).not.toContain(
      "database connection lost"
    );
    // No secret material in the exposed context either.
    const ctx = (err as ApiKeyError).context;
    expect(JSON.stringify(ctx)).not.toContain("abc123");
    // Single attempt; raw errors are not retried.
    expect(apiKeys.calls.revokeApiKey).toHaveLength(1);
  });

  it("invokes the onError observer with a sanitized error for every failure", async () => {
    const observed: ApiKeyError[] = [];
    const { apiKeys, authz, controller } = makeHarness({
      onError: (err) => observed.push(err),
    });
    authz.canActImpl = async () => false; // FORBIDDEN (terminal)
    apiKeys.getApiKeyImpl = async () => activeKey();

    const err = await controller
      .revokeApiKey(okRequest({ requestId: "req_obs_1" }))
      .then(() => null)
      .catch((e) => e);

    expect(err).toBeInstanceOf(ApiKeyError);
    expect(observed).toHaveLength(1);
    expect(observed[0]).toBe(err);
    expect((observed[0] as ApiKeyError).code).toBe("FORBIDDEN");
    expect((observed[0] as ApiKeyError).context.requestId).toBe("req_obs_1");
    expect((observed[0] as ApiKeyError).context.apiKeyId).toBe(KEY_ID);
    // Observer payload must never carry secret material.
    expect(JSON.stringify(observed[0])).not.toContain("secret");
  });
});

describe("revokeApiKey — concurrency boundary", () => {
  it("deduplicates concurrent revocations of the same key to one execution", async () => {
    const { clock, apiKeys, authz, controller } = makeHarness();
    authz.canActImpl = async () => true;
    apiKeys.getApiKeyImpl = async () => activeKey();

    const def = deferred();
    apiKeys.revokeApiKeyImpl = async (id, by) => {
      return def.promise.then(() =>
        buildRevokedApiKey({ id, revokedBy: by, revokedAt: clock.now() })
      );
    };

    // Fire two callers "at once" before the underlying op resolves.
    const p1 = controller.revokeApiKey(okRequest({ requestId: "r1" }));
    const p2 = controller.revokeApiKey(okRequest({ requestId: "r2" }));

    // Both calls enter the boundary synchronously; flush the microtask queue so
    // the in-flight execution advances to the (pending) store call.
    await flush();

    // Both are pending and share the same in-flight execution.
    expect(controller.isRevokePending(KEY_ID)).toBe(true);
    expect(apiKeys.calls.revokeApiKey).toHaveLength(1);

    def.resolve(undefined);
    const [r1, r2] = await Promise.all([p1, p2]);

    expect(r1).toEqual(r2);
    expect(r1.success).toBe(true);
    expect(r1.wasAlreadyRevoked).toBe(false);
    // Even though two callers raced, the store was written exactly once.
    expect(apiKeys.calls.revokeApiKey).toHaveLength(1);
    expect(controller.isRevokePending(KEY_ID)).toBe(false);
  });

  it("does not dedupe revocations of distinct keys", async () => {
    const { clock, apiKeys, authz, controller } = makeHarness();
    authz.canActImpl = async () => true;
    const keyA = activeKey({ id: "018f2b5e-3a1c-4d2e-9b7f-6a5c4d3e2f10" });
    const keyB = activeKey({ id: "018f2b5e-3a1c-4d2e-9b7f-6a5c4d3e2f11" });
    apiKeys.getApiKeyImpl = async (id: string) =>
      id === keyA.id ? keyA : keyB;
    apiKeys.revokeApiKeyImpl = async (id, by) =>
      buildRevokedApiKey({ id, revokedBy: by, revokedAt: clock.now() });

    const [a, b] = await Promise.all([
      controller.revokeApiKey(okRequest({ apiKeyId: keyA.id })),
      controller.revokeApiKey(okRequest({ apiKeyId: keyB.id })),
    ]);

    expect(a.success && b.success).toBe(true);
    expect(apiKeys.calls.revokeApiKey).toHaveLength(2);
  });
});

describe("revokeApiKey — request id / diagnostics", () => {
  it("surfaces the caller-supplied request id in error context", async () => {
    const { apiKeys, authz, controller } = makeHarness();
    authz.canActImpl = async () => false; // FORBIDDEN
    apiKeys.getApiKeyImpl = async () => activeKey();

    const err = await controller
      .revokeApiKey(okRequest({ requestId: "trace-abc-123" }))
      .then(() => null)
      .catch((e) => e);

    expect((err as ApiKeyError).context.requestId).toBe("trace-abc-123");
    expect((err as ApiKeyError).context.apiKeyId).toBe(KEY_ID);
    expect((err as ApiKeyError).context.actorId).toBe(ACTOR_ID);
    expect((err as ApiKeyError).context.action).toBe("revokeApiKey");
  });

  it("auto-generates a request id when none is supplied", async () => {
    const { apiKeys, authz, controller } = makeHarness();
    authz.canActImpl = async () => true;
    apiKeys.getApiKeyImpl = async () => null; // NOT_FOUND

    const err = await controller
      .revokeApiKey({ apiKeyId: KEY_ID, actorId: ACTOR_ID })
      .then(() => null)
      .catch((e) => e);

    expect((err as ApiKeyError).context.requestId).toMatch(/^req_/);
  });
});

describe("revokeApiKey — InMemoryApiKeyService integration", () => {
  it("revokes through the real OCC service and bumps the version", async () => {
    const clock = new FrozenClock();
    const apiKeys = new InMemoryApiKeyService(clock);
    apiKeys.create({
      id: KEY_ID,
      name: "Prod Key",
      ownerId: OWNER_ID,
      permissions: ["read"],
      createdAt: FIXED_TIME,
    });
    const authz = new StubAuthorizationService();
    authz.canActImpl = async () => true;
    const controller = new ApiKeysController(apiKeys, authz, clock, {
      maxAttempts: 3,
      baseDelayMs: 100,
    });

    const before = await apiKeys.getApiKey(KEY_ID);
    expect(before?.version).toBe(1);
    expect(before?.status).toBe("active");

    const result = await controller.revokeApiKey(okRequest());

    expect(result.success).toBe(true);
    expect(result.wasAlreadyRevoked).toBe(false);
    expect(result.revokedAt).toBe(clock.now());

    // Persisted state reflects the revocation.
    const after = await apiKeys.getApiKey(KEY_ID);
    expect(after).not.toBeNull();
    expect(after!.status).toBe("revoked");
    expect(after!.revokedBy).toBe(ACTOR_ID);
    expect(after!.version).toBe(2);

    // Idempotent second call must not mutate again.
    const again = await controller.revokeApiKey(okRequest());
    expect(again.wasAlreadyRevoked).toBe(true);
    expect((await apiKeys.getApiKey(KEY_ID))!.version).toBe(2);
  });

  it("InMemoryApiKeyService rejects stale-version revokes (OCC)", async () => {
    const clock = new FrozenClock();
    const apiKeys = new InMemoryApiKeyService(clock);
    apiKeys.create({
      id: KEY_ID,
      name: "Prod Key",
      ownerId: OWNER_ID,
      permissions: ["read"],
      createdAt: FIXED_TIME,
    });

    const loaded = await apiKeys.getApiKey(KEY_ID);
    expect(loaded).not.toBeNull();
    expect(loaded!.version).toBe(1);

    // A different actor mutates the key, advancing the version.
    await apiKeys.revokeApiKey(KEY_ID, OWNER_ID);
    expect((await apiKeys.getApiKey(KEY_ID))!.version).toBe(2);

    // The original actor, still holding version 1, cannot revoke (stale read).
    await expect(
      apiKeys.revokeApiKey(KEY_ID, OWNER_ID, loaded!.version)
    ).rejects.toMatchObject({ code: "CONFLICT" });

    // Re-reading yields version 2; revoking with the fresh version is idempotent
    // (already revoked) and does not throw.
    const fresh = await apiKeys.getApiKey(KEY_ID);
    expect(fresh!.version).toBe(2);
    const outcome = await apiKeys.revokeApiKey(
      KEY_ID,
      OWNER_ID,
      fresh!.version
    );
    expect(outcome.status).toBe("revoked");
  });
});

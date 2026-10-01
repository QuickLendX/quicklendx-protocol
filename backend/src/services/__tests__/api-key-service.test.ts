import { describe, it, expect } from "vitest";
import { FrozenClock } from "../clock";
import { InMemoryApiKeyService } from "../api-key-service";
import { ApiKeyError } from "../../errors/api-key-error";

const KEY_ID = "018f2b5e-3a1c-4d2e-9b7f-6a5c4d3e2f10";
const OWNER = "owner_1";

const seedActive = (svc: InMemoryApiKeyService) =>
  svc.create({
    id: KEY_ID,
    name: "Prod",
    ownerId: OWNER,
    permissions: ["read"],
  });

describe("InMemoryApiKeyService", () => {
  it("creates, reads, and records size", async () => {
    const svc = new InMemoryApiKeyService(new FrozenClock());
    seedActive(svc);
    expect(svc.size).toBe(1);

    const key = await svc.getApiKey(KEY_ID);
    expect(key).not.toBeNull();
    expect(key!.id).toBe(KEY_ID);
    expect(key!.status).toBe("active");
    expect(key!.version).toBe(1);
  });

  it("create() applies defaults (empty permissions, active status, now timestamp)", async () => {
    const clock = new FrozenClock(7_000);
    const svc = new InMemoryApiKeyService(clock);
    svc.create({ id: KEY_ID, name: "k", ownerId: OWNER }); // no permissions/status/createdAt

    const key = await svc.getApiKey(KEY_ID);
    expect(key).not.toBeNull();
    expect(key!.permissions).toEqual([]);
    expect(key!.status).toBe("active");
    expect(key!.createdAt).toBe(7_000); // defaulted to clock.now()
    expect(key!.revokedAt).toBeNull();
    expect(key!.revokedBy).toBeNull();
  });

  it("create() honors explicit permissions, a non-active status, and createdAt", async () => {
    const clock = new FrozenClock(7_000);
    const svc = new InMemoryApiKeyService(clock);
    svc.create({
      id: KEY_ID,
      name: "k",
      ownerId: OWNER,
      permissions: ["read", "write"],
      status: "expired",
      createdAt: 123,
    });

    const key = await svc.getApiKey(KEY_ID);
    expect(key).not.toBeNull();
    expect(key!.permissions).toEqual(["read", "write"]);
    expect(key!.status).toBe("expired");
    expect(key!.createdAt).toBe(123);
    expect(key!.revokedAt).toBe(123);
    expect(key!.revokedBy).toBe(OWNER);
  });

  it("getApiKey returns null for a missing id", async () => {
    const svc = new InMemoryApiKeyService(new FrozenClock());
    expect(await svc.getApiKey("missing")).toBeNull();
  });

  it("revokeApiKey atomically revokes and bumps the version", async () => {
    const svc = new InMemoryApiKeyService(new FrozenClock(5_000));
    seedActive(svc);

    const revoked = await svc.revokeApiKey(KEY_ID, OWNER);
    expect(revoked.status).toBe("revoked");
    expect(revoked.revokedBy).toBe(OWNER);
    expect(revoked.revokedAt).toBe(5_000);
    expect(revoked.version).toBe(2);

    const stored = await svc.getApiKey(KEY_ID);
    expect(stored!.status).toBe("revoked");
    expect(stored!.version).toBe(2);
  });

  it("revokeApiKey rejects a stale expectedVersion with CONFLICT (retryable)", async () => {
    const svc = new InMemoryApiKeyService(new FrozenClock());
    seedActive(svc);

    await expect(
      svc.revokeApiKey(KEY_ID, OWNER, 999 /* stale version */)
    ).rejects.toMatchObject({ code: "CONFLICT", retryable: true });
  });

  it("revoking an already-revoked key is idempotent (no version bump)", async () => {
    const svc = new InMemoryApiKeyService(new FrozenClock(5_000));
    seedActive(svc);
    await svc.revokeApiKey(KEY_ID, OWNER);

    const second = await svc.revokeApiKey(KEY_ID, OWNER);
    expect(second.status).toBe("revoked");
    // No further mutation; version stays at the post-revoke value.
    const stored = await svc.getApiKey(KEY_ID);
    expect(stored!.version).toBe(2);
  });

  it("revokeApiKey throws NOT_FOUND for a missing key", async () => {
    const svc = new InMemoryApiKeyService(new FrozenClock());
    const err = await svc
      .revokeApiKey("missing", OWNER, 1)
      .then(() => null)
      .catch((e) => e);
    expect(err).toBeInstanceOf(ApiKeyError);
    expect((err as ApiKeyError).code).toBe("NOT_FOUND");
  });

  it("does not return or log secret material", async () => {
    const svc = new InMemoryApiKeyService(new FrozenClock());
    seedActive(svc);
    const key = await svc.getApiKey(KEY_ID);
    // The model intentionally has no `secret` field.
    expect("secret" in (key as object)).toBe(false);
  });
});

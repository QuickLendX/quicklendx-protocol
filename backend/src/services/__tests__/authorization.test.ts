import { describe, it, expect } from "vitest";
import { FrozenClock } from "../clock";
import { AuthorizationService } from "../authorization";
import { InMemoryApiKeyService } from "../api-key-service";

describe("AuthorizationService.ownerBased", () => {
  const build = () => {
    const clock = new FrozenClock();
    const apiKeys = new InMemoryApiKeyService(clock);
    const ownerId = "owner_1";
    apiKeys.create({
      id: "018f2b5e-3a1c-4d2e-9b7f-6a5c4d3e2f10",
      name: "key",
      ownerId,
      permissions: ["read"],
    });
    const authz = AuthorizationService.ownerBased(
      async (id) => (await apiKeys.getApiKey(id))?.ownerId ?? null
    );
    return { apiKeys, authz, ownerId };
  };

  it("allows the owner to revoke", async () => {
    const { authz, ownerId } = build();
    await expect(
      authz.canAct(ownerId, "revoke", "018f2b5e-3a1c-4d2e-9b7f-6a5c4d3e2f10")
    ).resolves.toBe(true);
  });

  it("forbids a non-owner", async () => {
    const { authz } = build();
    await expect(
      authz.canAct(
        "someone-else",
        "revoke",
        "018f2b5e-3a1c-4d2e-9b7f-6a5c4d3e2f10"
      )
    ).resolves.toBe(false);
  });

  it("forbids unknown / non-existent keys", async () => {
    const { authz } = build();
    await expect(
      authz.canAct("anyone", "revoke", "nonexistent-id")
    ).resolves.toBe(false);
  });

  it("does not permit non-revoke actions even for the owner", async () => {
    const { authz, ownerId } = build();
    await expect(
      authz.canAct(ownerId, "rotate", "018f2b5e-3a1c-4d2e-9b7f-6a5c4d3e2f10")
    ).resolves.toBe(false);
  });
});

/**
 * Authorization contract for API-key operations.
 *
 * Implementations decide whether `actorId` may perform `action` on the key
 * identified by `keyId`. The controller treats a `false` result as a terminal
 * `FORBIDDEN` outcome (no retry).
 */
export type ApiKeyAction = "revoke" | "rotate";

export interface AuthorizationService {
  canAct(
    actorId: string,
    action: ApiKeyAction,
    keyId: string
  ): Promise<boolean>;
}

/** Factory namespace with ready-made strategies. */
export const AuthorizationService = {
  /** Owner-based policy: the key's owner may revoke; everyone else is forbidden. */
  ownerBased(
    resolveOwnerId: (keyId: string) => Promise<string | null>
  ): AuthorizationService {
    return {
      async canAct(actorId, action, keyId) {
        if (action !== "revoke") return false;
        const owner = await resolveOwnerId(keyId);
        return owner !== null && owner === actorId;
      },
    };
  },
};

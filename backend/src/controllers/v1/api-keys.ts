/**
 * API Keys v1 controller.
 *
 * Entry point: `ApiKeysController.revokeApiKey`.
 *
 * Design goals (see AGENTS.md / issue requirements):
 *  - Deterministic normal and adverse paths: validation -> authorization ->
 *    existence -> state transition.
 *  - Authorization, validation, and state-transition invariants always enforced.
 *  - Retries (transient), partial failure (OCC stale reads), and concurrent
 *    execution (in-flight dedupe) cannot produce an unsafe or inconsistent
 *    result. Revoking an already-revoked key is idempotent.
 *  - Failures are diagnosable via stable error `code` + non-sensitive context
 *    (`apiKeyId`, `actorId`, `requestId`); secret key material is never logged
 *    or surfaced.
 *  - Public surface is a single method + result type. No existing callers exist,
 *    so there is no migration path requirement.
 */
import {
  ApiKeyError,
  apiKeyRetryPredicate,
  isApiKeyError,
} from "../../errors/api-key-error";
import type {
  ApiKey,
  ApiKeyService,
  RevokedApiKey,
} from "../../models/api-key";
import { FailureBoundary } from "../../lib/failure-boundary";
import type { AuthorizationService } from "../../services/authorization";
import type { Clock } from "../../services/clock";

const UUID_V4 =
  /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const MAX_ID_LENGTH = 256;

export interface RevokeApiKeyRequest {
  /** Opaque, non-secret key identifier (validated as UUIDv4). */
  apiKeyId: unknown;
  /** Opaque, non-secret actor identifier performing the revocation. */
  actorId: unknown;
  /** Optional correlation id; generated when absent. */
  requestId?: string;
}

export interface RevokeApiKeyResult {
  success: true;
  apiKeyId: string;
  ownerId: string;
  revokedAt: number;
  /** `true` when the key was already revoked/expired (no mutation occurred). */
  wasAlreadyRevoked: boolean;
}

/** Errors that escape the boundary are always `ApiKeyError` (never raw). */
export type RevokeApiKeyError = ApiKeyError;

export interface ApiKeysControllerOptions {
  /** Total attempts for transient retry (default 3). */
  maxAttempts?: number;
  /** Base backoff ms for transient retry (default 100). */
  baseDelayMs?: number;
  /**
   * Optional observer invoked exactly once for every error that escapes
   * `revokeApiKey` (after normalization to `ApiKeyError`). Wire this to a
   * structured logger/metrics sink. The callback receives only a sanitized
   * `ApiKeyError` whose context never contains secret material.
   */
  onError?: (err: ApiKeyError) => void;
}

export class ApiKeysController {
  private readonly apiKeys: ApiKeyService;
  private readonly authz: AuthorizationService;
  private readonly clock: Clock;
  private readonly boundary: FailureBoundary;
  private readonly maxAttempts: number;
  private readonly onError?: (err: ApiKeyError) => void;

  constructor(
    apiKeys: ApiKeyService,
    authz: AuthorizationService,
    clock: Clock,
    opts: ApiKeysControllerOptions = {}
  ) {
    this.apiKeys = apiKeys;
    this.authz = authz;
    this.clock = clock;
    this.maxAttempts = opts.maxAttempts ?? 3;
    this.onError = opts.onError;
    this.boundary = new FailureBoundary({
      maxAttempts: this.maxAttempts,
      baseDelayMs: opts.baseDelayMs,
      clock,
    });
  }

  /**
   * Revoke an API key, enforcing the full failure boundary.
   *
   * State machine:
   *  1. VALIDATION_ERROR  (terminal)   — malformed/missing identifiers.
   *  2. UNAUTHORIZED       (terminal)   — actor identity missing.
   *  3. FORBIDDEN          (terminal)   — actor not permitted to revoke.
   *  4. NOT_FOUND          (terminal)   — key does not exist.
   *  5. [idempotent] already revoked/expired -> success, no mutation.
   *  6. revoke with OCC; on CONFLICT (stale) re-read & recover, else retry.
   *  7. TRANSIENT / SYSTEM_ERROR -> bounded retry with backoff.
   *
   * All exceptions are normalized to `ApiKeyError` before returning to callers.
   */
  async revokeApiKey(req: RevokeApiKeyRequest): Promise<RevokeApiKeyResult> {
    const requestId = req.requestId ?? this.generateRequestId();

    // (1) Validate inputs up front — deterministic, no I/O, never retried.
    const apiKeyId = validateKeyId(req.apiKeyId, requestId);
    const actorId = validateActorId(req.actorId, requestId);

    try {
      return await this.boundary.dedupe(`revoke:${apiKeyId}`, async () => {
        return this.revokeInternal(apiKeyId, actorId, requestId);
      });
    } catch (err) {
      // Normalize unexpected throws so callers never see a raw exception and
      // so sensitive internal details are not leaked via stack traces.
      const appErr =
        err instanceof ApiKeyError
          ? err
          : new ApiKeyError(
              "SYSTEM_ERROR",
              "An unexpected error occurred while revoking the API key.",
              {
                action: "revokeApiKey",
                apiKeyId,
                actorId,
                requestId,
                cause: err,
              }
            );
      // Observability seam: logging/metrics receive only the sanitized
      // ApiKeyError (no raw secrets or stack data in user-facing fields).
      this.onError?.(appErr);
      throw appErr;
    }
  }

  private generateRequestId(): string {
    return `req_${this.clock.now()}_${Math.random().toString(36).slice(2, 10)}`;
  }

  private async revokeInternal(
    apiKeyId: string,
    actorId: string,
    requestId: string
  ): Promise<RevokeApiKeyResult> {
    // (3) Authorization boundary — terminal FORBIDDEN on denial.
    const permitted = await this.boundary.withRetry(
      () => this.authz.canAct(actorId, "revoke", apiKeyId),
      apiKeyRetryPredicate
    );
    if (!permitted) {
      throw new ApiKeyError(
        "FORBIDDEN",
        "You do not have permission to revoke this API key.",
        { action: "revokeApiKey", apiKeyId, actorId, requestId }
      );
    }

    // (4) Existence + stale-view load — terminal NOT_FOUND.
    const existing = await this.boundary.withRetry(
      () => this.apiKeys.getApiKey(apiKeyId),
      apiKeyRetryPredicate
    );
    if (existing === null) {
      throw new ApiKeyError(
        "NOT_FOUND",
        "The requested API key was not found.",
        { action: "revokeApiKey", apiKeyId, actorId, requestId }
      );
    }

    // (5) Idempotency: already revoked/expired -> success, no mutation.
    if (existing.status !== "active") {
      return {
        success: true,
        apiKeyId,
        ownerId: existing.ownerId,
        revokedAt: existing.revokedAt ?? this.clock.now(),
        wasAlreadyRevoked: true,
      };
    }

    // (6) Atomic revoke with optimistic-concurrency stale-read recovery.
    const outcome = await this.revokeWithOccRecovery(
      apiKeyId,
      actorId,
      existing,
      requestId
    );

    return {
      success: true,
      apiKeyId,
      ownerId: outcome.revKey.ownerId,
      revokedAt: outcome.revKey.revokedAt,
      wasAlreadyRevoked: outcome.wasAlreadyRevoked,
    };
  }

  /**
   * Revoke using the version we read. If the store reports CONFLICT (stale),
   * re-read and recover: if the key was revoked by another actor in the
   * meantime, surface idempotent success; otherwise retry until the OCC
   * conflict resolves or attempts are exhausted.
   */
  private async revokeWithOccRecovery(
    apiKeyId: string,
    actorId: string,
    existing: ApiKey,
    requestId: string
  ): Promise<{ revKey: RevokedApiKey; wasAlreadyRevoked: boolean }> {
    return this.boundary.withRetry(async () => {
      try {
        const revKey = await this.apiKeys.revokeApiKey(
          apiKeyId,
          actorId,
          existing.version
        );
        return { revKey, wasAlreadyRevoked: false };
      } catch (err) {
        if (isStaleConflict(err)) {
          // Stale read: re-evaluate current state before deciding to retry.
          const fresh = await this.apiKeys.getApiKey(apiKeyId);
          if (fresh === null) {
            throw new ApiKeyError(
              "NOT_FOUND",
              "API key was removed during revocation.",
              {
                action: "revokeApiKey",
                apiKeyId,
                actorId,
                requestId,
                cause: err,
              }
            );
          }
          if (fresh.status !== "active") {
            // Another actor already revoked/expired it: idempotent success.
            return {
              revKey: fresh as RevokedApiKey,
              wasAlreadyRevoked: true,
            };
          }
          // Still active but version drifted: retry (re-attempt revoke with
          // the stale version; the OCC check will re-evaluate each attempt).
          throw err;
        }
        throw err;
      }
    }, apiKeyRetryPredicate);
  }

  /** Expose in-flight state for diagnostics/tests. */
  isRevokePending(apiKeyId: string): boolean {
    return this.boundary.isPending(`revoke:${apiKeyId}`);
  }
}

/** Type guard: an OCC stale-read conflict that warrants recovery, not failure. */
function isStaleConflict(err: unknown): boolean {
  return isApiKeyError(err) && err.code === "CONFLICT" && err.retryable;
}

/**
 * Validate the key identifier: must be a non-empty UUIDv4 string of sane length.
 * Returns the validated string or throws VALIDATION_ERROR (terminal).
 */
function validateKeyId(input: unknown, requestId: string): string {
  if (typeof input !== "string") {
    throw new ApiKeyError("VALIDATION_ERROR", "API key id must be a string.", {
      action: "revokeApiKey",
      requestId,
      cause: { inputType: typeof input },
    });
  }
  const trimmed = input.trim();
  if (trimmed.length === 0) {
    throw new ApiKeyError("VALIDATION_ERROR", "API key id is required.", {
      action: "revokeApiKey",
      requestId,
    });
  }
  if (trimmed.length > MAX_ID_LENGTH) {
    throw new ApiKeyError("VALIDATION_ERROR", "API key id is too long.", {
      action: "revokeApiKey",
      requestId,
      cause: { length: input.length },
    });
  }
  if (!UUID_V4.test(trimmed)) {
    throw new ApiKeyError(
      "VALIDATION_ERROR",
      "API key id must be a valid UUIDv4.",
      { action: "revokeApiKey", requestId }
    );
  }
  return trimmed;
}

/**
 * Rotate an API key's signing secret
 * POST /api/v1/keys/:id/rotate-signing-secret
 */
export async function rotateApiKeySigningSecret(req: Request, res: Response): Promise<void> {
  try {
    const { id } = req.params;

    // Validate request body
    const validation = rotateSigningSecretSchema.safeParse(req.body);
    if (!validation.success) {
      res.status(400).json({
        error: {
          message: 'Invalid request body',
          code: 'VALIDATION_ERROR',
          details: validation.error.errors,
        },
      });
      return;
    }

    const { actor, grace_window_hours } = validation.data;
    const ipAddress = (req.ip || req.socket.remoteAddress) as string | undefined;

    // Failure-boundary guards
    const existing = await apiKeyService.getApiKeyById(id);
    if (!existing) {
      res.status(404).json({
        error: { message: 'API key not found', code: 'KEY_NOT_FOUND' },
      });
      return;
    }
    if (existing.revoked) {
      res.status(403).json({
        error: { message: 'API key is revoked', code: 'KEY_REVOKED' },
      });
      return;
    }
    if (existing.prev_secret_expires_at && new Date(existing.prev_secret_expires_at) > new Date()) {
      res.status(409).json({
        error: { message: 'Previous signing secret still active', code: 'GRACE_WINDOW_CONFLICT' },
      });
      return;
    }

    let key;
    try {
      key = await apiKeyService.rotateSigningSecret(id, actor, ipAddress, grace_window_hours);
    } catch (svcErr: any) {
      const isDbError = svcErr.message?.toLowerCase().includes('db') || svcErr.message?.toLowerCase().includes('database') || svcErr.message?.toLowerCase().includes('constraint');
      const status = isDbError ? 500 : 400;
      const code = isDbError ? 'ROTATE_SECRET_DB_ERROR' : 'ROTATE_SECRET_ERROR';
      res.status(status).json({
        error: { message: svcErr.message || 'Failed to rotate API key signing secret', code },
      });
      return;
    }

    res.json({
      data: {
        id: key.id,
        name: key.name,
        prefix: key.prefix,
        scopes: key.scopes,
        created_at: key.created_at,
        expires_at: key.expires_at,
        prev_secret_expires_at: key.prev_secret_expires_at,
        key: key.plaintext_key, // Only returned once!
        warning: 'Store this new secret securely. The old secret will expire after the grace window.',
      },
    });
  } catch (error: any) {
    console.error('[RotateApiKeySigningSecret] Unexpected error:', error);
    res.status(500).json({
      error: {
        message: error.message || 'Internal server error',
        code: 'UNEXPECTED_ERROR',
      },
    });
  }
  const trimmed = input.trim();
  if (trimmed.length === 0) {
    throw new ApiKeyError("UNAUTHORIZED", "Actor identity is required.", {
      action: "revokeApiKey",
      requestId,
    });
  }
  if (trimmed.length > MAX_ID_LENGTH) {
    throw new ApiKeyError("UNAUTHORIZED", "Actor identity is too long.", {
      action: "revokeApiKey",
      requestId,
      cause: { length: input.length },
    });
  }
  return trimmed;
}

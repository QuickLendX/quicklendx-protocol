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
import { SystemClock } from "../../services/clock";
import type { Request, Response } from "express";
import { z } from "zod";
import { apiKeyService } from "../../services/api-key-service";
import { auditLogService } from "../../services/audit-log";
import { SCOPE_REGISTRY, hasRequiredScopes } from "../../config/scopes";

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
  private readonly apiKeys: Pick<ApiKeyService, "getApiKey" | "revokeApiKey">;
  private readonly authz: AuthorizationService;
  private readonly clock: Clock;
  private readonly boundary: FailureBoundary;
  private readonly maxAttempts: number;
  private readonly onError?: (err: ApiKeyError) => void;

  constructor(
    apiKeys: Pick<ApiKeyService, "getApiKey" | "revokeApiKey">,
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
          // Retry with the newly observed version after an active-key mutation.
          existing = fresh;
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

function validateActorId(input: unknown, requestId: string): string {
  if (typeof input !== "string" || !input.trim()) {
    throw new ApiKeyError("UNAUTHORIZED", "Actor identity is required.", { requestId });
  }
  if (input.trim().length > MAX_ID_LENGTH) {
    throw new ApiKeyError("UNAUTHORIZED", "Actor identity is too long.", { requestId });
  }
  return input.trim();
}

// Never serialize a service record directly: signing hashes are HMAC material.
function publicKey(key: ApiKey) {
  return {
    id: key.id, name: key.name, prefix: key.prefix, scopes: key.scopes,
    created_by: key.created_by, created_at: key.created_at, last_used_at: key.last_used_at,
    expires_at: key.expires_at, revoked: key.revoked, status: key.status, version: key.version,
    revokedAt: key.revokedAt, revokedBy: key.revokedBy, prev_secret_expires_at: key.prev_secret_expires_at,
  };
}

function sendFailure(res: Response, error: unknown): void {
  const status = error instanceof ApiKeyError ? {
    VALIDATION_ERROR: 400, UNAUTHORIZED: 401, FORBIDDEN: 403, NOT_FOUND: 404,
    CONFLICT: 409, TRANSIENT: 503, SYSTEM_ERROR: 500,
  }[error.code] : 500;
  res.status(status).json({ error: {
    code: error instanceof ApiKeyError ? error.code : "SYSTEM_ERROR",
    message: error instanceof ApiKeyError ? error.message : "API key operation failed.",
  } });
}

function actor(req: Request): string {
  if (!req.apiKey || !hasRequiredScopes(req.apiKey.scopes, ["admin:keys"])) {
    throw new ApiKeyError("UNAUTHORIZED", "An admin API key is required.");
  }
  return req.apiKey.created_by;
}
function keyId(req: Request): string {
  return validateKeyId(req.params.id, "api-key-management");
}

const createSchema = z.object({
  name: z.string().trim().min(1).max(256), scopes: z.array(z.string()),
  expires_at: z.string().datetime().nullable().optional(),
});

export async function createApiKey(req: Request, res: Response): Promise<void> {
  try {
    const created_by = actor(req);
    const input = createSchema.safeParse(req.body);
    if (!input.success) throw new ApiKeyError("VALIDATION_ERROR", "Invalid API key input.");
    const key = await apiKeyService.createApiKey({ ...input.data, created_by }, req.ip);
    res.status(201).json({ data: { ...publicKey(key), plaintext_key: key.plaintext_key,
      plaintext_signing_secret: key.plaintext_signing_secret } });
  } catch (error) { sendFailure(res, error); }
}
export async function listApiKeys(req: Request, res: Response): Promise<void> {
  try {
    actor(req);
    const filters = z.object({ created_by: z.string().optional(), revoked: z.enum(["true", "false"]).optional() }).safeParse(req.query);
    if (!filters.success) throw new ApiKeyError("VALIDATION_ERROR", "Invalid filters.");
    const keys = await apiKeyService.listApiKeys({ created_by: filters.data.created_by,
      revoked: filters.data.revoked === undefined ? undefined : filters.data.revoked === "true" });
    res.json({ data: keys.map(publicKey) });
  } catch (error) { sendFailure(res, error); }
}
export async function getApiKey(req: Request, res: Response): Promise<void> {
  try {
    actor(req);
    const key = await apiKeyService.getApiKeyById(keyId(req));
    if (!key) throw new ApiKeyError("NOT_FOUND", "API key not found.");
    res.json({ data: publicKey(key) });
  } catch (error) { sendFailure(res, error); }
}
export async function rotateApiKey(req: Request, res: Response): Promise<void> {
  try {
    const key = await apiKeyService.rotateApiKey(keyId(req), actor(req), req.ip);
    res.json({ data: { ...publicKey(key), plaintext_key: key.plaintext_key,
      plaintext_signing_secret: key.plaintext_signing_secret } });
  } catch (error) { sendFailure(res, error); }
}
export async function revokeApiKey(req: Request, res: Response): Promise<void> {
  try {
    const actorId = actor(req);
    // Per-request authorization avoids sharing an in-flight result across actors.
    const controller = new ApiKeysController(apiKeyService, {
      canAct: async candidate => candidate === actorId && !!req.apiKey && hasRequiredScopes(req.apiKey.scopes, ["admin:keys"]),
    }, new SystemClock());
    res.json({ data: await controller.revokeApiKey({ apiKeyId: keyId(req), actorId }) });
  } catch (error) { sendFailure(res, error); }
}
export async function getKeyAuditLogs(req: Request, res: Response): Promise<void> {
  try {
    actor(req);
    const id = keyId(req);
    if (!await apiKeyService.getApiKeyById(id)) throw new ApiKeyError("NOT_FOUND", "API key not found.");
    res.json({ data: auditLogService.getLogsForKey(id) });
  } catch (error) { sendFailure(res, error); }
}
export function getScopes(_req: Request, res: Response): void {
  try {
    res.json({ data: SCOPE_REGISTRY, count: SCOPE_REGISTRY.length });
  } catch {
    console.error("[ApiKeys] Failed to serialize scopes");
    res.status(500).json({ error: { message: "Failed to get scopes", code: "GET_SCOPES_ERROR" } });
  }
}
export async function rotateApiKeySigningSecret(req: Request, res: Response): Promise<void> {
  try {
    const input = z.object({ grace_window_hours: z.number().min(0).max(168).optional() }).safeParse(req.body ?? {});
    if (!input.success) throw new ApiKeyError("VALIDATION_ERROR", "Invalid grace window.");
    const key = await apiKeyService.rotateSigningSecret(keyId(req), actor(req), req.ip, input.data.grace_window_hours);
    res.json({ data: { ...publicKey(key), plaintext_signing_secret: key.plaintext_signing_secret } });
  } catch (error) { sendFailure(res, error); }
}

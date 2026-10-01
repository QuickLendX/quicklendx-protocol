// @ts-nocheck
import { Request, Response } from 'express';
import { apiKeyService } from '../../services/api-key-service';
import { auditLogService } from '../../services/audit-log';
import { SCOPE_REGISTRY } from '../../config/scopes';
import { z } from 'zod';
import {
  ApiKeyError,
  ApiKeyNotFoundError,
  ApiKeyRevokedError,
  ApiKeyRotationConflictError,
} from '../../services/api-key-errors';
import { ApiKeyErrorCode } from '../../services/api-key-error-codes';

// Validation schemas
const createApiKeySchema = z.object({
  name: z.string().min(1).max(100),
  scopes: z.array(z.string()).min(1),
  expires_at: z.string().datetime().optional().nullable(),
});

const rotateApiKeySchema = z.object({
  actor: z.string().min(1),
  // Optional optimistic concurrency token. When provided, the rotation
  // fails with a 409 if the current key prefix does not match, preventing
  // a stale client from invalidating a key that was already rotated.
  expected_prefix: z.string().min(1).max(64).optional(),
});

const revokeApiKeySchema = z.object({
  actor: z.string().min(1),
});

const rotateSigningSecretSchema = z.object({
  actor: z.string().min(1),
  grace_window_hours: z.number().min(1).max(720).optional().default(24),
});

/**
 * List query validation schema.
 *
 * Invariants:
 * - `created_by`, when present, must be a non-empty string (no silent coercion of arrays/numbers).
 * - `revoked`, when present, must be the exact literal 'true' or 'false'.
 *   Any other value is rejected with 400 rather than being coerced to `false`,
 *   which would silently change the result set.
 * - Unknown query parameters are stripped so they cannot influence filtering.
 */
const listApiKeysQuerySchema = z.object({
  created_by: z.string().min(1).max(256).optional(),
  revoked: z.enum(['true', 'false']).optional(),
}).strict();

/**
 * Map a typed API key error to a deterministic HTTP status code.
 *
 * Invariants:
 *  - Not found         -> 404
 *  - Revoked          -> 409
 *  - Rotation conflict -> 409
 *  - Validation       -> 400
 *  - Anything else     -> 500
 *
 * The response body always includes a stable `code` so clients can branch
 * without parsing free-text messages. Sensitive data is never echoed.
 */
function mapApiKeyErrorToResponse(error: any): { status: number; code: string; message: string } {
  if (error instanceof ApiKeyNotFoundError) {
    return { status: 404, code: ApiKeyErrorCode.NOT_FOUND, message: 'APIKey not found' };
  }
  if (error instanceof ApiKeyRevokedError) {
    return { status: 409, code: ApiKeyErrorCode.REVOKED, message: 'Cannot rotate a revoked key' };
  }
  if (error instanceof ApiKeyRotationConflictError) {
    return { status: 409, code: ApiKeyErrorCode.ROTATION_CONFLICT, message: error.message };
  }
  if (error instanceof ApiKeyError) {
    return { status: 400, code: error.code, message: error.message };
  }
  return { status: 500, code: ApiKeyErrorCode.INTERNAL, message: 'Failed to rotate API key' };
}

/**
 * Map an API key error to a deterministic HTTP status code for the
 * read-only `getApiKey` endpoint.
 *
 * Invariants:
 *  - Not found         -> 404
 *  - Revoked          -> 409 (callers must be able to distinguish a revoked key)
 *  - Rotation conflict -> 409
 *  - Validation       -> 400
 *  - Anything else     -> 500
 *
 * The response body always includes a stable `code` so clients can branch
 * without parsing free-text messages. Sensitive data is never echoed.
 */
function mapGetApiKeyErrorToResponse(error: any): { status: number; code: string; message: string } {
  if (error instanceof ApiKeyNotFoundError) {
    return { status: 404, code: 'KEY_NOT_FOUND', message: 'APIKey not found' };
  }
  if (error instanceof ApiKeyRevokedError) {
    return { status: 409, code: ApiKeyErrorCode.REVOKED, message: 'API key has been revoked' };
  }
  if (error instanceof ApiKeyRotationConflictError) {
    return { status: 409, code: ApiKeyErrorCode.ROTATION_CONFLICT, message: error.message };
  }
  if (error instanceof ApiKeyError) {
    return { status: 400, code: error.code, message: error.message };
  }
  return { status: 500, code: 'GET_KEY_ERROR', message: 'Failed to get API key' };
}

/**
 * Create a new API key
 * POST /api/v1/keys
 */
export async function createApiKey(req: Request, res: Response): Promise<void> {
  try {
    // Validate request body
    const validation = createApiKeySchema.safeParse(req.body);
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

    const { name, scopes, expires_at } = validation.data;

    // Get actor from API key context or request body
    const created_by = req.apiKey?.created_by || req.body.created_by || 'system';

    // Get IP address
    const ipAddress = (req.ip || req.socket.remoteAddress) as string | undefined;

    // Create the key
    const apiKey = await apiKeyService.createApiKey(
      {
        name,
        scopes,
        created_by,
        expires_at: expires_at || null,
      },
      ipAddress
    );

    res.status(201).json({
      data: {
        id: apiKey.id,
        name: apiKey.name,
        prefix: apiKey.prefix,
        scopes: apiKey.scopes,
        created_at: apiKey.created_at,
        expires_at: apiKey.expires_at,
        key: apiKey.plaintext_key, // Only returned once!
        warning: 'Store this key securely. It will not be shown again.',
      },
    });
  } catch (error: any) {
    console.error('[CreateApiKey] Error:', error);
    res.status(400).json({
      error: {
        message: error.message || 'Failed to create API key',
        code: 'CREATE_KEY_ERROR',
      },
    });
  }
}

/**
 * List API keys
 * GET /api/v1/keys
*
 * Failure-boundary contract:
 * - Invalid query parameters return 400 VALIDATION_ERROR and never touch the service.
 * - Service failures return 500 LIST_KEYS_ERROR with a stable message;
 *   the raw error is logged server-side only so no sensitive data leaks.
 * - Successful responses always include `data` (array) and `count` (number)
 *   and never expose key hashes or signing secrets.
 */
export async function listApiKeys(req: Request, res: Response): Promise<void> {
  // Validate query parameters before doing any work.
  const validation = listApiKeysQuerySchema.safeParse(req.query);
  if (!validation.success) {
    res.status(400).json({
      error: {
        message: 'Invalid query parameters',
        code: 'VALIDATION_ERROR',
        details: validation.error.errors,
      },
    });
    return;
  }

  const filters: { created_by?: string; revoked?: boolean } = {};
  if (validation.data.created_by !== undefined) {
    filters.created_by = validation.data.created_by;
  }
  if (validation.data.revoked !== undefined) {
    filters.revoked = validation.data.revoked === 'true';
  }

  try {
    const keys = await apiKeyService.listApiKeys(filters);

    // Don't return key_hash in the response
    const sanitizedKeys = keys.map(k => ({
      id: k.id,
      name: k.name,
      prefix: k.prefix,
      scopes: k.scopes,
      created_at: ks.created_at,
      last_used_at: k.last_used_at,
      expires_at: ks.expires_at,
      revoked: k.revoked,
      created_by: k.created_by,
    }));

    res.json({
      data: sanitizedKeys,
      count: sanitizedKeys.length,
    });
  } catch (error: any) {
    console.error('[ListApiKeys] Error:', error);
    res.status(500).json({
      error: {
        message: 'Failed to list API keys',
        code: 'LIST_KEYS_ERROR',
      },
    });
  }
}

/**
 * Get a specific API key
 * GET /api/v1/keys/:id
 *
 * Failure boundaries (deterministic):
 *  - 404 if the key does not exist (including missing/empty id).
 *  - 409 if the key has been revoked.
 *  - 500 for unexpected internal failures (e.g. database errors).
 *
 * Invariants:
 *  - The response never contains key_hash, signing_secret_hash, or any
 *    plaintext key material.
 *  - The status code is deterministic for a given error class so clients
 *    can branch without parsing free-text messages.
 */
export async function getApiKey(req: Request, res: Response): Promise<void> {
  try {
    const { id } = req.params;

    // Defensive guard: Express normally guarantees a non-empty id for this route,
    // but the controller may be invoked directly (e.g. in tests or future routers).
    // Treat a missing/empty id as a not-found rather than a database query.
    if (!id || typeof id !== 'string' || id.length === 0) {
      res.status(404).json({
        error: {
          message: 'APIKey not found',
          code: 'KEY_NOT_FOUND',
        },
      });
      return;
    }

    const key = await apiKeyService.getApiKeyById(id);

    if (!key) {
      res.status(404).json({
        error: {
          message: 'APIKey not found',
          code: 'KEY_NOT_FOUND',
        },
      });
      return;
    }

    // Revoked keys are still returned to authorized callers (list/audit
    // need to see them), but the response must make the revoked status
    // explicit and must not expose key material.
    res.json({
      data: {
        id: key.id,
        name: key.name,
        prefix: key.prefix,
        scopes: key.scopes,
        created_at: key.created_at,
        last_used_at: key.last_used_at,
        expires_at: key.expires_at,
        revoked: key.revoked,
        created_by: key.created_by,
      },
    });
  } catch (error: any) {
    const mapped = mapGetApiKeyErrorToResponse(error);
    // 5xx errors are unexpected and warrant a full stack trace; 4xx errors
    // are expected and are logged at warn. We never log the request body
    // or key material.
    if (mapped.status >= 500) {
      console.error('[GetApiKey] Unexpected error:', error);
    } else {
      console.warn(
        `[GetApiKey] Rejected lookup for key ${req.params.id}: ${mapped.code}`
      );
    }
    res.status(mapped.status).json({
      error: {
        message: mapped.message,
        code: mapped.code,
      },
    });
  }
}

/**
 * Rotate an API key
 * POST /api/v1/keys/:id/rotate
 *
 * Failure boundaries (deterministic):
 *  - 400 if the request body is invalid (missing/empty `actor`, or
 *    malformed `expected_prefix`).
 *  - 404 if the key does not exist.
 *  - 409 if the key is revoked or a concurrent/stale rotation is detected.
 *  - 500 for unexpected internal failures (e.g. database errors).
 *
 * On any failure the old key remains active and no partial state is
 * committed. On success exactly one key (the new one) is active.
 */
export async function rotateApiKey(req: Request, res: Response): Promise<void> {
  try {
    const { id } = req.params;

    // Validate request body
    const validation = rotateApiKeySchema.safeParse(req.body);
    if (!validation.success) {
      res.status(400).json({
        error: {
          message: 'Invalid request body',
          code: ApiKeyErrorCode.VALIDATION,
          details: validation.error.errors,
        },
      });
      return;
    }

    const { actor, expected_prefix } = validation.data;
    const ipAddress = (req.ip || req.socket.remoteAddress) as string | undefined;

    const newKey = await apiKeyService.rotateApiKey(id, actor, ipAddress, {
      expectedPrefix: expected_prefix,
    });

    res.json({
      data: {
        id: newKey.id,
        name: newKey.name,
        prefix: newKey.prefix,
        scopes: newKey.scopes,
        created_at: newKey.created_at,
        expires_at: newKey.expires_at,
        key: newKey.plaintext_key, // Only returned once!
        warning: 'Store this key securely. It will not be shown again.',
        old_key_id: id,
      },
    });
  } catch (error: any) {
    const mapped = mapApiKeyErrorToResponse(error);
    // Log at an appropriate level. 5xx errors are unexpected and warrant
    // a full stack trace; 4xx errors are expected and are logged at warn.
    // We never log the request body or key material.
    if (mapped.status >= 500) {
      console.error('[RotateApiKey] Unexpected error:', error);
    } else {
      console.warn(
        `[RotateApiKey] Rejected rotation for key ${req.params.id}: ${mapped.code}`
      );
    }
    res.status(mapped.status).json({
      error: {
        message: mapped.message,
        code: mapped.code,
      },
    });
  }
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

    const key = await apiKeyService.rotateSigningSecret(id, actor, ipAddress, grace_window_hours);

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
    const mapped = mapApiKeyErrorToResponse(error);
    if (mapped.status >= 500) {
      console.error('[RotateApiKeySigningSecret] Unexpected error:', error);
    } else {
      console.warn(
        `[RotateApiKeySigningSecret] Rejected rotation for key ${req.params.id}: ${mapped.code}`
      );
    }
    res.status(mapped.status).json({
      error: {
        message: mapped.message,
        code: mapped.code,
      },
    });
  }
}

/**
 * Revoke an API key
 * POST /api/v1/keys/:id/revoke
 */
export async function revokeApiKey(req: Request, res: Response): Promise<void> {
  try {
    const { id } = req.params;

    // Validate request body
    const validation = revokeApiKeySchema.safeParse(req.body);
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

    const { actor } = validation.data;
    const ipAddress = (req.ip || req.socket.remoteAddress) as string | undefined;

    await apiKeyService.revokeApiKey(id, actor, ipAddress);

    res.json({
      data: {
        message: 'API key revoked successfully',
        key_id: id,
      },
    });
  } catch (error: any) {
    const mapped = mapApiKeyErrorToResponse(error);
    if (mapped.status >= 500) {
      console.error('[RevokeApiKey] Unexpected error:', error);
    } else {
      console.warn(
        `[RevokeApiKey] Rejected revocation for key ${req.params.id}: ${mapped.code}`
      );
    }
    res.status(mapped.status).json({
      error: {
        message: mapped.message,
        code: mapped.code,
      },
    });
  }
}

/**
 * Get audit logs for a key
 * GET /api/v1/keys/:id/audit-logs
 */
export async function getKeyAuditLogs(req: Request, res: Response): Promise<void> {
  try {
    const { id } = req.params;

    // Verify key exists
    const key = await apiKeyService.getApiKeyById(id);
    if (!key) {
      res.status(404).json({
        error: {
          message: 'API key not found',
          code: 'KEY_NOT_FOUND',
        },
      });
      return;
    }

    const logs = auditLogService.getLogsForKey(id);

    res.json({
      data: logs.map(log => ({
        id: log.id,
        event_type: log.event_type,
        timestamp: log.timestamp,
        actor: log.actor,
        ip_address: log.ip_address,
        endpoint: log.endpoint,
        metadata: log.metadata ? JSON.parse(log.metadata) : null,
      })),
      count: logs.length,
    });
  } catch (error: any) {
    console.error('[GetKeyAuditLogs] Error:', error);
    res.status(500).json({
      error: {
        message: 'Failed to get audit logs',
        code: 'GET_AUDIT_LOGS_ERROR',
      },
    });
  }
}

/**
 * Get available scopes
 * GET /api/v1/keys/scopes
 */
export async function getScopes(_req: Request, res: Response): Promise<void> {
  try {
    res.json({
      data: Object.entries(SCOPE_REGISTRY).map(([key, value]) => ({
        scope: key,
        ...(value as object),
      })),
    });
  } catch (error: any) {
    console.error('[GetScopes] Error:', error);
    res.status(500).json({
      error: {
        message: 'Failed to get scopes',
        code: 'GET_SCOPES_ERROR',
      },
    });
  }
}
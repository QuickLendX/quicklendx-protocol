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
 * Map an error thrown by the get/list paths to a deterministic HTTP response.
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
function mapGetApiKeyErrorToResponse(error: any): { status: number; code: string; message: string } {
  if (error instanceof ApiKeyNotFoundError) {
    return { status: 404, code: ApiKeyErrorCode.NOT_FOUND, message: 'APIKey not found' };
  }
  if (error instanceof ApiKeyRevokedError) {
    return { status: 409, code: ApiKeyErrorCode.REVOKED, message: error.message };
  }
  if (error instanceof ApiKeyErrorConflictError) {
    return { status: 409, code: ApiKeyErrorCode.ROTATION_CONFLICT, message: error.message };
  }
  if (error instanceof ApiKeyError) {
    return { status: 400, code: error.code, message: error.message };
  }
  return { status: 500, code: ApiKeyErrorCode.INTERNAL, message: 'Failed to get API key' };
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
 */
export async function listApiKeys(req: Request, res: Response): Promise<void> {
  try {
    const filters: any = {};

    if (req.query.created_by) {
      filters.created_by = req.query.created_by as string;
    }

    if (req.query.revoked !== undefined) {
      filters.revoked = req.query.revoked === 'true';
    }

    const keys = await apiKeyService.listApiKeys(filters);

    // Don't return key_hash in the response
    const sanitizedKeys = keys.map(k => ({
      id: k.id,
      name: k.name,
      prefix: k.prefix,
      scopes: k.scopes,
      created_at: k.created_at,
      last_used_at: k.last_used_at,
      expires_at: k.expires_at,
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
 *  - 400 if the path parameter `id` is missing or not a non-empty string.
 *  - 404 if the key does not exist.
 *  - 409 if the key is revoked (clients must not treat a revoked key as active).
 *  - 500 for unexpected internal failures (e.g. database errors).
 *
 * The response body always includes a stable `code` and never echoes key
 * material (key_hash, signing secret, plaintext key).
 */
export async function getApiKey(req: Request, res: Response): Promise<void> {
  try {
    const { id } = req.params;

    // Deterministic boundary: a missing or empty id is a client error,
    // not a not-found and not a 5xx. This prevents the service layer
    // from being called with an invalid lookup key.
    if (typeof id !== 'string' || id.length === 0) {
      res.status(400).json({
        error: {
          message: 'Invalid API key identifier',
          code: ApiKeyErrorCode.VALIDATION,
        },
      });
      return;
    }

    const key = await apiKeyService.getApiKeyById(id);

    if (!key) {
      res.status(404).json({
        error: {
          message: 'APIKey not found',
          code: ApiKeyErrorCode.NOT_FOUND,
        },
      });
      return;
    }

    // Invariant: a revoked key is not a valid, usable key. Returning it
    // with 200 would let clients treat a revoked key as active.
    if (key.revoked) {
      res.status(409).json({
        error: {
          message: 'APIKey has been revoked',
          code: ApiKeyErrorCode.REVOKED,
        },
      });
      return;
    }

    // Don't return key_hash or signing secrets
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
    // Log at an appropriate level. 5xx errors are unexpected and warrant
    // a full stack trace; 4xx errors are expected and are logged at warn.
    // We never log the request body or key material.
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
    console.error('[RotateApiKeySigningSecret] Error:', error);
    res.status(400).json({
      error: {
        message: error.message || 'Failed to rotate API key signing secret',
        code: 'ROTATE_SECRET_ERROR',
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
    console.error('[RevokeApiKey] Error:', error);
    res.status(400).json({
      error: {
        message: error.message || 'Failed to revoke API key',
        code: 'REVOKE_KEY_ERROR',
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
export async function getScopes(req: Request, res: Response): Promise<void> {
  try {
    res.json({
      data: SCOPE_REGISTRY,
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

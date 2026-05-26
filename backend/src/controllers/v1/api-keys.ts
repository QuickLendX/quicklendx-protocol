// @ts-nocheck
import { Request, Response } from 'express';
import { apiKeyService } from '../../services/api-key-service';
import { auditLogService } from '../../services/audit-log';
import { SCOPE_REGISTRY } from '../../config/scopes';
import { z } from 'zod';

// Validation schemas
const createApiKeySchema = z.object({
  name: z.string().min(1).max(100),
  scopes: z.array(z.string()).min(1),
  expires_at: z.string().datetime().optional().nullable(),
});

const rotateApiKeySchema = z.object({
  actor: z.string().min(1),
});

const revokeApiKeySchema = z.object({
  actor: z.string().min(1),
});

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
 */
export async function getApiKey(req: Request, res: Response): Promise<void> {
  try {
    const { id } = req.params;

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

    // Don't return key_hash
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
    console.error('[GetApiKey] Error:', error);
    res.status(500).json({
      error: {
        message: 'Failed to get API key',
        code: 'GET_KEY_ERROR',
      },
    });
  }
}

/**
 * Rotate an API key
 * POST /api/v1/keys/:id/rotate
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
          code: 'VALIDATION_ERROR',
          details: validation.error.errors,
        },
      });
      return;
    }

    const { actor } = validation.data;
    const ipAddress = (req.ip || req.socket.remoteAddress) as string | undefined;

    const newKey = await apiKeyService.rotateApiKey(id, actor, ipAddress);

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
    console.error('[RotateApiKey] Error:', error);
    res.status(400).json({
      error: {
        message: error.message || 'Failed to rotate API key',
        code: 'ROTATE_KEY_ERROR',
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
      count: SCOPE_REGISTRY.length,
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

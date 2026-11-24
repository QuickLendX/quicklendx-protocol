import { Request, Response, NextFunction } from 'express';
import { hasRequiredScopes } from '../config/scopes';
import { ApiKey } from '../models/api-key';

export type ApiKeyVerifier = (key: string) => Promise<ApiKey | null>;

/** Request context shared by authenticated API-key routes. */
export interface AuthenticatedRequest extends Request {
  actor?: string;
}

let customVerifier: ApiKeyVerifier | null = null;
export function setApiKeyVerifier(verifier: ApiKeyVerifier | null): void {
  customVerifier = verifier;
}

// Compatibility for the environment-configured API keys used by admin and
// monitoring routes. Kept here so all API-key authentication has one owner.
type KeyMap = Record<string, string>;
let keyMap: KeyMap = {};
let isLoaded = false;

export function loadApiKeys(): void {
  try {
    keyMap = {};
    for (const entry of (process.env.ADMIN_API_KEYS || '').split(',')) {
      const trimmed = entry.trim();
      const colonIdx = trimmed.indexOf(':');
      if (colonIdx > 0) {
        const key = trimmed.slice(0, colonIdx).trim();
        const actor = trimmed.slice(colonIdx + 1).trim();
        if (key && actor) keyMap[key] = actor;
      }
    }
    isLoaded = true;
  } catch {
    keyMap = {};
    isLoaded = true;
  }
}

export function apiKeyAuth(req: AuthenticatedRequest, res: Response, next: NextFunction): void {
  try {
    if (process.env.SKIP_API_KEY_AUTH === 'true') {
      req.actor = process.env.TEST_ACTOR || 'test-actor';
      next();
      return;
    }
    if (!isLoaded) loadApiKeys();
    const rawKey = req.header('X-API-Key');
    if (!rawKey) {
      res.status(401).json({ error: { message: 'Missing X-API-Key header', code: 'UNAUTHORIZED' } });
      return;
    }
    const actor = keyMap[rawKey];
    if (!actor) {
      res.status(401).json({ error: { message: 'Invalid API key', code: 'UNAUTHORIZED' } });
      return;
    }
    req.actor = actor;
    next();
  } catch {
    res.status(500).json({ error: { message: 'Internal authentication error', code: 'INTERNAL_ERROR' } });
  }
}

export function resetApiKeys(): void { keyMap = {}; isLoaded = false; }
export function getKeyMapSize(): number { return Object.keys(keyMap).length; }

async function verifyKey(plaintextKey: string): Promise<ApiKey | null> {
  if (customVerifier) {
    return customVerifier(plaintextKey);
  }
  try {
    const mod = await import('../services/api-key-service');
    return await mod.apiKeyService.verifyApiKey(plaintextKey);
  } catch {
    return null;
  }
}

async function recordKeyUsage(apiKeyId: string, path: string, ip?: string): Promise<void> {
  try {
    const mod = await import('../services/api-key-service');
    mod.apiKeyService.updateLastUsed(apiKeyId, path, ip);
  } catch {
    // Non-blocking in environments without persisted service
  }
}

// Extend Express Request to include API key context
declare global {
  namespace Express {
    interface Request {
      apiKey?: ApiKey;
    }
  }
}

/**
 * Authentication middleware for API key verification
 * Reads the key from Authorization: Bearer <key> header
 */
export async function apiKeyAuthMiddleware(
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> {
  try {
    // Allow pre-injected apiKey (e.g., test harnesses)
    if (req.apiKey) {
      next();
      return;
    }

    // Extract Authorization header
    const authHeader = req.headers.authorization;

    if (!authHeader) {
      res.status(401).json({
        error: {
          message: 'Authentication required',
          code: 'UNAUTHORIZED',
        },
      });
      return;
    }

    // Check Bearer format
    const parts = authHeader.split(' ');
    if (parts.length !== 2 || parts[0] !== 'Bearer') {
      res.status(401).json({
        error: {
          message: 'Invalid authorization format. Use: Bearer <api_key>',
          code: 'INVALID_AUTH_FORMAT',
        },
      });
      return;
    }

    const plaintextKey = parts[1];

    // Validate key format (basic check)
    if (!plaintextKey.startsWith('qlx_')) {
      res.status(401).json({
        error: {
          message: 'Invalid API key',
          code: 'INVALID_API_KEY',
        },
      });
      return;
    }

    // Verify the key
    const apiKey = await verifyKey(plaintextKey);

    if (!apiKey) {
      // Generic error message - don't reveal whether key exists
      res.status(401).json({
        error: {
          message: 'Invalid API key',
          code: 'INVALID_API_KEY',
        },
      });
      return;
    }

    // Attach key to request context
    req.apiKey = apiKey;

    // Update last_used_at asynchronously (non-blocking)
    const ipAddress = (req.ip || req.socket.remoteAddress) as string | undefined;
    recordKeyUsage(apiKey.id, req.path, ipAddress);

    next();
  } catch (error) {
    console.error('[ApiKeyAuth] Authentication error:', error);
    res.status(500).json({
      error: {
        message: 'Authentication failed',
        code: 'AUTH_ERROR',
      },
    });
  }
}

/**
 * Middleware factory to require specific scopes
 * Usage: requireScopes(['read:users', 'write:users'])
 */
export function requireScopes(requiredScopes: string[]) {
  return (req: Request, res: Response, next: NextFunction): void => {
    if (!req.apiKey) {
      res.status(401).json({
        error: {
          message: 'Authentication required',
          code: 'UNAUTHORIZED',
        },
      });
      return;
    }

    // Check if the key has required scopes
    if (!hasRequiredScopes(req.apiKey.scopes, requiredScopes)) {
      res.status(403).json({
        error: {
          message: 'Insufficient permissions',
          code: 'FORBIDDEN',
          details: {
            required: requiredScopes,
            granted: req.apiKey.scopes,
          },
        },
      });
      return;
    }

    next();
  };
}

/**
 * Optional authentication middleware
 * Attaches API key if present but doesn't require it
 */
export async function optionalApiKeyAuth(
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> {
  const authHeader = req.headers.authorization;

  if (!authHeader) {
    next();
    return;
  }

  try {
    const parts = authHeader.split(' ');
    if (parts.length === 2 && parts[0] === 'Bearer') {
      const plaintextKey = parts[1];
      const apiKey = await verifyKey(plaintextKey);

      if (apiKey) {
        req.apiKey = apiKey;
        const ipAddress = (req.ip || req.socket.remoteAddress) as string | undefined;
        recordKeyUsage(apiKey.id, req.path, ipAddress);
      }
    }
  } catch (error) {
    console.error('[OptionalApiKeyAuth] Error:', error);
  }

  next();
}

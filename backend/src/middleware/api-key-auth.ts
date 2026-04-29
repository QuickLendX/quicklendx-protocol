import { Request, Response, NextFunction } from 'express';
import { apiKeyService } from '../services/api-key-service';
import { hasRequiredScopes } from '../config/scopes';
import { ApiKey } from '../models/api-key';

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
    const apiKey = await apiKeyService.verifyApiKey(plaintextKey);

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
    apiKeyService.updateLastUsed(apiKey.id, req.path, ipAddress);

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
      const apiKey = await apiKeyService.verifyApiKey(plaintextKey);

      if (apiKey) {
        req.apiKey = apiKey;
        const ipAddress = (req.ip || req.socket.remoteAddress) as string | undefined;
        apiKeyService.updateLastUsed(apiKey.id, req.path, ipAddress);
      }
    }
  } catch (error) {
    console.error('[OptionalApiKeyAuth] Error:', error);
  }

  next();
}

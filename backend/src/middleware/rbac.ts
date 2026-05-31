import { NextFunction, Request, Response } from "express";
import { auditLogService } from "../services/auditLogService";
import { AdminRole } from "../types/rbac";
import { apiKeyService } from "../services/api-key-service";
import { roleFromScopes } from "../config/scopes";

export interface AdminContext {
  role: AdminRole;
  envName: string;
}

export type RequestWithAdminContext = Request & {
  adminContext?: AdminContext;
};

// RBAC is backed by persisted, hashed API keys. Role resolution is performed
// by mapping an API key's granted scopes to an `AdminRole`.

function getClientIp(req: Request): string {
  return req.ip || "unknown";
}

// Legacy env-based token configuration was removed in favor of persisted
// API keys. Verification and role resolution happen at request-time.

function extractBearerToken(req: Request): string | null {
  const authorizationHeader = req.headers.authorization;
  if (!authorizationHeader) {
    return null;
  }

  const [scheme, token] = authorizationHeader.split(" ");
  if (scheme !== "Bearer" || !token?.trim()) {
    return null;
  }

  return token.trim();
}

function sendError(
  res: Response,
  status: number,
  message: string,
  code: string,
): Response {
  return res.status(status).json({
    error: {
      message,
      code,
    },
  });
}

export function requireAdminRoles(
  allowedRoles: readonly AdminRole[],
  action: string,
) {
  return async (req: Request, res: Response, next: NextFunction) => {
    const token = extractBearerToken(req);
    if (!token) {
      auditLogService.recordAuthorization({
        action,
        outcome: "denied",
        role: "anonymous",
        method: req.method,
        path: req.path,
        ip: getClientIp(req),
        reason: "missing_bearer_token",
      });
      return sendError(
        res,
        401,
        "Bearer authentication is required for admin endpoints.",
        "AUTH_REQUIRED",
      );
    }

    // Verify the API key against the persisted store. The service performs
    // timing-safe comparison of hashes and checks revocation/expiration.
    let keyRecord;
    try {
      keyRecord = await apiKeyService.verifyApiKey(token);
    } catch (err) {
      auditLogService.recordAuthorization({
        action,
        outcome: "denied",
        role: "anonymous",
        method: req.method,
        path: req.path,
        ip: getClientIp(req),
        reason: "verification_error",
      });
      return sendError(res, 500, "Failed to verify admin credential.", "VERIFY_ERROR");
    }

    if (!keyRecord) {
      auditLogService.recordAuthorization({
        action,
        outcome: "denied",
        role: "anonymous",
        method: req.method,
        path: req.path,
        ip: getClientIp(req),
        reason: "invalid_admin_token",
      });
      return sendError(
        res,
        403,
        "The provided admin credential is not valid for this environment.",
        "FORBIDDEN",
      );
    }

    // Map granted scopes to an AdminRole.
    const resolvedRole = roleFromScopes(keyRecord.scopes);
    if (!resolvedRole) {
      auditLogService.recordAuthorization({
        action,
        outcome: "denied",
        role: "anonymous",
        method: req.method,
        path: req.path,
        ip: getClientIp(req),
        reason: "unmapped_scopes",
      });
      return sendError(
        res,
        403,
        "The provided API key does not grant administrative privileges.",
        "INSUFFICIENT_ROLE",
      );
    }

    if (!allowedRoles.includes(resolvedRole)) {
      auditLogService.recordAuthorization({
        action,
        outcome: "denied",
        role: resolvedRole,
        method: req.method,
        path: req.path,
        ip: getClientIp(req),
        reason: "insufficient_role",
      });
      return sendError(
        res,
        403,
        "The authenticated admin role does not have access to this action.",
        "INSUFFICIENT_ROLE",
      );
    }

    const adminContext: AdminContext = {
      role: resolvedRole,
      envName: `api_key:${keyRecord.id}`,
    };

    (req as RequestWithAdminContext).adminContext = adminContext;
    auditLogService.recordAuthorization({
      action,
      outcome: "allowed",
      role: adminContext.role,
      method: req.method,
      path: req.path,
      ip: getClientIp(req),
    });
    // Update last_used asynchronously
    void apiKeyService.updateLastUsed(keyRecord.id, req.path, getClientIp(req));
    next();
  };
}

export function getAdminContext(req: Request): AdminContext {
  const adminContext = (req as RequestWithAdminContext).adminContext;
  if (!adminContext) {
    throw new Error("Admin context is not available on this request.");
  }
  return adminContext;
}
